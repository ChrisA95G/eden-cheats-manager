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
import androidx.activity.enableEdgeToEdge
import androidx.core.app.NotificationCompat
import java.lang.ref.WeakReference

class MainActivity : TauriActivity() {
    companion object {
        @Volatile private var instance: WeakReference<MainActivity>? = null
        @Volatile private var killEdenOnResume = false

        private const val ALERT_CHANNEL_ID       = "scan_alert_channel"
        private const val RETURN_NOTIFICATION_ID = 43
        private const val RETURN_REQUEST_CODE    = 1

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
}
