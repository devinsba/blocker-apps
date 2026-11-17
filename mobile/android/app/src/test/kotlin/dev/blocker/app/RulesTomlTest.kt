package dev.blocker.app

import dev.blocker.app.data.ConfigRepository
import dev.blocker.app.data.RuleSummary
import dev.blocker.app.data.RulesToml
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class RulesTomlTest {
    @Test
    fun parsesDocumentedExample() {
        val text = """
            [settings]
            block_quic = true

            [server]
            url = "https://lists.example.com"
            public_key = "AAAA"

            [[rule]]
            name = "focus" # trailing comment
            domains = ["youtube.com", "reddit.com"]
            categories = ["social", "ads"]
            bundle_ids = ["com.valvesoftware.steam"]
            process_names = ["Discord"]
            schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }
            locked = true
        """.trimIndent()
        assertEquals(
            listOf(
                RuleSummary(
                    name = "focus",
                    locked = true,
                    domains = listOf("youtube.com", "reddit.com"),
                    categories = listOf("social", "ads"),
                    bundleIds = listOf("com.valvesoftware.steam"),
                    scheduled = true,
                ),
            ),
            RulesToml.parse(text),
        )
    }

    @Test
    fun multiLineArraysCommentsAndSubTables() {
        val text = """
            [[rule]]
            name = 'literal # not a comment'
            enabled = false
            domains = [
              "a.com",  # first
              "b.com",
            ]

            [[rule]]
            name = "windows table"
            [rule.schedule]
            type = "windows"
            [[rule.schedule.windows]]
            start = "18:00"
            end = "22:00"

            [settings]
            block_ech = true
        """.trimIndent()
        val rules = RulesToml.parse(text)
        assertEquals(2, rules.size)
        assertEquals(RuleSummary(name = "literal # not a comment", enabled = false, domains = listOf("a.com", "b.com")), rules[0])
        assertEquals("windows table", rules[1].name)
        assertTrue(rules[1].scheduled)
    }

    @Test
    fun appendRoundTrips() {
        val base = "[settings]\nblock_quic = true\n"
        val name = "evil \"quotes\" \\ and émoji 🌙"
        val text = RulesToml.appendRule(base, name, listOf("x.com", "y.org"), listOf("com.example.game"))
        assertTrue(text.startsWith(base.trimEnd() + "\n\n[[rule]]\n"))
        assertEquals(
            listOf(RuleSummary(name = name, domains = listOf("x.com", "y.org"), bundleIds = listOf("com.example.game"))),
            RulesToml.parse(text),
        )
        assertEquals(listOf(name), RulesToml.parse(RulesToml.appendRule("", name, emptyList())).map { it.name })
    }

    @Test
    fun quoteEscapesControlCharacters() {
        assertEquals("\"a\\nb\\u0001\"", RulesToml.quote("a\nb\u0001"))
    }

    @Test
    fun sampleConfigParses() {
        val rules = RulesToml.parse(ConfigRepository.SAMPLE_CONFIG)
        assertEquals(listOf("focus", "always"), rules.map { it.name })
        assertTrue(rules[0].scheduled)
    }
}
