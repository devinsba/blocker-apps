package dev.blocker.app.data

/** What the rules screen shows for one `[[rule]]` in config.toml. */
data class RuleSummary(
    val name: String,
    val enabled: Boolean = true,
    val locked: Boolean = false,
    val domains: List<String> = emptyList(),
    val categories: List<String> = emptyList(),
    val bundleIds: List<String> = emptyList(),
    /** True if the rule has a `windows` schedule (false = always active). */
    val scheduled: Boolean = false,
)

/**
 * Small, display-oriented reader/writer for the `[[rule]]` tables in
 * config.toml. The Rust core stays the source of truth: every edit is checked
 * with `BlockerEngine.validateConfig` / `fromToml` before it is saved, and the
 * engine decides what is blocked.
 *
 * Handles the subset the app writes and the documented examples use: string
 * and boolean values, (multi-line) arrays of strings, inline-table schedules,
 * comments. Unknown keys are ignored.
 */
object RulesToml {
    fun parse(text: String): List<RuleSummary> {
        val rules = mutableListOf<RuleSummary>()
        var current: MutableMap<String, String>? = null
        var scheduleTable = false

        fun finish() {
            current?.let { rules += toSummary(it) }
            current = null
            scheduleTable = false
        }

        val lines = text.lines().iterator()
        while (lines.hasNext()) {
            var line = stripComment(lines.next()).trim()
            if (line.isEmpty()) continue
            if (line.startsWith("[")) {
                val header = line.trimStart('[').trimEnd(']').trim()
                when {
                    line.startsWith("[[") && header == "rule" -> {
                        finish()
                        current = mutableMapOf()
                    }
                    // [rule.schedule], [[rule.schedule.windows]], ...: still inside the rule.
                    header.startsWith("rule.") && current != null -> scheduleTable = true
                    else -> finish()
                }
                continue
            }
            val rule = current ?: continue
            if (scheduleTable) {
                // Inside [rule.schedule] etc.: only the schedule type matters for display.
                val (key, value) = splitKeyValue(line) ?: continue
                if (key == "type" && rule["schedule"] == null) rule["schedule"] = "{ type = $value }"
                continue
            }
            // Join multi-line arrays / inline tables until brackets balance.
            while (unbalanced(line) && lines.hasNext()) {
                line += " " + stripComment(lines.next()).trim()
            }
            val (key, value) = splitKeyValue(line) ?: continue
            rule[key] = value
        }
        finish()
        return rules
    }

    /** Returns [text] with a new `[[rule]]` appended. */
    fun appendRule(
        text: String,
        name: String,
        domains: List<String>,
        bundleIds: List<String> = emptyList(),
    ): String = buildString {
        append(text.trimEnd())
        if (isNotEmpty()) append("\n\n")
        append("[[rule]]\n")
        append("name = ").append(quote(name)).append('\n')
        if (domains.isNotEmpty()) append("domains = ").append(stringArray(domains)).append('\n')
        if (bundleIds.isNotEmpty()) append("bundle_ids = ").append(stringArray(bundleIds)).append('\n')
    }

    /** TOML basic string literal. */
    fun quote(s: String): String = buildString {
        append('"')
        for (c in s) {
            when {
                c == '"' -> append("\\\"")
                c == '\\' -> append("\\\\")
                c == '\n' -> append("\\n")
                c == '\t' -> append("\\t")
                c == '\r' -> append("\\r")
                c < ' ' || c == '\u007f' -> append("\\u%04X".format(c.code))
                else -> append(c)
            }
        }
        append('"')
    }

    private fun stringArray(items: List<String>) = items.joinToString(", ", "[", "]") { quote(it) }

    private fun toSummary(raw: Map<String, String>) = RuleSummary(
        name = raw["name"]?.let(::parseString).orEmpty(),
        enabled = raw["enabled"]?.trim() != "false",
        locked = raw["locked"]?.trim() == "true",
        domains = raw["domains"]?.let(::parseStringArray).orEmpty(),
        categories = raw["categories"]?.let(::parseStringArray).orEmpty(),
        bundleIds = raw["bundle_ids"]?.let(::parseStringArray).orEmpty(),
        scheduled = raw["schedule"]?.let { Regex("""type\s*=\s*["']windows["']""").containsMatchIn(it) } ?: false,
    )

    private fun splitKeyValue(line: String): Pair<String, String>? {
        val eq = line.indexOf('=')
        if (eq <= 0) return null
        val key = line.substring(0, eq).trim().removeSurrounding("\"")
        return key to line.substring(eq + 1).trim()
    }

    /** Removes a trailing `# comment` that is not inside a string. */
    internal fun stripComment(line: String): String {
        var quote: Char? = null
        var i = 0
        while (i < line.length) {
            val c = line[i]
            when {
                quote == '"' && c == '\\' -> i++ // skip escaped char
                quote != null && c == quote -> quote = null
                quote == null && (c == '"' || c == '\'') -> quote = c
                quote == null && c == '#' -> return line.substring(0, i)
            }
            i++
        }
        return line
    }

    private fun unbalanced(s: String): Boolean {
        var depth = 0
        var quote: Char? = null
        var i = 0
        while (i < s.length) {
            val c = s[i]
            when {
                quote == '"' && c == '\\' -> i++
                quote != null && c == quote -> quote = null
                quote == null && (c == '"' || c == '\'') -> quote = c
                quote == null && (c == '[' || c == '{') -> depth++
                quote == null && (c == ']' || c == '}') -> depth--
            }
            i++
        }
        return depth > 0
    }

    /** Parses one TOML string (basic or literal) starting at the beginning of [s]. */
    internal fun parseString(s: String): String = readString(s.trim(), 0)?.first.orEmpty()

    internal fun parseStringArray(s: String): List<String> {
        val t = s.trim()
        if (!t.startsWith("[")) return emptyList()
        val out = mutableListOf<String>()
        var i = 1
        while (i < t.length) {
            when (t[i]) {
                '"', '\'' -> {
                    val (value, end) = readString(t, i) ?: return out
                    out += value
                    i = end
                }
                ']' -> return out
                else -> i++
            }
        }
        return out
    }

    /** Reads a string starting at [start]; returns the value and the index after it. */
    private fun readString(s: String, start: Int): Pair<String, Int>? {
        if (start >= s.length) return null
        val q = s[start]
        if (q != '"' && q != '\'') return null
        val sb = StringBuilder()
        var i = start + 1
        while (i < s.length) {
            val c = s[i]
            if (c == q) return sb.toString() to i + 1
            if (q == '"' && c == '\\' && i + 1 < s.length) {
                i++
                when (val e = s[i]) {
                    'n' -> sb.append('\n')
                    't' -> sb.append('\t')
                    'r' -> sb.append('\r')
                    'b' -> sb.append('\b')
                    'f' -> sb.append('\u000c')
                    'u', 'U' -> {
                        val len = if (e == 'u') 4 else 8
                        val code = s.substring(i + 1, minOf(s.length, i + 1 + len)).toIntOrNull(16)
                        if (code != null) sb.appendCodePoint(code)
                        i += len
                    }
                    else -> sb.append(e)
                }
            } else {
                sb.append(c)
            }
            i++
        }
        return null
    }
}
