package dev.eden.cheats_manager

import android.content.Intent
import android.content.pm.PackageInfo
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import java.io.File
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit

internal data class UpdateIdentity(
    val packageName: String,
    val versionName: String?,
    val versionCode: Long,
    val signers: Set<String>,
)

internal fun validateAppUpdate(installed: UpdateIdentity, candidate: UpdateIdentity, expectedVersion: String) {
    require(candidate.packageName == installed.packageName) { "The downloaded APK is not EDC." }
    require(expectedVersion.isNotBlank() && candidate.versionName == expectedVersion) {
        "The APK version does not match the selected release. Check for updates again."
    }
    require(candidate.versionCode > installed.versionCode) { "The APK is not newer than the installed app." }
    require(candidate.signers.isNotEmpty() && candidate.signers == installed.signers) {
        "The APK signing certificate does not match this installation."
    }
}

internal class AppUpdateInstaller(private val activity: MainActivity) {
    private fun updateFile() = File(activity.cacheDir, "updates/eden-cheats-manager.apk")

    fun getDownloadPath(): String = try {
        val file = updateFile()
        check(file.parentFile!!.isDirectory || file.parentFile!!.mkdirs()) {
            "Could not create the app update cache."
        }
        file.absolutePath
    } catch (error: Exception) {
        "ERROR: ${error.message}"
    }

    @Suppress("DEPRECATION")
    fun install(expectedVersion: String): String = try {
        val file = updateFile()
        require(file.isFile && file.length() > 0) { "Download the app update before installing it." }
        val manager = activity.packageManager
        val flags = if (Build.VERSION.SDK_INT >= 28) {
            PackageManager.GET_SIGNING_CERTIFICATES
        } else {
            PackageManager.GET_SIGNATURES
        }
        val candidate = manager.getPackageArchiveInfo(file.absolutePath, flags)
            ?: throw IllegalArgumentException("The downloaded APK could not be read.")
        val installed = manager.getPackageInfo(activity.packageName, flags)
        validateAppUpdate(identity(installed), identity(candidate), expectedVersion)

        if (Build.VERSION.SDK_INT >= 26 && !manager.canRequestPackageInstalls()) {
            launch(
                Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${activity.packageName}")),
                "permissionRequired",
            )
        } else {
            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
            launch(
                Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "application/vnd.android.package-archive")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
                "installerOpened",
            )
        }
    } catch (error: Exception) {
        "ERROR: ${error.cause?.message ?: error.message ?: "Could not open the Android update screen. Try again."}"
    }

    @Suppress("DEPRECATION")
    private fun identity(info: PackageInfo): UpdateIdentity {
        val signers = if (Build.VERSION.SDK_INT >= 28) info.signingInfo?.apkContentsSigners else info.signatures
        return UpdateIdentity(
            info.packageName,
            info.versionName,
            if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong(),
            signers?.map { it.toCharsString() }?.toSet() ?: emptySet(),
        )
    }

    private fun launch(intent: Intent, result: String): String {
        val task = FutureTask {
            check(!activity.isFinishing && !activity.isDestroyed) { "Main activity unavailable." }
            activity.startActivity(intent)
            result
        }
        activity.runOnUiThread(task)
        return try {
            task.get(10, TimeUnit.SECONDS)
        } catch (error: Exception) {
            task.cancel(false)
            if (error is InterruptedException) Thread.currentThread().interrupt()
            throw error
        }
    }
}
