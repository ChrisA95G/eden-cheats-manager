package dev.eden.cheats_manager

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.contract.ActivityResultContracts
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.Executors

/** The cheat install folder: a `load` folder EDC writes cheats into through SAF. */
internal class EdenSafStorage(private val activity: MainActivity) {
    companion object {
        private const val SAF_PREFS = "eden_saf"
        private const val PREF_EDEN_LOAD_URI = "eden_load_uri"
        /**
         * Document ID of the `load` folder when it is a child of the granted tree. Absent when
         * the grant points at `load` itself, which can happen in either install mode, so callers
         * must not infer the mode from this field.
         */
        private const val PREF_LOAD_DOCUMENT_ID = "load_document_id"
        /** [MODE_DIRECT] or [MODE_CUSTOM]. A saved tree without a mode predates custom folders and is [MODE_DIRECT]. */
        private const val PREF_INSTALL_MODE = "install_mode"
        /** An emulator's own `load`, where the emulator creates each game's title-ID folder. */
        const val MODE_DIRECT = "direct"
        /** A `load` folder on shared storage that another app (such as Peanuts Native) imports from. */
        const val MODE_CUSTOM = "custom"
        private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"
        /** Android's own shared-storage providers; a folder from any of them is a custom install. */
        private val SHARED_STORAGE_AUTHORITIES = setOf(
            EXTERNAL_STORAGE_AUTHORITY,
            "com.android.providers.downloads.documents"
        )
        private const val RW = SafGrants.READ or SafGrants.WRITE

        /** The saved cheat-install tree, for the game library's grant bookkeeping. */
        fun savedTreeUri(context: Context): Uri? =
            context.getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)
                .getString(PREF_EDEN_LOAD_URI, null)
                ?.let { runCatching { Uri.parse(it) }.getOrNull() }
    }

    private data class SafDocument(
        val uri: Uri,
        val name: String,
        val directory: Boolean
    )

    /** The saved tree, `load` child, and mode, read from one preferences snapshot. */
    private data class Selection(
        val treeUri: String,
        val loadDocumentId: String?,
        val mode: String
    )

    private val contentResolver
        get() = activity.contentResolver

    private val selectionExecutor = Executors.newSingleThreadExecutor()

    @Volatile
    private var lastSelectionError = ""

    @Volatile
    private var selectionPending = false

    private val folderPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            lastSelectionError = ""
            if (uri == null) {
                android.util.Log.i("CheatsManager", "Cheat install folder selection cancelled")
                return@registerForActivityResult
            }
            val mode = if ((uri.authority ?: "") in SHARED_STORAGE_AUTHORITIES) MODE_CUSTOM else MODE_DIRECT
            selectionPending = true
            selectionExecutor.execute {
                try {
                    persistSelection(uri, mode)
                } catch (error: Exception) {
                    lastSelectionError = error.message ?: "Could not save access to the selected folder."
                    android.util.Log.e("CheatsManager", "Could not save the cheat install folder", error)
                } finally {
                    selectionPending = false
                }
            }
        }

    /**
     * Keep the grant first, then use the selected `load` directory, or find or create its
     * `load` child. Runs off the UI thread. On failure only the flags this pick acquired are
     * released and the previous selection stays active.
     */
    private fun persistSelection(uri: Uri, mode: String) {
        require(uri.scheme == "content" && DocumentsContract.isTreeUri(uri)) {
            "Select a folder on this device using the folder picker."
        }
        val treeDocument = DocumentsContract.buildDocumentUriUsingTree(
            uri,
            DocumentsContract.getTreeDocumentId(uri)
        )
        val treeName = displayName(treeDocument)
            ?: throw IllegalStateException("Android could not read the selected folder's name. Choose another folder.")

        val previous = savedSelection()
        val acquired = SafGrants.missingFlags(contentResolver, uri, RW)
        contentResolver.takePersistableUriPermission(uri, RW)
        try {
            val loadDocumentId = if (treeName == "load") null else findOrCreateLoad(treeDocument)
            val rootUri = resolveLoadDirectory(uri, loadDocumentId)
            queryChildren(rootUri)
            check(documentSupportsCreate(rootUri)) {
                "The selected load folder does not allow cheat files to be created."
            }
            val editor = preferences()
                .edit()
                .putString(PREF_EDEN_LOAD_URI, uri.toString())
                .putString(PREF_INSTALL_MODE, mode)
            if (loadDocumentId == null) {
                editor.remove(PREF_LOAD_DOCUMENT_ID)
            } else {
                editor.putString(PREF_LOAD_DOCUMENT_ID, loadDocumentId)
            }
            if (!editor.commit()) {
                restoreSelection(previous)
                throw IllegalStateException("Could not save the cheat install folder.")
            }
        } catch (error: Exception) {
            SafGrants.release(contentResolver, uri, acquired)
            throw error
        }

        val previousUri = previous?.treeUri?.let { runCatching { Uri.parse(it) }.getOrNull() }
        if (previousUri != null && previousUri != uri) {
            val libraryKeeps = if (GameLibraryStorage.savedTreeUri(activity) == previousUri) SafGrants.READ else 0
            SafGrants.release(contentResolver, previousUri, RW, keep = libraryKeeps)
        }
        android.util.Log.i("CheatsManager", "Cheat install folder saved ($mode)")
    }

    /** The document ID of [treeDocument]'s `load` child, created when missing. */
    private fun findOrCreateLoad(treeDocument: Uri): String {
        val existing = queryChildren(treeDocument).firstOrNull { it.name == "load" }
        check(existing == null || existing.directory) {
            "The selected folder has a file named load. Pick another folder."
        }
        val load = existing?.uri ?: run {
            check(documentSupportsCreate(treeDocument)) {
                "The selected folder does not allow a load folder to be created in it."
            }
            DocumentsContract.createDocument(
                contentResolver,
                treeDocument,
                DocumentsContract.Document.MIME_TYPE_DIR,
                "load"
            ) ?: throw IllegalStateException("Could not create the load folder.")
        }
        return DocumentsContract.getDocumentId(load)
    }

    /** Put back the selection a failed save left in memory. */
    private fun restoreSelection(previous: Selection?) {
        val editor = preferences().edit()
        if (previous == null) {
            editor.remove(PREF_EDEN_LOAD_URI).remove(PREF_LOAD_DOCUMENT_ID).remove(PREF_INSTALL_MODE)
        } else {
            editor.putString(PREF_EDEN_LOAD_URI, previous.treeUri)
                .putString(PREF_INSTALL_MODE, previous.mode)
            if (previous.loadDocumentId == null) {
                editor.remove(PREF_LOAD_DOCUMENT_ID)
            } else {
                editor.putString(PREF_LOAD_DOCUMENT_ID, previous.loadDocumentId)
            }
        }
        editor.apply()
    }

    /** Open the folder picker. Throws when it cannot be launched, so the caller is not left waiting. */
    fun selectFolder() {
        activity.runOnUiThread {
            try {
                lastSelectionError = ""
                folderPicker.launch(null)
            } catch (error: Exception) {
                lastSelectionError = "Android could not open the folder picker."
                android.util.Log.e("CheatsManager", "Could not open the folder picker", error)
            }
        }
    }

    private fun savedSelection(): Selection? {
        val values = preferences().all
        val treeUri = values[PREF_EDEN_LOAD_URI] as? String ?: return null
        val mode = if (values[PREF_INSTALL_MODE] == MODE_CUSTOM) MODE_CUSTOM else MODE_DIRECT
        return Selection(treeUri, values[PREF_LOAD_DOCUMENT_ID] as? String, mode)
    }

    private fun displayName(documentUri: Uri): String? {
        val projection = arrayOf(DocumentsContract.Document.COLUMN_DISPLAY_NAME)
        return try {
            contentResolver.query(documentUri, projection, null, null, null)?.use {
                if (it.moveToFirst()) it.getString(0) else null
            }
        } catch (_: Exception) {
            null
        }
    }

    /** Where EDC writes, for messages: a shared-storage path, or the provider's app and folder names. */
    private fun folderLabel(treeUri: Uri, loadDocumentId: String?): String {
        val documentId = loadDocumentId ?: DocumentsContract.getTreeDocumentId(treeUri)
        if (treeUri.authority == EXTERNAL_STORAGE_AUTHORITY && documentId.contains(':')) {
            val volume = documentId.substringBefore(':')
            val path = documentId.substringAfter(':')
            val volumeName = if (volume == "primary") "Internal storage" else volume
            return if (path.isEmpty()) volumeName else "$volumeName/$path"
        }
        val provider = try {
            val packageManager = activity.packageManager
            packageManager.resolveContentProvider(treeUri.authority ?: "", 0)
                ?.loadLabel(packageManager)?.toString()
        } catch (_: Exception) {
            null
        }
        val treeName = displayName(
            DocumentsContract.buildDocumentUriUsingTree(treeUri, DocumentsContract.getTreeDocumentId(treeUri))
        )
        val parts = listOfNotNull(provider, treeName, if (loadDocumentId != null) "load" else null)
        return parts.filterIndexed { index, part -> index == 0 || part != parts[index - 1] }
            .joinToString("/")
            .ifEmpty { "load" }
    }

    fun getAccessStatus(): String {
        val selection = savedSelection()
            ?: return accessStatusJson(
                selection = null,
                validLocation = false,
                readPermission = false,
                writePermission = false,
                readable = false,
                writable = false,
                message = "Choose a cheat install folder to continue."
            )

        val treeUri = try {
            Uri.parse(selection.treeUri)
        } catch (_: Exception) {
            return accessStatusJson(
                selection = selection,
                validLocation = false,
                readPermission = false,
                writePermission = false,
                readable = false,
                writable = false,
                message = "The saved cheat install folder is invalid. Choose it again."
            )
        }
        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        val readPermission = permission?.isReadPermission == true
        val writePermission = permission?.isWritePermission == true
        if (!readPermission || !writePermission) {
            return accessStatusJson(
                selection = selection,
                validLocation = false,
                readPermission = readPermission,
                writePermission = writePermission,
                readable = false,
                writable = false,
                message = "Android no longer grants full access to the cheat install folder. Choose it again."
            )
        }

        val rootUri = try {
            resolveLoadDirectory(treeUri, selection.loadDocumentId)
        } catch (error: Exception) {
            return accessStatusJson(
                selection = selection,
                validLocation = false,
                readPermission = readPermission,
                writePermission = writePermission,
                readable = false,
                writable = false,
                message = error.message ?: "The saved cheat install folder is unavailable. Choose it again."
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
            !readable -> "EDC could not read the cheat install folder. Choose it again."
            !writable -> "The cheat install folder does not allow new files. Choose a writable folder."
            else -> "Cheat install folder is ready."
        }
        return accessStatusJson(
            selection = selection,
            validLocation = true,
            readPermission = true,
            writePermission = true,
            readable = readable,
            writable = writable,
            message = message,
            folderLabel = folderLabel(treeUri, selection.loadDocumentId)
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
        val selection = savedSelection()
            ?: throw IllegalStateException("Choose a cheat install folder first")
        val treeUri = Uri.parse(selection.treeUri)
        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == treeUri }
        if (permission?.isReadPermission != true || permission.isWritePermission != true) {
            throw IllegalStateException("Access to the cheat install folder is no longer available")
        }
        return resolveLoadDirectory(treeUri, selection.loadDocumentId)
    }

    /** The `load` folder EDC writes to: the granted tree itself, or its `load` child [loadDocumentId]. */
    private fun resolveLoadDirectory(treeUri: Uri, loadDocumentId: String? = null): Uri {
        require(treeUri.scheme == "content" && !treeUri.authority.isNullOrBlank() &&
            DocumentsContract.isTreeUri(treeUri)) {
            "Choose the cheat install folder using the folder picker."
        }
        val documentId = loadDocumentId ?: DocumentsContract.getTreeDocumentId(treeUri)
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
                "The cheat install folder's load folder is missing. Choose the folder again."
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

    /** Status of the active folder; a failed or unfinished pick is reported separately. */
    private fun accessStatusJson(
        selection: Selection?,
        validLocation: Boolean,
        readPermission: Boolean,
        writePermission: Boolean,
        readable: Boolean,
        writable: Boolean,
        message: String,
        folderLabel: String = ""
    ): String = JSONObject()
        .put("selected", selection != null)
        .put("validLocation", validLocation)
        .put("readPermission", readPermission)
        .put("writePermission", writePermission)
        .put("readable", readable)
        .put("writable", writable)
        .put(
            "ready",
            validLocation && readPermission && writePermission && readable && writable
        )
        .put("message", message)
        .put("installMode", selection?.mode ?: "")
        .put("folderLabel", folderLabel)
        .put("selectionError", lastSelectionError)
        .put("selectionPending", selectionPending)
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
            ?: throw IllegalStateException("Could not read the cheat install folder")
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
