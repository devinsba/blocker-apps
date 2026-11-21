package dev.blocker.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

class JsonTest {
    @Test
    fun stringArrays() {
        assertEquals(emptyList<String>(), Json.stringList("[]"))
        assertEquals(listOf("focus", "évening 🌙"), Json.stringList(""" [ "focus" , "évening 🌙" ] """))
        assertEquals(listOf("a\"b", "c\\d", "é\n"), Json.stringList("""["a\"b","c\\d","é\n"]"""))
    }

    @Test
    fun blockedApps() {
        val apps = Json.blockedApps("""{"bundle_ids":["com.example.game"],"process_names":[]}""")
        assertEquals(BlockedApps(listOf("com.example.game"), emptyList()), apps)
        // Missing keys default to empty.
        assertEquals(BlockedApps(emptyList(), emptyList()), Json.blockedApps("{}"))
    }

    @Test
    fun scalarsAndNesting() {
        assertEquals(mapOf("a" to listOf(1.0, true, null), "b" to mapOf("c" to "d")),
            Json.parse("""{"a":[1,true,null],"b":{"c":"d"}}"""))
        assertNull(Json.parse("null"))
        assertEquals(-2.5e3, Json.parse("-2.5e3"))
    }

    @Test
    fun rejectsMalformedInput() {
        for (bad in listOf("", "[", "[1,]", "{\"a\" 1}", "\"abc", "[1] x", "tru")) {
            assertThrows("input: $bad", BlockerException::class.java) { Json.parse(bad) }
        }
        assertThrows(BlockerException::class.java) { Json.stringList("[1]") }
        assertThrows(BlockerException::class.java) { Json.blockedApps("[]") }
    }
}
