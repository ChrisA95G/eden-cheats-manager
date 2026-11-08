package dev.eden.cheats_manager

import org.junit.Assert.assertThrows
import org.junit.Test

class AppUpdateInstallerTest {
    private val installed = UpdateIdentity("dev.eden.cheats_manager", "0.2.1", 2003, setOf("release-key"))
    private val update = installed.copy(versionName = "0.3.0", versionCode = 3000)

    @Test fun acceptsNewerReleaseWithTheSameIdentity() {
        validateAppUpdate(installed, update, "0.3.0")
    }

    @Test fun rejectsAnotherAppOrReleaseVersion() {
        for (candidate in listOf(
            update.copy(packageName = "dev.eden.other"),
            update.copy(versionName = "0.4.0"),
            update.copy(versionName = null),
        )) {
            assertThrows(IllegalArgumentException::class.java) { validateAppUpdate(installed, candidate, "0.3.0") }
        }
        assertThrows(IllegalArgumentException::class.java) { validateAppUpdate(installed, update, "") }
    }

    @Test fun rejectsReinstallAndDowngradeEvenWithANewerVersionName() {
        for (code in listOf(2003L, 2002L)) {
            assertThrows(IllegalArgumentException::class.java) {
                validateAppUpdate(installed, update.copy(versionCode = code), "0.3.0")
            }
        }
    }

    @Test fun rejectsMissingDifferentAndAdditionalSigners() {
        for (signers in listOf(emptySet(), setOf("debug-key"), setOf("release-key", "another-key"))) {
            assertThrows(IllegalArgumentException::class.java) {
                validateAppUpdate(installed, update.copy(signers = signers), "0.3.0")
            }
        }
        assertThrows(IllegalArgumentException::class.java) {
            validateAppUpdate(installed.copy(signers = emptySet()), update.copy(signers = emptySet()), "0.3.0")
        }
    }

    @Test fun acceptsMatchingMultipleSignersRegardlessOfOrder() {
        validateAppUpdate(
            installed.copy(signers = linkedSetOf("key-a", "key-b")),
            update.copy(signers = linkedSetOf("key-b", "key-a")),
            "0.3.0",
        )
    }

    @Test fun comparesFullVersionCodesWithoutOverflow() {
        validateAppUpdate(installed.copy(versionCode = Int.MAX_VALUE.toLong()),
            update.copy(versionCode = Int.MAX_VALUE.toLong() + 1), "0.3.0")
    }
}
