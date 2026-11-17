package dev.blocker.core

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Runs the real libblocker_jni.so (JNI shim + Rust core) on a device/emulator. */
@RunWith(AndroidJUnit4::class)
class BlockerEngineInstrumentedTest {
    private val config = """
        [[rule]]
        name = "focus"
        domains = ["example.com", "reddit.com"]
        bundle_ids = ["com.example.game"]

        [[rule]]
        name = "evening"
        domains = ["news.example.org"]
        schedule = { type = "windows", windows = [{ start = "18:00", end = "22:00" }] }
    """.trimIndent()

    private val mondayMorning = WeekTime(0, 10 * 60)
    private val mondayEvening = WeekTime(0, 19 * 60)

    @Test
    fun version() {
        assertTrue(BlockerEngine.version.isNotEmpty())
    }

    @Test
    fun checkHost() {
        BlockerEngine.fromToml(config).use { engine ->
            assertEquals(Verdict.Block("focus", "example.com"), engine.checkHost("www.example.com", mondayMorning))
            assertEquals(Verdict.Allow, engine.checkHost("example.net", mondayMorning))
            assertEquals(Verdict.Allow, engine.checkHost("news.example.org", mondayMorning))
            assertTrue(engine.checkHost("news.example.org", mondayEvening).isBlocked)
        }
    }

    @Test
    fun activeRulesAndApps() {
        BlockerEngine.fromToml(config).use { engine ->
            assertEquals(listOf("focus"), engine.activeRules(mondayMorning))
            assertEquals(listOf("focus", "evening"), engine.activeRules(mondayEvening))
            assertEquals(listOf("com.example.game"), engine.blockedApps(mondayMorning).bundleIds)
            assertFalse(engine.isIdle(mondayMorning))
            assertEquals(emptyList<String>(), engine.warnings)
        }
    }

    @Test
    fun decideWithPeek() {
        BlockerEngine.fromToml(config).use { engine ->
            val flow = Flow(remoteIp = "93.184.216.34", remotePort = 443, proto = Proto.TCP, appId = "com.android.chrome")
            assertEquals(Verdict.Peek, engine.decide(flow, at = mondayMorning))
            val http = "GET / HTTP/1.1\r\nHost: reddit.com\r\n\r\n".toByteArray()
            val peek = BlockerEngine.peekHost(http)
            assertEquals("reddit.com", peek.host)
            assertTrue(engine.decide(flow, peeked = true, peekedHost = peek.host, at = mondayMorning).isBlocked)
        }
    }

    @Test
    fun staticHelpers() {
        assertEquals("www.example.com", BlockerEngine.normalizeDomain("https://WWW.Example.com/x"))
        assertNull(BlockerEngine.normalizeDomain("not a domain!"))
        assertNull(BlockerEngine.validateConfig(config))
        assertNotNull(BlockerEngine.validateConfig("[[rule"))
        assertTrue(BlockerEngine.peekHost(byteArrayOf(0x16, 0x03, 0x01)).wantMore > 0)
    }

    @Test
    fun errorsAndClose() {
        assertThrows(BlockerException::class.java) { BlockerEngine.fromToml("[[rule") }
        val engine = BlockerEngine.fromToml(config)
        engine.close()
        engine.close() // idempotent
        assertTrue(engine.isClosed)
        assertThrows(IllegalStateException::class.java) { engine.checkHost("example.com") }
    }

    @Test
    fun openDir() {
        val dir = File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "blocker-test")
        dir.mkdirs()
        File(dir, "config.toml").writeText(config)
        BlockerEngine.openDir(dir.path).use { engine ->
            assertTrue(engine.checkHost("reddit.com", mondayMorning).isBlocked)
        }
        dir.deleteRecursively()
    }
}
