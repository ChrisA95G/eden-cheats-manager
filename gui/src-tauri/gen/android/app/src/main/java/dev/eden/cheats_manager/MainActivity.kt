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
import android.os.Environment
import android.provider.Settings
import androidx.activity.enableEdgeToEdge
import androidx.core.app.NotificationCompat
import java.lang.ref.WeakReference
import rikka.shizuku.Shizuku

class MainActivity : TauriActivity() {
    companion object {
        @Volatile private var instance: WeakReference<MainActivity>? = null
        @Volatile private var killEdenOnResume = false

        private const val ALERT_CHANNEL_ID       = "scan_alert_channel"
        private const val RETURN_NOTIFICATION_ID = 43
        private const val RETURN_REQUEST_CODE    = 1
        private const val SHIZUKU_REQ_CODE       = 2001

        // ── API level ──────────────────────────────────────────────────────────

        @JvmStatic
        fun getApiLevel(): Int = Build.VERSION.SDK_INT

        // ── Shizuku status ─────────────────────────────────────────────────────

        /** True if the Shizuku service is running (binder reachable). */
        @JvmStatic
        fun isShizukuAvailable(): Boolean = try {
            Shizuku.pingBinder()
        } catch (_: Exception) { false }

        /** True if MANAGE_EXTERNAL_STORAGE-equivalent permission is granted via Shizuku. */
        @JvmStatic
        fun isShizukuGranted(): Boolean = try {
            !Shizuku.isPreV11() &&
                Shizuku.checkSelfPermission() == android.content.pm.PackageManager.PERMISSION_GRANTED
        } catch (_: Exception) { false }

        /** Show the Shizuku permission dialog. No-op if pre-v11 or unavailable. */
        @JvmStatic
        fun requestShizukuPermission() {
            try {
                if (!Shizuku.isPreV11()) Shizuku.requestPermission(SHIZUKU_REQ_CODE)
            } catch (e: Exception) {
                android.util.Log.e("CheatsManager", "requestShizukuPermission: $e")
            }
        }

        // ── Shizuku file bridge ────────────────────────────────────────────────
        // Each method spawns a shell command via Shizuku (uid=2000 / ADB-level).
        // Returns null / false on any failure so Rust can propagate a clean error.

        /** Read a file's full text content via `cat`. Returns null on error. */
        @JvmStatic
        fun shizukuReadFile(path: String): String? = try {
            val p = Shizuku.newProcess(arrayOf("cat", path), null, null)
            val out = p.inputStream.bufferedReader().readText()
            val exit = p.waitFor()
            if (exit == 0) out else null
        } catch (e: Exception) {
            android.util.Log.e("CheatsManager", "shizukuReadFile($path): $e")
            null
        }

        /** List directory entries, one name per line. Returns "" on error/empty. */
        @JvmStatic
        fun shizukuListDir(path: String): String = try {
            val p = Shizuku.newProcess(arrayOf("ls", "-1", path), null, null)
            val out = p.inputStream.bufferedReader().readText()
            p.waitFor()
            out
        } catch (e: Exception) {
            android.util.Log.e("CheatsManager", "shizukuListDir($path): $e")
            ""
        }

        /**
         * Find all .txt files under `dir` recursively.
         * Returns newline-separated absolute paths, "" on error.
         */
        @JvmStatic
        fun shizukuFindTxtFiles(dir: String): String = try {
            val p = Shizuku.newProcess(
                arrayOf("find", dir, "-name", "*.txt", "-type", "f"), null, null
            )
            val out = p.inputStream.bufferedReader().readText()
            p.waitFor()
            out
        } catch (e: Exception) {
            android.util.Log.e("CheatsManager", "shizukuFindTxtFiles($dir): $e")
            ""
        }

        /**
         * Write `content` to `path` via `tee`. Parent directory must already exist.
         * Returns true on success.
         */
        @JvmStatic
        fun shizukuWriteFile(path: String, content: String): Boolean = try {
            val p = Shizuku.newProcess(arrayOf("tee", path), null, null)
            p.outputStream.use { it.write(content.toByteArray(Charsets.UTF_8)) }
            p.waitFor() == 0
        } catch (e: Exception) {
            android.util.Log.e("CheatsManager", "shizukuWriteFile($path): $e")
            false
        }

        /** Delete a file with `rm -f` (ignores not-found). Returns true on success. */
        @JvmStatic
        fun shizukuDeleteFile(path: String): Boolean = try {
            Shizuku.newProcess(arrayOf("rm", "-f", path), null, null).waitFor() == 0
        } catch (_: Exception) { false }

        /**
         * Remove an *empty* directory with `rmdir`.
         * Succeeds silently if non-empty or not found — used for best-effort cleanup.
         */
        @JvmStatic
        fun shizukuRmdir(path: String): Boolean = try {
            Shizuku.newProcess(arrayOf("rmdir", path), null, null).waitFor() == 0
        } catch (_: Exception) { false }

        /** Create directory tree with `mkdir -p`. Returns true on success. */
        @JvmStatic
        fun shizukuMkdirs(path: String): Boolean = try {
            Shizuku.newProcess(arrayOf("mkdir", "-p", path), null, null).waitFor() == 0
        } catch (_: Exception) { false }

        /** Return true if `path` exists (file or directory). */
        @JvmStatic
        fun shizukuPathExists(path: String): Boolean = try {
            Shizuku.newProcess(arrayOf("test", "-e", path), null, null).waitFor() == 0
        } catch (_: Exception) { false }

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

        /**
         * Returns true if MANAGE_EXTERNAL_STORAGE is granted (Android 11+),
         * or unconditionally true on older APIs where scoped storage doesn't apply.
         * Called from Rust via JNI to check permission without path probing.
         */
        @JvmStatic
        fun hasAllFilesAccess(): Boolean {
            return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                Environment.isExternalStorageManager()
            } else {
                true
            }
        }

        /**
         * Open the system "All files access" page for this app.
         * MANAGE_EXTERNAL_STORAGE is a special permission — it lives under
         * "Special app access", not the normal "Permissions" screen.
         * Falls back to the global special-access list if the per-app page fails.
         */
        @JvmStatic
        fun openStorageSettings() {
            val activity = instance?.get() ?: run {
                android.util.Log.e("CheatsManager", "openStorageSettings: no activity reference")
                return
            }
            try {
                val intent = Intent(
                    Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:${activity.packageName}")
                )
                activity.startActivity(intent)
                android.util.Log.i("CheatsManager", "openStorageSettings: per-app page opened")
            } catch (e: Exception) {
                android.util.Log.w("CheatsManager", "openStorageSettings: per-app failed, trying global: $e")
                try {
                    activity.startActivity(Intent(Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION))
                } catch (e2: Exception) {
                    android.util.Log.e("CheatsManager", "openStorageSettings: both intents failed: $e2")
                }
            }
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
}
