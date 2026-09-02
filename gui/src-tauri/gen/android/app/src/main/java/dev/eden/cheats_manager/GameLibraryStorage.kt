package dev.eden.cheats_manager

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract
import android.system.Os
import android.system.OsConstants
import androidx.activity.result.contract.ActivityResultContracts
import org.json.JSONArray
import org.json.JSONObject

/** Read-only SAF tree used to enumerate user-owned NSP and XCI packages. */
internal class GameLibraryStorage(private val activity: MainActivity) {
    companion object {
        private const val PREFS = "game_library"
        private const val PREF_LIBRARY_URI = "library_uri"
        private const val MAX_SCAN_DEPTH = 5
        private const val MAX_PACKAGES = 2_000
    }

    private data class SafDocument(
        val uri: Uri,
        val name: String,
        val directory: Boolean,
        val size: Long
    )

    private val contentResolver
        get() = activity.contentResolver

    @Volatile
    private var packageUris: Map<String, Uri> = emptyMap()

    private val libraryPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            if (uri == null) return@registerForActivityResult

            val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION
            val previousUri = preferences().getString(PREF_LIBRARY_URI, null)?.let(Uri::parse)
            try {
                contentResolver.takePersistableUriPermission(uri, flags)
                val saved = preferences()
                    .edit()
                    .putString(PREF_LIBRARY_URI, uri.toString())
                    .commit()
                if (!saved) {
                    if (previousUri != uri) {
                        contentResolver.releasePersistableUriPermission(uri, flags)
                    }
                    throw IllegalStateException("Could not save the game-library permission")
                }

                if (previousUri != null && previousUri != uri) {
                    try {
                        contentResolver.releasePersistableUriPermission(previousUri, flags)
                    } catch (error: SecurityException) {
                        android.util.Log.w(
                            "CheatsManager",
                            "Could not release the previous game-library permission",
                            error
                        )
                    }
                }
                packageUris = emptyMap()
            } catch (error: Exception) {
                android.util.Log.e(
                    "CheatsManager",
                    "Could not persist the game-library permission",
                    error
                )
            }
        }

    fun selectDirectory() {
        activity.runOnUiThread {
            libraryPicker.launch(null)
        }
    }

    fun getStatus(): String {
        val treeUri = savedTreeUri()
        if (treeUri == null) {
            return statusJson(false, "", false, false, "Select the game-library directory.")
        }

        val name = displayName(rootDocumentUri(treeUri))
        val readPermission = hasReadPermission(treeUri)
        if (!readPermission) {
            return statusJson(
                true,
                name,
                false,
                false,
                "Android can no longer read the game library. Select it again."
            )
        }

        val readable = try {
            queryChildren(rootDocumentUri(treeUri))
            true
        } catch (_: Exception) {
            false
        }
        return statusJson(
            true,
            name,
            true,
            readable,
            if (readable) "Game-library access is ready." else "The game library could not be read."
        )
    }

    fun listPackages(): String {
        return try {
            val treeUri = requireTreeUri()
            val packages = mutableListOf<Pair<String, SafDocument>>()
            val visited = mutableSetOf<String>()
            scanDirectory(
                rootDocumentUri(treeUri),
                "",
                0,
                visited,
                packages
            )
            packages.sortBy { it.first.lowercase() }
            packageUris = packages.associate { (path, document) -> path to document.uri }

            val result = JSONArray()
            for ((path, document) in packages) {
                result.put(
                    JSONObject()
                        .put("relativePath", path)
                        .put("name", document.name)
                        .put("size", document.size)
                )
            }
            result.toString()
        } catch (error: Exception) {
            "ERROR: ${error.message ?: error.javaClass.simpleName}"
        }
    }

    fun openPackageReadFd(relativePath: String): Int {
        val uri = packageUris[relativePath]
            ?: return PackageDocumentStorage.FD_ERROR_UNAVAILABLE
        if (!relativePath.endsWith(".nsp", ignoreCase = true) &&
            !relativePath.endsWith(".xci", ignoreCase = true)
        ) {
            return PackageDocumentStorage.FD_ERROR_OPEN
        }

        return try {
            val original = contentResolver.openFileDescriptor(uri, "r")
                ?: return PackageDocumentStorage.FD_ERROR_OPEN
            original.use {
                val duplicate = ParcelFileDescriptor.dup(it.fileDescriptor)
                try {
                    try {
                        Os.lseek(duplicate.fileDescriptor, 0, OsConstants.SEEK_SET)
                    } catch (_: Exception) {
                        return PackageDocumentStorage.FD_ERROR_NOT_SEEKABLE
                    }
                    duplicate.detachFd()
                } finally {
                    duplicate.close()
                }
            }
        } catch (error: Exception) {
            android.util.Log.e("CheatsManager", "Could not open library package", error)
            PackageDocumentStorage.FD_ERROR_OPEN
        }
    }

    private fun scanDirectory(
        directoryUri: Uri,
        relativePath: String,
        depth: Int,
        visited: MutableSet<String>,
        packages: MutableList<Pair<String, SafDocument>>
    ) {
        if (depth > MAX_SCAN_DEPTH) return
        val documentId = DocumentsContract.getDocumentId(directoryUri)
        if (!visited.add(documentId)) return

        for (child in queryChildren(directoryUri)) {
            val childPath = if (relativePath.isEmpty()) {
                child.name
            } else {
                "$relativePath/${child.name}"
            }
            if (child.directory) {
                scanDirectory(child.uri, childPath, depth + 1, visited, packages)
            } else if (child.name.endsWith(".nsp", ignoreCase = true) ||
                child.name.endsWith(".xci", ignoreCase = true)
            ) {
                if (packages.size >= MAX_PACKAGES) {
                    throw IllegalStateException("The selected library contains more than $MAX_PACKAGES packages")
                }
                packages.add(childPath to child)
            }
        }
    }

    private fun savedTreeUri(): Uri? =
        preferences().getString(PREF_LIBRARY_URI, null)?.let {
            try {
                Uri.parse(it)
            } catch (_: Exception) {
                null
            }
        }

    private fun requireTreeUri(): Uri {
        val treeUri = savedTreeUri()
            ?: throw IllegalStateException("Select the game-library directory first")
        if (!hasReadPermission(treeUri)) {
            throw IllegalStateException("Game-library permission is no longer available")
        }
        return treeUri
    }

    private fun hasReadPermission(uri: Uri): Boolean =
        contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == uri }
            ?.isReadPermission == true

    private fun rootDocumentUri(treeUri: Uri): Uri =
        DocumentsContract.buildDocumentUriUsingTree(
            treeUri,
            DocumentsContract.getTreeDocumentId(treeUri)
        )

    private fun queryChildren(parentUri: Uri): List<SafDocument> {
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(
            parentUri,
            DocumentsContract.getDocumentId(parentUri)
        )
        val projection = arrayOf(
            DocumentsContract.Document.COLUMN_DOCUMENT_ID,
            DocumentsContract.Document.COLUMN_DISPLAY_NAME,
            DocumentsContract.Document.COLUMN_MIME_TYPE,
            DocumentsContract.Document.COLUMN_SIZE
        )
        val cursor = contentResolver.query(childrenUri, projection, null, null, null)
            ?: throw IllegalStateException("Could not query the game library")
        val children = mutableListOf<SafDocument>()
        cursor.use {
            val idColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DOCUMENT_ID)
            val nameColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
            val mimeColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_MIME_TYPE)
            val sizeColumn = it.getColumnIndexOrThrow(DocumentsContract.Document.COLUMN_SIZE)
            while (it.moveToNext()) {
                val documentId = it.getString(idColumn)
                children.add(
                    SafDocument(
                        uri = DocumentsContract.buildDocumentUriUsingTree(parentUri, documentId),
                        name = it.getString(nameColumn) ?: "",
                        directory = it.getString(mimeColumn) ==
                                DocumentsContract.Document.MIME_TYPE_DIR,
                        size = if (it.isNull(sizeColumn)) 0 else it.getLong(sizeColumn)
                    )
                )
            }
        }
        return children
    }

    private fun displayName(uri: Uri): String {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        return try {
            contentResolver.query(uri, projection, null, null, null)?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) ?: "" else ""
            } ?: ""
        } catch (_: Exception) {
            ""
        }
    }

    private fun statusJson(
        selected: Boolean,
        name: String,
        readPermission: Boolean,
        readable: Boolean,
        message: String
    ): String = JSONObject()
        .put("selected", selected)
        .put("name", name)
        .put("readPermission", readPermission)
        .put("readable", readable)
        .put("ready", selected && readPermission && readable)
        .put("message", message)
        .toString()

    private fun preferences() =
        activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
