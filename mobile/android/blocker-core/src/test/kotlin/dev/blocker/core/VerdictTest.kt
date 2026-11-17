package dev.blocker.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class VerdictTest {
    @Test
    fun decodesNativeArrays() {
        assertEquals(Verdict.Allow, Verdict.fromNative(arrayOf("allow", null, null)))
        assertEquals(Verdict.Peek, Verdict.fromNative(arrayOf("peek", null, null)))
        val block = Verdict.fromNative(arrayOf("block", "focus", "example.com"))
        assertEquals(Verdict.Block("focus", "example.com"), block)
        assertTrue(block.isBlocked)
        assertFalse(Verdict.Allow.isBlocked)
    }

    @Test
    fun rejectsUnknownKinds() {
        assertThrows(BlockerException::class.java) { Verdict.fromNative(arrayOf("maybe", null, null)) }
        assertThrows(BlockerException::class.java) { Verdict.fromNative(arrayOf()) }
    }

    @Test
    fun flowValidatesPort() {
        assertThrows(IllegalArgumentException::class.java) { Flow(remotePort = 70000) }
        assertEquals(1, Proto.UDP.code)
    }
}
