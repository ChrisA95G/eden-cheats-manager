package dev.eden.cheats_manager

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri

/**
 * Persisted tree grants shared by the cheat install folder (read and write) and the game
 * library (read). Both settings may point at the same tree, so neither releases a flag the
 * other still needs.
 */
internal object SafGrants {
    const val READ = Intent.FLAG_GRANT_READ_URI_PERMISSION
    const val WRITE = Intent.FLAG_GRANT_WRITE_URI_PERMISSION

    /** The flags of [flags] that [uri] does not hold yet, which taking them would acquire. */
    fun missingFlags(resolver: ContentResolver, uri: Uri, flags: Int): Int {
        val held = resolver.persistedUriPermissions.firstOrNull { it.uri == uri }
        var missing = flags
        if (held?.isReadPermission == true) missing = missing and READ.inv()
        if (held?.isWritePermission == true) missing = missing and WRITE.inv()
        return missing
    }

    /** Release [flags] from [uri], except those in [keep]. */
    fun release(resolver: ContentResolver, uri: Uri, flags: Int, keep: Int = 0) {
        val released = flags and keep.inv()
        if (released == 0) return
        try {
            resolver.releasePersistableUriPermission(uri, released)
        } catch (error: SecurityException) {
            android.util.Log.w("CheatsManager", "Could not release a SAF permission", error)
        }
    }
}
