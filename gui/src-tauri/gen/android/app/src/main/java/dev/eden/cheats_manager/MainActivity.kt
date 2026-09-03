package dev.eden.cheats_manager

import android.os.Bundle
import androidx.activity.enableEdgeToEdge
import java.lang.ref.WeakReference

class MainActivity : TauriActivity() {
    private val edenSafStorage = EdenSafStorage(this)
    private val gameLibraryStorage = GameLibraryStorage(this)
    private val packageDocumentStorage = PackageDocumentStorage(this)

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

        @JvmStatic
        fun selectGameLibraryDirectory() {
            instance?.get()?.gameLibraryStorage?.selectDirectory()
        }

        @JvmStatic
        fun getGameLibraryStatus(): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.gameLibraryStorage.getStatus()
        }

        @JvmStatic
        fun listGameLibraryPackages(): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.gameLibraryStorage.listPackages()
        }

        @JvmStatic
        fun openGameLibraryPackageReadFd(relativePath: String): Int {
            val activity = instance?.get()
                ?: return PackageDocumentStorage.FD_ERROR_UNAVAILABLE
            return activity.gameLibraryStorage.openPackageReadFd(relativePath)
        }

        @JvmStatic
        fun selectProdKeysDocument() {
            instance?.get()?.packageDocumentStorage?.selectProdKeys()
        }

        @JvmStatic
        fun selectGamePackageDocument() {
            instance?.get()?.packageDocumentStorage?.selectGamePackage()
        }

        @JvmStatic
        fun getPackageDiscoveryStatus(): String {
            val activity = instance?.get()
                ?: return "ERROR: Main activity unavailable"
            return activity.packageDocumentStorage.getStatus()
        }

        @JvmStatic
        fun openProdKeysReadFd(): Int {
            val activity = instance?.get()
                ?: return PackageDocumentStorage.FD_ERROR_UNAVAILABLE
            return activity.packageDocumentStorage.openProdKeysReadFd()
        }

        @JvmStatic
        fun openGamePackageReadFd(): Int {
            val activity = instance?.get()
                ?: return PackageDocumentStorage.FD_ERROR_UNAVAILABLE
            return activity.packageDocumentStorage.openGamePackageReadFd()
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
