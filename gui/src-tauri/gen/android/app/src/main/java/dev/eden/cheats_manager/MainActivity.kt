package dev.eden.cheats_manager

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import java.lang.ref.WeakReference

class MainActivity : TauriActivity() {
    private val edenSafStorage = EdenSafStorage(this)
    private val edenRootDiscovery = EdenRootDiscovery(this)

    companion object {
        @Volatile
        private var instance: WeakReference<MainActivity>? = null

        @JvmStatic
        fun selectEdenLoadDirectory() {
            instance?.get()?.edenSafStorage?.selectLoadDirectory()
        }

        @JvmStatic
        fun getEdenLoadAccessStatus(): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenSafStorage.getAccessStatus()
        }

        @JvmStatic
        fun selectEdenRootDirectory() {
            instance?.get()?.edenRootDiscovery?.selectRootDirectory()
        }

        @JvmStatic
        fun inspectEdenInstallation(): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenRootDiscovery.inspectInstallation()
        }

        @JvmStatic
        fun safListDirectory(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenSafStorage.listDirectory(relativePath)
        }

        @JvmStatic
        fun safWriteTextFile(relativePath: String, content: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenSafStorage.writeTextFile(relativePath, content)
        }

        @JvmStatic
        fun safDeleteFile(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenSafStorage.deleteFile(relativePath)
        }

        @JvmStatic
        fun safRemoveEmptyDirectory(relativePath: String): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.edenSafStorage.removeEmptyDirectory(relativePath)
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        instance = WeakReference(this)
    }

    override fun onDestroy() {
        super.onDestroy()
        instance = null
    }
}
