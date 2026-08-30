package dev.eden.cheats_manager

import android.app.ActivityManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ActivityNotFoundException
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.DocumentsContract
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.core.app.NotificationCompat
import java.lang.ref.WeakReference
import org.json.JSONArray
import org.json.JSONObject

class MainActivity : TauriActivity() {
    companion object {
        @Volatile private var instance: WeakReference<MainActivity>? = null
        @Volatile private var killEdenOnResume = false

        private const val ALERT_CHANNEL_ID       = "scan_alert_channel"
        private const val RETURN_NOTIFICATION_ID = 43
        private const val RETURN_REQUEST_CODE    = 1
        private const val SAF_PREFS = "eden_saf"
        private const val PREF_EDEN_LOAD_URI = "eden_load_uri"
        private const val EDEN_DOCUMENTS_AUTHORITY = "dev.eden.eden_emulator.user"
        private val REQUIRED_DIR_NAME = "load"

        // SAF test
        @JvmStatic
        fun selectEdenLoadDirectory() {
            val activity = instance?.get() ?: return

            activity.runOnUiThread {
                activity.edenLoadPicker.launch(null)
            }
        }


        @JvmStatic
        fun safListDirectory(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.safListDirectoryInternal(relativePath)
        }

        @JvmStatic
        fun safWriteTextFile(relativePath: String, content: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.safWriteTextFileInternal(relativePath, content)
        }

        @JvmStatic
        fun safDeleteFile(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.safDeleteFileInternal(relativePath)
        }

        @JvmStatic
        fun safRemoveEmptyDirectory(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.safRemoveEmptyDirectoryInternal(relativePath)
        }

        /**
         * Called from Rust via JNI to launch Eden's EmulationActivity with a ROM URI.
         * Runs on whichever thread calls it; FLAG_ACTIVITY_NEW_TASK ensures Android
         * accepts the call even if this app is in the background at that moment.
         *
         * Strategy: try explicit component first (works if EmulationActivity is exported),
         * fall back to implicit ACTION_VIEW (works if Eden has a matching intent filter).
         */
        @JvmStatic
        fun launchIntent(uri: String) {
            val activity = instance?.get() ?: run {
                android.util.Log.e("CheatsManager", "launchIntent: no activity reference")
                return
            }
            android.util.Log.i("CheatsManager", "launchIntent uri=$uri")

            val explicit = Intent().apply {
                component = ComponentName(
                    "dev.eden.eden_emulator",
                    "org.yuzu.yuzu_emu.activities.EmulationActivity"
                )
                data = Uri.parse(uri)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            try {
                activity.startActivity(explicit)
                android.util.Log.i("CheatsManager", "launchIntent explicit OK")
                return
            } catch (e: ActivityNotFoundException) {
                android.util.Log.w("CheatsManager", "EmulationActivity not found, trying implicit: $e")
            } catch (e: SecurityException) {
                android.util.Log.w("CheatsManager", "EmulationActivity not exported, trying implicit: $e")
            } catch (e: Exception) {
                android.util.Log.w("CheatsManager", "Explicit launch failed, trying implicit: $e")
            }

            val implicit = Intent(Intent.ACTION_VIEW).apply {
                data = Uri.parse(uri)
                addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            }
            try {
                activity.startActivity(implicit)
                android.util.Log.i("CheatsManager", "launchIntent implicit OK")
            } catch (e: Exception) {
                android.util.Log.e("CheatsManager", "launchIntent implicit also failed: $e")
            }
        }

        /**
         * Bring this app back to the foreground after the build-ID scan.
         *
         * Android 13+ removed the foreground-service exemption for background activity
         * starts, so a plain startActivity() is silently dropped.  Instead we fire a
         * full-screen intent notification: the system dispatches it on our behalf,
         * bypassing the background-launch restriction.  On Android 14+ (where
         * USE_FULL_SCREEN_INTENT is restricted to call/alarm apps) the notification
         * falls back to a heads-up that the user can tap.
         */
        @JvmStatic
        fun returnToApp() {
            val activity = instance?.get() ?: run {
                android.util.Log.e("CheatsManager", "returnToApp: no activity reference")
                return
            }

            val returnIntent = Intent(activity, MainActivity::class.java).apply {
                addFlags(Intent.FLAG_ACTIVITY_REORDER_TO_FRONT or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            }
            val pi = PendingIntent.getActivity(
                activity, RETURN_REQUEST_CODE, returnIntent,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            )

            val notification = NotificationCompat.Builder(activity, ALERT_CHANNEL_ID)
                .setSmallIcon(android.R.drawable.ic_popup_sync)
                .setContentTitle("Eden Cheats Manager")
                .setContentText("Build ID scan complete — tap to view results")
                .setPriority(NotificationCompat.PRIORITY_MAX)
                .setCategory(NotificationCompat.CATEGORY_ALARM)
                .setContentIntent(pi)
                .setFullScreenIntent(pi, true)
                .setAutoCancel(true)
                .build()

            val nm = activity.getSystemService(NotificationManager::class.java)
            val notiEnabled = nm?.areNotificationsEnabled() ?: false
            val channelOk = nm?.getNotificationChannel(ALERT_CHANNEL_ID)
                ?.importance?.let { it >= NotificationManager.IMPORTANCE_HIGH } ?: false
            android.util.Log.i("CheatsManager",
                "returnToApp: notiEnabled=$notiEnabled channelOk=$channelOk")
            nm?.notify(RETURN_NOTIFICATION_ID, notification)
            android.util.Log.i("CheatsManager", "returnToApp: full-screen intent posted")

            // Also try direct launch — works on devices that haven't removed the exemption.
            try {
                activity.startActivity(returnIntent.apply {
                    addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                })
            } catch (_: Exception) {}

            // onResume() will kill Eden once our activity is definitively in the foreground.
            killEdenOnResume = true
        }

/** Start the scan foreground service so we can launch activities from background. */
        @JvmStatic
        fun startScanService() {
            val activity = instance?.get() ?: run {
                android.util.Log.e("CheatsManager", "startScanService: no activity reference")
                return
            }
            activity.startForegroundService(Intent(activity, ScanForegroundService::class.java))
            android.util.Log.i("CheatsManager", "startScanService: OK")
        }

        /** Stop the scan foreground service once we are back in the foreground. */
        @JvmStatic
        fun stopScanService() {
            val activity = instance?.get() ?: run {
                android.util.Log.e("CheatsManager", "stopScanService: no activity reference")
                return
            }
            activity.stopService(Intent(activity, ScanForegroundService::class.java))
            android.util.Log.i("CheatsManager", "stopScanService: OK")
        }


    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        instance = WeakReference(this)
        // Android 13+ requires POST_NOTIFICATIONS to show any notification (including
        // the full-screen intent we use to return from the build-ID scan).
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS)
                != android.content.pm.PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(android.Manifest.permission.POST_NOTIFICATIONS), 1001)
        }

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val nm = getSystemService(NotificationManager::class.java)
            nm?.createNotificationChannel(NotificationChannel(
                ScanForegroundService.CHANNEL_ID,
                "Build ID Scan Progress",
                NotificationManager.IMPORTANCE_LOW
            ))
            // HIGH importance so the full-screen intent fires immediately when scan completes.
            nm?.createNotificationChannel(NotificationChannel(
                ALERT_CHANNEL_ID,
                "Build ID Scan Result",
                NotificationManager.IMPORTANCE_HIGH
            ))
        }
    }

    override fun onResume() {
        super.onResume()
        if (killEdenOnResume) {
            killEdenOnResume = false
            // Best-effort kill; Eden is suspended in background and not consuming CPU,
            // so this is cosmetic. killBackgroundProcesses cannot kill apps that hold
            // a foreground service (which Eden does during emulation).
            val am = getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
            am.killBackgroundProcesses("dev.eden.eden_emulator")
            android.util.Log.i("CheatsManager", "onResume: killBackgroundProcesses sent for Eden")
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        instance = null
    }
    private data class SafDocument(
        val uri: Uri,
        val name: String,
        val directory: Boolean
    )

    private val edenLoadPicker =
        registerForActivityResult(ActivityResultContracts.OpenDocumentTree()) { uri ->
            if (uri == null) {
                android.util.Log.i("CheatsManager", "SAF selection cancelled")
                return@registerForActivityResult
            }

            if (uri.authority != EDEN_DOCUMENTS_AUTHORITY) {
                android.util.Log.e(
                    "CheatsManager",
                    "Selected directory is not provided by Eden"
                )
                return@registerForActivityResult
            }
            // Validate the folder name
            val documentFile = androidx.documentfile.provider.DocumentFile.fromTreeUri(this, uri)
            val directoryName = documentFile?.name

            if (directoryName != REQUIRED_DIR_NAME) {
                android.util.Log.e(
                    "CheatsManager",
                    "Invalid directory selected: '$directoryName'. Expected: '$REQUIRED_DIR_NAME'"
                )
                return@registerForActivityResult
            }

            try {
                val flags =
                    Intent.FLAG_GRANT_READ_URI_PERMISSION or
                        Intent.FLAG_GRANT_WRITE_URI_PERMISSION

                contentResolver.takePersistableUriPermission(uri, flags)

                getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)
                    .edit()
                    .putString(PREF_EDEN_LOAD_URI, uri.toString())
                    .apply()

                android.util.Log.i(
                    "CheatsManager",
                    "Eden SAF directory permission saved"
                )
            } catch (e: Exception) {
                android.util.Log.e(
                    "CheatsManager",
                    "Could not persist Eden SAF permission",
                    e
                )
            }
        }

    private fun edenLoadRootUri(): Uri {
        val savedUri = getSharedPreferences(SAF_PREFS, Context.MODE_PRIVATE)
            .getString(PREF_EDEN_LOAD_URI, null)
            ?: throw IllegalStateException("Select Eden's load directory first")
        val treeUri = Uri.parse(savedUri)
        return DocumentsContract.buildDocumentUriUsingTree(
            treeUri,
            DocumentsContract.getTreeDocumentId(treeUri)
        )
    }

    private fun safPathSegments(relativePath: String): List<String> {
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
        var current = edenLoadRootUri()
        for (segment in safPathSegments(relativePath)) {
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
        val segments = safPathSegments(relativePath)
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

    private fun safListDirectoryInternal(relativePath: String): String {
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

    private fun safWriteTextFileInternal(relativePath: String, content: String): String =
        safStatus {
            val segments = safPathSegments(relativePath)
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

    private fun safDeleteFileInternal(relativePath: String): String = safStatus {
        val document = resolveDocument(relativePath) ?: return@safStatus
        if (document.directory) {
            throw IllegalStateException("Path is a directory: $relativePath")
        }
        if (!DocumentsContract.deleteDocument(contentResolver, document.uri)) {
            throw IllegalStateException("Could not delete file: $relativePath")
        }
    }

    private fun safRemoveEmptyDirectoryInternal(relativePath: String): String = safStatus {
        val document = resolveDocument(relativePath) ?: return@safStatus
        if (!document.directory) {
            throw IllegalStateException("Path is not a directory: $relativePath")
        }
        if (queryChildren(document.uri).isEmpty() &&
            !DocumentsContract.deleteDocument(contentResolver, document.uri)) {
            throw IllegalStateException("Could not delete directory: $relativePath")
        }
    }

}
