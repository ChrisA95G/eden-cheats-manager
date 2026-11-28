package dev.eden.cheats_manager

import android.content.Context
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.contract.ActivityResultContracts
import androidx.lifecycle.lifecycleScope
import com.google.android.material.dialog.MaterialAlertDialogBuilder
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import org.json.JSONArray
import org.json.JSONObject

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
        /** A `load` folder another app imports from. */
        const val MODE_CUSTOM = "custom"
        private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"
        // Keep each write or removal on its original grant until it finishes.
        private val mutationLock = Any()
        private val TITLE_ID = Regex("[0-9A-Fa-f]{16}")
        private const val RW = SafGrants.READ or SafGrants.WRITE

        internal fun needsLoadChild(treeName: String, mode: String): Boolean {
            if (treeName == "load") return false
            require(mode == MODE_CUSTOM) { "For a direct install, choose the emulator’s existing load folder." }
            return true
        }

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
    ) {
        /** A load directory keeps this ID when selected directly or through its parent. */
        val destinationId: String
            get() {
                val tree = Uri.parse(treeUri)
                val id = loadDocumentId ?: DocumentsContract.getTreeDocumentId(tree)
                return DocumentsContract.buildDocumentUri(tree.authority, id).toString()
            }
    }

    private val contentResolver
        get() = activity.contentResolver

    private class PendingPick(val previous: Selection?) {
        @Volatile var status: String? = null
    }

    private var pendingPick: PendingPick? = null
    private var lastSelectionError = ""

    private val folderPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            if (uri == null) {
                finishPick()
                return@registerForActivityResult
            }
            // Android can deliver a picker result to a recreated activity.
            synchronized(this) {
                if (pendingPick == null) pendingPick = PendingPick(savedSelection())
            }
            MaterialAlertDialogBuilder(activity)
                .setTitle("How will you use this folder?")
                .setMessage("Direct installs use the emulator’s existing load folder. " +
                    "For imports, select load itself or a folder where EDC can find or create load.")
                .setPositiveButton("Install directly") { _, _ -> savePickedFolder(uri, MODE_DIRECT) }
                .setNegativeButton("Prepare for import") { _, _ -> savePickedFolder(uri, MODE_CUSTOM) }
                .setNeutralButton(android.R.string.cancel) { _, _ -> finishPick() }
                .setOnCancelListener { finishPick() }
                .show()
        }

    private fun savePickedFolder(uri: Uri, mode: String) {
        activity.lifecycleScope.launch(Dispatchers.IO) {
            try {
                synchronized(mutationLock) { persistSelection(uri, mode) }
                finishPick()
            } catch (error: Exception) {
                finishPick(error.message ?: "Could not save access to the selected folder.")
                android.util.Log.e("CheatsManager", "Could not save the cheat install folder", error)
            }
        }
    }

    @Synchronized
    private fun finishPick(error: String = "") {
        lastSelectionError = error
        pendingPick = null
    }

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

        val useChild = needsLoadChild(treeName, mode)
        val previous = savedSelection()
        val acquired = SafGrants.missingFlags(contentResolver, uri, RW)
        contentResolver.takePersistableUriPermission(uri, RW)
        try {
            val loadDocumentId = if (useChild) findOrCreateLoad(treeDocument) else null
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

    fun selectFolder() {
        synchronized(this) {
            check(pendingPick == null) { "A folder selection is already in progress." }
            pendingPick = PendingPick(savedSelection())
            lastSelectionError = ""
        }
        activity.runOnUiThread {
            try {
                folderPicker.launch(null)
            } catch (error: Exception) {
                finishPick("Android could not open the folder picker.")
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
        val pick: PendingPick?
        val selection: Selection?
        val error: String
        synchronized(this) {
            pick = pendingPick
            pick?.status?.let { return it }
            selection = if (pick == null) savedSelection() else pick.previous
            error = lastSelectionError
        }
        val status = readAccessStatus(selection)
            .put("selectionPending", pick != null)
            .put("selectionError", error)
            .toString()
        pick?.status = status
        return status
    }

    private fun readAccessStatus(selection: Selection?): JSONObject {
        if (selection == null) {
            return accessStatusJson(
                selection = null,
                validLocation = false,
                readPermission = false,
                writePermission = false,
                readable = false,
                writable = false,
                message = "Choose a cheat install folder to continue."
            )
        }

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

    fun listDirectory(relativePath: String, destinationId: String? = null): String {
        return try {
            val root = loadRootUri(destinationId)
            val directory = resolveDirectory(root, pathSegments(relativePath), false) ?: return "[]"
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

    fun writeTextFile(relativePath: String, content: String, destinationId: String, replace: Boolean): String =
        mutateDestination(destinationId) { root ->
            val segments = pathSegments(relativePath)
            require(segments.isNotEmpty()) { "A file path is required" }
            val parent = resolveDirectory(root, segments.dropLast(1), true)
                ?: throw IllegalStateException("Could not resolve parent directory")
            val filename = segments.last()
            val existing = findChild(parent, filename)
            check(existing?.directory != true) { "Path is a directory: $relativePath" }
            check(existing == null || replace) {
                "This cheat file already exists. Choose Replace file to overwrite it."
            }
            val fileUri = existing?.uri ?: DocumentsContract.createDocument(
                contentResolver, parent, "text/plain", filename
            ) ?: throw IllegalStateException("Could not create file: $filename")
            try {
                if (existing == null) {
                    check(displayName(fileUri) == filename) { "The provider could not create the file as $filename." }
                }
                val output = contentResolver.openOutputStream(fileUri, "wt")
                    ?: throw IllegalStateException("Could not open file: $filename")
                output.use { it.write(content.toByteArray(Charsets.UTF_8)) }
            } catch (error: Exception) {
                if (existing == null) runCatching { DocumentsContract.deleteDocument(contentResolver, fileUri) }
                throw error
            }
        }

    fun deleteFile(relativePath: String, destinationId: String): String =
        mutateDestination(destinationId) { root ->
            val segments = pathSegments(relativePath)
            require(segments.isNotEmpty()) { "A file path is required" }
            val file = resolveDocument(root, segments)
            if (file != null) {
                check(!file.directory) { "Path is a directory: $relativePath" }
                check(DocumentsContract.deleteDocument(contentResolver, file.uri)) { "Could not delete file: $relativePath" }
            }
            // Clean up the cheats and mod directories while retaining the game's title directory.
            for (level in 1..minOf(2, segments.lastIndex)) {
                val parent = resolveDocument(root, segments.dropLast(level)) ?: continue
                if (!parent.directory || queryChildren(parent.uri).isNotEmpty()) break
                check(DocumentsContract.deleteDocument(contentResolver, parent.uri)) {
                    "Could not remove the empty directory: ${parent.name}"
                }
            }
        }

    private fun preferences() =
        activity.getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)

    private fun loadRootUri(expectedDestinationId: String? = null): Uri {
        val selection = savedSelection()
            ?: throw IllegalStateException("Choose a cheat install folder first")
        check(expectedDestinationId == null || selection.destinationId == expectedDestinationId) {
            "The cheat install folder changed. Refresh the installed cheats and try again."
        }
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
    ): JSONObject = JSONObject()
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
        .put("destinationId", selection?.let { runCatching { it.destinationId }.getOrNull() }.orEmpty())

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

    private fun resolveDirectory(root: Uri, segments: List<String>, create: Boolean): Uri? {
        var current = root
        val canCreateTitle = create && savedSelection()?.mode == MODE_CUSTOM
        for (segment in segments) {
            val existing = findChild(current, segment)
            current = when {
                existing == null && create -> {
                    check(current != root || canCreateTitle || !TITLE_ID.matches(segment)) {
                        "Open this game in your emulator before installing cheats."
                    }
                    DocumentsContract.createDocument(contentResolver, current, DocumentsContract.Document.MIME_TYPE_DIR, segment)
                        ?: throw IllegalStateException("Could not create directory: $segment")
                }
                existing == null -> return null
                !existing.directory -> throw IllegalStateException("Not a directory: $segment")
                else -> existing.uri
            }
        }
        return current
    }

    private fun resolveDocument(root: Uri, segments: List<String>): SafDocument? {
        val parent = resolveDirectory(root, segments.dropLast(1), false) ?: return null
        return findChild(parent, segments.last())
    }

    private fun safError(error: Exception): String =
        "ERROR: ${error.message ?: error.javaClass.simpleName}"

    private fun mutateDestination(destinationId: String, action: (Uri) -> Unit): String = try {
        synchronized(mutationLock) { action(loadRootUri(destinationId)) }
        "OK"
    } catch (error: Exception) {
        safError(error)
    }
}
