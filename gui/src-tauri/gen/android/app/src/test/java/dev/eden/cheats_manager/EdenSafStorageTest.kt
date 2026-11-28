package dev.eden.cheats_manager

import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class EdenSafStorageTest {
    @Test fun directInstallsRequireTheSelectedLoadFolder() {
        assertFalse(EdenSafStorage.needsLoadChild("load", EdenSafStorage.MODE_DIRECT))
        for (name in listOf("0100000000000000", "Emulator data")) {
            assertThrows(IllegalArgumentException::class.java) {
                EdenSafStorage.needsLoadChild(name, EdenSafStorage.MODE_DIRECT)
            }
        }
    }

    @Test fun importsUseLoadItselfOrItsChild() {
        assertFalse(EdenSafStorage.needsLoadChild("load", EdenSafStorage.MODE_CUSTOM))
        assertTrue(EdenSafStorage.needsLoadChild("Cheat exports", EdenSafStorage.MODE_CUSTOM))
    }
}
