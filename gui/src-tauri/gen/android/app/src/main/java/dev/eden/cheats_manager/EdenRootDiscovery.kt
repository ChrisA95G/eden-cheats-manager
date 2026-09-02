package dev.eden.cheats_manager

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.contract.ActivityResultContracts
import org.json.JSONArray
import org.json.JSONObject

/** Read-only access used to inspect Eden's configuration and content layout. */
internal class EdenRootDiscovery(private val activity: MainActivity) {
    companion object {
        private const val SAF_PREFS = "eden_saf"
        private const val PREF_EDEN_ROOT_URI = "eden_root_uri"
        private const val EDEN_DOCUMENTS_AUTHORITY = "dev.eden.eden_emulator.user"
        private const val MAX_PROBE_ENTRIES = 200

        private val PROBE_PATHS = listOf(
            "",
            "config",
            "nand",
            "nand/user",
            "nand/user/Contents",
            "nand/user/Contents/registered",
            "sdmc"
        )
    }

    private data class SafDocument(
        val uri: Uri,
        val name: String,
        val directory: Boolean
    )

    private data class AccessStatus(
        val selected: Boolean,
        val validLocation: Boolean,
        val readPermission: Boolean,
        val readable: Boolean,
        val message: String
    ) {
        val ready: Boolean
            get() = validLocation && readPermission && readable

        fun toJson(): JSONObject = JSONObject()
            .put("selected", selected)
            .put("validLocation", validLocation)
            .put("readPermission", readPermission)
            .put("readable", readable)
            .put("ready", ready)
            .put("message", message)
    }

    private val contentResolver
        get() = activity.contentResolver

    private val edenRootPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            if (uri == null) {
                android.util.Log.i("CheatsManager", "Eden root selection cancelled")
                return@registerForActivityResult
            }
            if (!isEdenRoot(uri)) {
                android.util.Log.e(
                    "CheatsManager",
                    "Selected directory is not Eden's provider root"
                )
                return@registerForActivityResult
            }

            try {
                val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION
                val previousUri = preferences()
                    .getString(PREF_EDEN_ROOT_URI, null)
                    ?.let(Uri::parse)

                contentResolver.takePersistableUriPermission(uri, flags)
                val saved = preferences()
                    .edit()
                    .putString(PREF_EDEN_ROOT_URI, uri.toString())
                    .commit()
                if (!saved) {
                    if (previousUri != uri) {
                        contentResolver.releasePersistableUriPermission(uri, flags)
                    }
                    throw IllegalStateException("Could not save Eden root permission")
                }

                if (previousUri != null && previousUri != uri) {
                    try {
                        contentResolver.releasePersistableUriPermission(previousUri, flags)
                    } catch (error: SecurityException) {
                        android.util.Log.w(
                            "CheatsManager",
                            "Could not release the previous Eden root permission",
                            error
                        )
                    }
                }

                android.util.Log.i("CheatsManager", "Eden root read permission saved")
            } catch (error: Exception) {
                android.util.Log.e(
                    "CheatsManager",
                    "Could not persist Eden root permission",
                    error
                )
            }
        }

    fun selectRootDirectory() {
        activity.runOnUiThread {
            edenRootPicker.launch(null)
        }
    }

    fun inspectInstallation(): String {
        val status = accessStatus()
        val result = JSONObject()
            .put("status", status.toJson())
            .put("configIni", JSONObject.NULL)
            .put("configError", JSONObject.NULL)
            .put("directories", JSONArray())
        if (!status.ready) {
            return result.toString()
        }

        val treeUri = rootTreeUri()
        try {
            result.put("configIni", readTextFile(treeUri, "config/config.ini"))
        } catch (error: Exception) {
            result.put(
                "configError",
                error.message ?: error.javaClass.simpleName
            )
        }

        val directories = JSONArray()
        for (path in PROBE_PATHS) {
            directories.put(probeDirectory(treeUri, path))
        }
        result.put("directories", directories)
        return result.toString()
    }

    private fun accessStatus(): AccessStatus {
        val savedUri = preferences().getString(PREF_EDEN_ROOT_URI, null)
            ?: return AccessStatus(
                selected = false,
                validLocation = false,
                readPermission = false,
                readable = false,
                message = "Select Eden's top-level provider directory to inspect it."
            )
        val treeUri = Uri.parse(savedUri)
        if (!isEdenRoot(treeUri)) {
            return AccessStatus(
                selected = true,
                validLocation = false,
                readPermission = false,
                readable = false,
                message = "The saved directory is not Eden's provider root. Select it again."
            )
        }

        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        val readPermission = permission?.isReadPermission == true
        if (!readPermission) {
            return AccessStatus(
                selected = true,
                validLocation = true,
                readPermission = false,
                readable = false,
                message = "Android no longer grants read access. Select Eden's root again."
            )
        }

        val readable = try {
            queryChildren(rootDocumentUri(treeUri))
            true
        } catch (_: Exception) {
            false
        }
        return AccessStatus(
            selected = true,
            validLocation = true,
            readPermission = true,
            readable = readable,
            message = if (readable) {
                "Eden root access is ready."
            } else {
                "Eden's root could not be read. Select it again."
            }
        )
    }

    private fun isEdenRoot(uri: Uri): Boolean {
        if (uri.authority != EDEN_DOCUMENTS_AUTHORITY) return false
        return try {
            DocumentsContract.getTreeDocumentId(uri).trimEnd('/') == "root"
        } catch (_: IllegalArgumentException) {
            false
        }
    }

    private fun preferences() =
        activity.getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)

    private fun rootTreeUri(): Uri {
        val savedUri = preferences().getString(PREF_EDEN_ROOT_URI, null)
            ?: throw IllegalStateException("Select Eden's root first")
        val treeUri = Uri.parse(savedUri)
        if (!isEdenRoot(treeUri)) {
            throw IllegalStateException("Saved directory is not Eden's provider root")
        }
        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        if (permission?.isReadPermission != true) {
            throw IllegalStateException("Eden root permission is no longer available")
        }
        return treeUri
    }

    private fun rootDocumentUri(treeUri: Uri): Uri =
        DocumentsContract.buildDocumentUriUsingTree(
            treeUri,
            DocumentsContract.getTreeDocumentId(treeUri)
        )

    private fun readTextFile(treeUri: Uri, relativePath: String): String {
        val document = resolveDocument(treeUri, relativePath)
            ?: throw IllegalStateException("Eden file not found: $relativePath")
        if (document.directory) {
            throw IllegalStateException("Eden path is a directory: $relativePath")
        }
        val input = contentResolver.openInputStream(document.uri)
            ?: throw IllegalStateException("Could not open Eden file: $relativePath")
        return input.bufferedReader(Charsets.UTF_8).use { it.readText() }
    }

    private fun probeDirectory(treeUri: Uri, relativePath: String): JSONObject {
        val displayPath = relativePath.ifEmpty { "/" }
        return try {
            val directory = resolveDirectory(treeUri, relativePath)
                ?: return JSONObject()
                    .put("path", displayPath)
                    .put("exists", false)
                    .put("entries", JSONArray())
                    .put("truncated", false)
                    .put("error", JSONObject.NULL)
            val children = queryChildren(directory, MAX_PROBE_ENTRIES + 1)
            val truncated = children.size > MAX_PROBE_ENTRIES
            val entries = JSONArray()
            for (child in children.take(MAX_PROBE_ENTRIES)) {
                entries.put(
                    JSONObject()
                        .put("name", child.name)
                        .put("directory", child.directory)
                )
            }
            JSONObject()
                .put("path", displayPath)
                .put("exists", true)
                .put("entries", entries)
                .put("truncated", truncated)
                .put("error", JSONObject.NULL)
        } catch (error: Exception) {
            JSONObject()
                .put("path", displayPath)
                .put("exists", false)
                .put("entries", JSONArray())
                .put("truncated", false)
                .put("error", error.message ?: error.javaClass.simpleName)
        }
    }

    private fun pathSegments(relativePath: String): List<String> {
        if (relativePath.isBlank()) return emptyList()
        val segments = relativePath.split('/')
        if (segments.any { it.isBlank() || it == "." || it == ".." }) {
            throw IllegalArgumentException("Invalid relative path: $relativePath")
        }
        return segments
    }

    private fun queryChildren(
        parentUri: Uri,
        limit: Int = Int.MAX_VALUE
    ): List<SafDocument> {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(
            parentUri,
            DocumentsContract.getDocumentId(parentUri)
        )
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        val cursor = contentResolver.query(childrenUri, projection, null, null, null)
            ?: throw IllegalStateException("Could not query Eden directory")
        val children = mutableListOf<SafDocument>()
        cursor.use {
            val idColumn = it.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID
            )
            val nameColumn = it.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_DISPLAY_NAME
            )
            val mimeColumn = it.getColumnIndexOrThrow(
                DocumentsContract.Document.COLUMN_MIME_TYPE
            )
            while (children.size < limit && it.moveToNext()) {
                val documentId = it.getString(idColumn)
                children.add(
                    SafDocument(
                        uri = DocumentsContract.buildDocumentUriUsingTree(
                            parentUri,
                            documentId
                        ),
                        name = it.getString(nameColumn),
                        directory = it.getString(mimeColumn) ==
                                DocumentsContract.Document.MIME_TYPE_DIR
                    )
                )
            }
        }
        return children.sortedBy { it.name.lowercase() }
    }

    private fun findChild(parentUri: Uri, name: String): SafDocument? =
        queryChildren(parentUri).firstOrNull { it.name == name }

    private fun resolveDirectory(treeUri: Uri, relativePath: String): Uri? {
        var current = rootDocumentUri(treeUri)
        for (segment in pathSegments(relativePath)) {
            val child = findChild(current, segment) ?: return null
            if (!child.directory) {
                throw IllegalStateException("Not an Eden directory: $segment")
            }
            current = child.uri
        }
        return current
    }

    private fun resolveDocument(treeUri: Uri, relativePath: String): SafDocument? {
        val segments = pathSegments(relativePath)
        if (segments.isEmpty()) {
            throw IllegalArgumentException("An Eden file path is required")
        }
        val parent = resolveDirectory(
            treeUri,
            segments.dropLast(1).joinToString("/")
        ) ?: return null
        return findChild(parent, segments.last())
    }
}
