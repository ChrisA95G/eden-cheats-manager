package dev.eden.cheats_manager

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.contract.ActivityResultContracts
import org.json.JSONArray
import org.json.JSONObject

/** Eden load-directory access through Eden's exported DocumentsProvider. */
internal class EdenSafStorage(private val activity: MainActivity) {
    companion object {
        private const val SAF_PREFS = "eden_saf"
        private const val PREF_EDEN_LOAD_URI = "eden_load_uri"
    }

    private data class SafDocument(
        val uri: Uri,
        val name: String,
        val directory: Boolean
    )

    private val contentResolver
        get() = activity.contentResolver

    @Volatile
    private var lastSelectionError = ""

    private val edenLoadPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            lastSelectionError = ""
            if (uri == null) {
                android.util.Log.i("CheatsManager", "SAF selection cancelled")
                return@registerForActivityResult
            }

            try {
                val rootUri = resolveLoadDirectory(uri)
                queryChildren(rootUri)
                check(documentSupportsCreate(rootUri)) {
                    "The selected load folder does not allow cheat files to be created."
                }
                val flags =
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                val previousUri = preferences()
                    .getString(PREF_EDEN_LOAD_URI, null)
                    ?.let(Uri::parse)

                contentResolver.takePersistableUriPermission(uri, flags)
                val saved = preferences()
                    .edit()
                    .putString(PREF_EDEN_LOAD_URI, uri.toString())
                    .commit()
                if (!saved) {
                    if (previousUri != uri) {
                        contentResolver.releasePersistableUriPermission(uri, flags)
                    }
                    throw IllegalStateException("Could not save Eden SAF directory")
                }

                if (previousUri != null && previousUri != uri) {
                    try {
                        contentResolver.releasePersistableUriPermission(previousUri, flags)
                    } catch (error: SecurityException) {
                        android.util.Log.w(
                            "CheatsManager",
                            "Could not release the previous Eden SAF permission",
                            error
                        )
                    }
                }

                android.util.Log.i(
                    "CheatsManager",
                    "Eden SAF directory permission saved"
                )
            } catch (error: Exception) {
                lastSelectionError = error.message ?: "Could not save access to the selected load folder."
                android.util.Log.e(
                    "CheatsManager",
                    "Could not persist Eden SAF permission",
                    error
                )
            }
        }

    fun selectLoadDirectory() {
        activity.runOnUiThread {
            edenLoadPicker.launch(null)
        }
    }

    fun getAccessStatus(): String {
        val savedUri = preferences().getString(PREF_EDEN_LOAD_URI, null)
        if (savedUri == null) {
            return accessStatusJson(
                selected = false,
                validLocation = false,
                readPermission = false,
                writePermission = false,
                readable = false,
                writable = false,
                message = "Select Eden → load to continue."
            )
        }

        val treeUri = try {
            Uri.parse(savedUri)
        } catch (_: Exception) {
            return accessStatusJson(
                selected = true,
                validLocation = false,
                readPermission = false,
                writePermission = false,
                readable = false,
                writable = false,
                message = "The saved Eden directory is invalid. Select it again."
            )
        }
        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        val readPermission = permission?.isReadPermission == true
        val writePermission = permission?.isWritePermission == true
        if (!readPermission || !writePermission) {
            return accessStatusJson(
                selected = true,
                validLocation = false,
                readPermission = readPermission,
                writePermission = writePermission,
                readable = false,
                writable = false,
                message = "Android no longer grants full access. Select Eden → load again."
            )
        }

        val rootUri = try {
            resolveLoadDirectory(treeUri)
        } catch (error: Exception) {
            return accessStatusJson(
                selected = true,
                validLocation = false,
                readPermission = readPermission,
                writePermission = writePermission,
                readable = false,
                writable = false,
                message = error.message ?: "The saved load folder is unavailable. Select it again."
            )
        }
        val readable = try {
            queryChildren(rootUri)
            true
        } catch (_: Exception) {
            false
        }
        val writable = readable && documentSupportsCreate(rootUri)
        val message = when {
            !readable -> "Eden's load directory could not be read. Select it again."
            !writable -> "Eden's load directory does not allow cheat files to be created."
            else -> "Eden load access is ready."
        }
        return accessStatusJson(
            selected = true,
            validLocation = true,
            readPermission = true,
            writePermission = true,
            readable = readable,
            writable = writable,
            message = message
        )
    }

    fun listDirectory(relativePath: String): String {
        return try {
            val directory = resolveDirectory(relativePath, false) ?: return "[]"
            val result = JSONArray()
            for (child in queryChildren(directory)) {
                result.put(
                    JSONObject()
                        .put("name", child.name)
                        .put("directory", child.directory)
                )
            }
            result.toString()
        } catch (error: Exception) {
            safError(error)
        }
    }

    fun writeTextFile(relativePath: String, content: String): String = safStatus {
        val segments = pathSegments(relativePath)
        if (segments.isEmpty()) {
            throw IllegalArgumentException("A file path is required")
        }
        val parent = resolveDirectory(
            segments.dropLast(1).joinToString("/"),
            true
        ) ?: throw IllegalStateException("Could not resolve parent directory")
        val filename = segments.last()
        val existing = findChild(parent, filename)
        if (existing?.directory == true) {
            throw IllegalStateException("Path is a directory: $relativePath")
        }
        val fileUri = existing?.uri ?: DocumentsContract.createDocument(
            contentResolver,
            parent,
            "text/plain",
            filename
        ) ?: throw IllegalStateException("Could not create file: $filename")
        val output = contentResolver.openOutputStream(fileUri, "wt")
            ?: throw IllegalStateException("Could not open file: $filename")
        output.use {
            it.write(content.toByteArray(Charsets.UTF_8))
        }
    }

    fun deleteFile(relativePath: String): String = safStatus {
        val document = resolveDocument(relativePath) ?: return@safStatus
        if (document.directory) {
            throw IllegalStateException("Path is a directory: $relativePath")
        }
        if (!DocumentsContract.deleteDocument(contentResolver, document.uri)) {
            throw IllegalStateException("Could not delete file: $relativePath")
        }
    }

    fun removeEmptyDirectory(relativePath: String): String = safStatus {
        val document = resolveDocument(relativePath) ?: return@safStatus
        if (!document.directory) {
            throw IllegalStateException("Path is not a directory: $relativePath")
        }
        if (queryChildren(document.uri).isEmpty() &&
            !DocumentsContract.deleteDocument(contentResolver, document.uri)) {
            throw IllegalStateException("Could not delete directory: $relativePath")
        }
    }

    private fun preferences() =
        activity.getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)

    private fun loadRootUri(): Uri {
        val savedUri = preferences().getString(PREF_EDEN_LOAD_URI, null)
            ?: throw IllegalStateException("Select Eden's load directory first")
        val treeUri = Uri.parse(savedUri)
        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        if (permission?.isReadPermission != true || permission.isWritePermission != true) {
            throw IllegalStateException("Eden load directory permission is no longer available")
        }
        return resolveLoadDirectory(treeUri)
    }

    private fun resolveLoadDirectory(treeUri: Uri): Uri {
        require(treeUri.scheme == "content" && !treeUri.authority.isNullOrBlank() &&
            DocumentsContract.isTreeUri(treeUri)) {
            "Select your Eden installation's load folder using the folder picker."
        }
        val documentId = DocumentsContract.getTreeDocumentId(treeUri)
        require(documentId.isNotBlank()) { "The selected folder has no document ID." }
        val rootUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE
        )
        val cursor = contentResolver.query(rootUri, projection, null, null, null)
            ?: throw IllegalStateException("The selected folder could not be read.")
        cursor.use {
            require(it.moveToFirst() &&
                it.getString(it.getColumnIndexOrThrow(projection[0])) == "load" &&
                it.getString(it.getColumnIndexOrThrow(projection[1])) ==
                    DocumentsContract.Document.MIME_TYPE_DIR) {
                "Select the folder named load inside your Eden installation, not its parent or a game folder."
            }
        }
        return rootUri
    }

    private fun documentSupportsCreate(documentUri: Uri): Boolean {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_FLAGS)
        return try {
            contentResolver.query(documentUri, projection, null, null, null)?.use { cursor ->
                if (!cursor.moveToFirst()) {
                    false
                } else {
                    val flags = cursor.getLong(0)
                    (flags and DocumentsContract.Document.FLAG_DIR_SUPPORTS_CREATE.toLong()) != 0L
                }
            } ?: false
        } catch (_: Exception) {
            false
        }
    }

    private fun accessStatusJson(
        selected: Boolean,
        validLocation: Boolean,
        readPermission: Boolean,
        writePermission: Boolean,
        readable: Boolean,
        writable: Boolean,
        message: String
    ): String = JSONObject()
        .put("selected", selected)
        .put("validLocation", validLocation)
        .put("readPermission", readPermission)
        .put("writePermission", writePermission)
        .put("readable", readable)
        .put("writable", writable)
        .put(
            "ready",
            validLocation && readPermission && writePermission && readable && writable
        )
        .put("message", lastSelectionError.ifEmpty { message })
        .toString()

    private fun pathSegments(relativePath: String): List<String> {
        if (relativePath.isBlank()) return emptyList()
        val segments = relativePath.split('/')
        if (segments.any { it.isBlank() || it == "." || it == ".." }) {
            throw IllegalArgumentException("Invalid relative path: $relativePath")
        }
        return segments
    }

    private fun queryChildren(parentUri: Uri): List<SafDocument> {
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
            while (it.moveToNext()) {
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
        return children
    }

    private fun findChild(parentUri: Uri, name: String): SafDocument? =
        queryChildren(parentUri).firstOrNull { it.name == name }

    private fun resolveDirectory(relativePath: String, create: Boolean): Uri? {
        var current = loadRootUri()
        for (segment in pathSegments(relativePath)) {
            val existing = findChild(current, segment)
            current = when {
                existing == null && create -> DocumentsContract.createDocument(
                    contentResolver,
                    current,
                    DocumentsContract.Document.MIME_TYPE_DIR,
                    segment
                ) ?: throw IllegalStateException("Could not create directory: $segment")
                existing == null -> return null
                !existing.directory -> throw IllegalStateException("Not a directory: $segment")
                else -> existing.uri
            }
        }
        return current
    }

    private fun resolveDocument(relativePath: String): SafDocument? {
        val segments = pathSegments(relativePath)
        if (segments.isEmpty()) {
            throw IllegalArgumentException("A file path is required")
        }
        val parent = resolveDirectory(segments.dropLast(1).joinToString("/"), false)
            ?: return null
        return findChild(parent, segments.last())
    }

    private fun safError(error: Exception): String =
        "ERROR: ${error.message ?: error.javaClass.simpleName}"

    private fun safStatus(action: () -> Unit): String = try {
        action()
        "OK"
    } catch (error: Exception) {
        safError(error)
    }
}
