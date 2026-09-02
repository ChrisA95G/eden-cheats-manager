package dev.eden.cheats_manager

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.system.Os
import android.system.OsConstants
import androidx.activity.result.contract.ActivityResultContracts
import org.json.JSONObject

/** Read-only persisted document access for user-supplied Switch keys and packages. */
internal class PackageDocumentStorage(private val activity: MainActivity) {
    companion object {
        private const val PREFS = "package_documents"
        private const val PREF_PROD_KEYS_URI = "prod_keys_uri"
        private const val PREF_PACKAGE_URI = "game_package_uri"

        const val FD_ERROR_UNAVAILABLE = -1
        const val FD_ERROR_PERMISSION = -2
        const val FD_ERROR_OPEN = -3
        const val FD_ERROR_NOT_SEEKABLE = -4
    }

    private data class DocumentProbe(
        val selected: Boolean,
        val name: String,
        val readable: Boolean,
        val seekable: Boolean
    )

    private val contentResolver
        get() = activity.contentResolver

    @Volatile
    private var lastSelectionError = ""

    private val prodKeysPicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            persistSelection(uri, PREF_PROD_KEYS_URI) { name ->
                if (!name.equals("prod.keys", ignoreCase = true)) {
                    "Select the file named prod.keys."
                } else {
                    null
                }
            }
        }

    private val packagePicker =
        activity.registerForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            persistSelection(uri, PREF_PACKAGE_URI) { name ->
                if (!name.endsWith(".nsp", ignoreCase = true) &&
                    !name.endsWith(".xci", ignoreCase = true)
                ) {
                    "Select an uncompressed .nsp or .xci file."
                } else {
                    null
                }
            }
        }

    fun selectProdKeys() {
        activity.runOnUiThread {
            prodKeysPicker.launch(arrayOf("*/*"))
        }
    }

    fun selectGamePackage() {
        activity.runOnUiThread {
            packagePicker.launch(arrayOf("*/*"))
        }
    }

    fun getStatus(): String {
        val prodKeys = probeDocument(PREF_PROD_KEYS_URI)
        val gamePackage = probeDocument(PREF_PACKAGE_URI)
        val ready = prodKeys.readable && prodKeys.seekable &&
                gamePackage.readable && gamePackage.seekable
        val message = when {
            lastSelectionError.isNotEmpty() -> lastSelectionError
            !prodKeys.selected -> "Select prod.keys to decrypt package metadata."
            !prodKeys.readable -> "Android can no longer read prod.keys. Select it again."
            !prodKeys.seekable -> "The selected prod.keys provider is not seekable. Select a local file."
            !gamePackage.selected -> "Select one uncompressed NSP or XCI package."
            !gamePackage.readable -> "Android can no longer read the selected package. Select it again."
            !gamePackage.seekable -> "The selected package provider is not seekable. Select a local file."
            else -> "Keys and package are ready to inspect."
        }

        return JSONObject()
            .put("prodKeysSelected", prodKeys.selected)
            .put("prodKeysName", prodKeys.name)
            .put("prodKeysReadable", prodKeys.readable)
            .put("prodKeysSeekable", prodKeys.seekable)
            .put("packageSelected", gamePackage.selected)
            .put("packageName", gamePackage.name)
            .put("packageReadable", gamePackage.readable)
            .put("packageSeekable", gamePackage.seekable)
            .put("ready", ready)
            .put("message", message)
            .toString()
    }

    fun openProdKeysReadFd(): Int = openReadFd(PREF_PROD_KEYS_URI)

    fun openGamePackageReadFd(): Int = openReadFd(PREF_PACKAGE_URI)

    private fun persistSelection(
        uri: Uri?,
        preferenceKey: String,
        validateName: (String) -> String?
    ) {
        if (uri == null) {
            android.util.Log.i("CheatsManager", "Package document selection cancelled")
            return
        }

        val name = displayName(uri)
        val validationError = validateName(name)
        if (validationError != null) {
            lastSelectionError = validationError
            android.util.Log.e("CheatsManager", validationError)
            return
        }

        val flags = Intent.FLAG_GRANT_READ_URI_PERMISSION
        val previousUri = preferences().getString(preferenceKey, null)?.let(Uri::parse)
        try {
            contentResolver.takePersistableUriPermission(uri, flags)
            val saved = preferences()
                .edit()
                .putString(preferenceKey, uri.toString())
                .commit()
            if (!saved) {
                if (previousUri != uri) {
                    contentResolver.releasePersistableUriPermission(uri, flags)
                }
                throw IllegalStateException("Could not save the selected document")
            }

            if (previousUri != null && previousUri != uri) {
                try {
                    contentResolver.releasePersistableUriPermission(previousUri, flags)
                } catch (error: SecurityException) {
                    android.util.Log.w(
                        "CheatsManager",
                        "Could not release a previous package document permission",
                        error
                    )
                }
            }
            lastSelectionError = ""
            android.util.Log.i("CheatsManager", "Persisted read access for $name")
        } catch (error: Exception) {
            lastSelectionError = error.message ?: "Could not persist document access."
            android.util.Log.e("CheatsManager", "Could not persist document access", error)
        }
    }

    private fun probeDocument(preferenceKey: String): DocumentProbe {
        val uri = preferences().getString(preferenceKey, null)?.let {
            try {
                Uri.parse(it)
            } catch (_: Exception) {
                null
            }
        } ?: return DocumentProbe(false, "", false, false)

        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == uri }
        if (permission?.isReadPermission != true) {
            return DocumentProbe(true, displayName(uri), false, false)
        }

        return try {
            val descriptor = contentResolver.openFileDescriptor(uri, "r")
                ?: return DocumentProbe(true, displayName(uri), false, false)
            descriptor.use {
                val seekable = try {
                    Os.lseek(it.fileDescriptor, 0, OsConstants.SEEK_SET)
                    true
                } catch (_: Exception) {
                    false
                }
                DocumentProbe(true, displayName(uri), true, seekable)
            }
        } catch (_: Exception) {
            DocumentProbe(true, displayName(uri), false, false)
        }
    }

    private fun openReadFd(preferenceKey: String): Int {
        val uri = preferences().getString(preferenceKey, null)?.let {
            try {
                Uri.parse(it)
            } catch (_: Exception) {
                null
            }
        } ?: return FD_ERROR_UNAVAILABLE

        val permission = contentResolver.persistedUriPermissions
            .firstOrNull { it.uri == uri }
        if (permission?.isReadPermission != true) {
            return FD_ERROR_PERMISSION
        }

        return try {
            val original = contentResolver.openFileDescriptor(uri, "r")
                ?: return FD_ERROR_OPEN
            original.use {
                val duplicate = ParcelFileDescriptor.dup(it.fileDescriptor)
                try {
                    try {
                        Os.lseek(duplicate.fileDescriptor, 0, OsConstants.SEEK_SET)
                    } catch (_: Exception) {
                        return FD_ERROR_NOT_SEEKABLE
                    }
                    duplicate.detachFd()
                } finally {
                    duplicate.close()
                }
            }
        } catch (error: Exception) {
            android.util.Log.e("CheatsManager", "Could not open package document", error)
            FD_ERROR_OPEN
        }
    }

    private fun displayName(uri: Uri): String {
        return try {
            contentResolver.query(
                uri,
                arrayOf(OpenableColumns.DISPLAY_NAME),
                null,
                null,
                null
            )?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) ?: "" else ""
            } ?: ""
        } catch (_: Exception) {
            ""
        }
    }

    private fun preferences() =
        activity.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}
