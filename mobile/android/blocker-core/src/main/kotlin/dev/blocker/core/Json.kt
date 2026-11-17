package dev.blocker.core

/**
 * Minimal JSON reader for the small documents blocker.h returns (arrays of
 * strings and `{"bundle_ids": [...], "process_names": [...]}`).
 *
 * Pure Kotlin so it runs in JVM unit tests (android.jar's org.json is a stub
 * there) and adds no dependency to the library.
 *
 * Values map to: object -> Map<String, Any?>, array -> List<Any?>,
 * string -> String, number -> Double, true/false -> Boolean, null -> null.
 */
internal object Json {
    fun parse(text: String): Any? {
        val reader = Reader(text)
        reader.skipWhitespace()
        val value = reader.readValue()
        reader.skipWhitespace()
        if (!reader.atEnd()) reader.fail("trailing characters")
        return value
    }

    fun stringList(text: String): List<String> = asStringList(parse(text), "document")

    fun blockedApps(text: String): BlockedApps {
        val obj = parse(text) as? Map<*, *> ?: throw BlockerException("expected a JSON object: $text")
        return BlockedApps(
            bundleIds = obj["bundle_ids"]?.let { asStringList(it, "bundle_ids") }.orEmpty(),
            processNames = obj["process_names"]?.let { asStringList(it, "process_names") }.orEmpty(),
        )
    }

    private fun asStringList(value: Any?, what: String): List<String> {
        val list = value as? List<*> ?: throw BlockerException("expected a JSON array for $what")
        return list.map { it as? String ?: throw BlockerException("expected strings in $what") }
    }

    private class Reader(private val s: String) {
        private var i = 0

        fun atEnd() = i >= s.length

        fun fail(msg: String): Nothing = throw BlockerException("invalid JSON at $i: $msg")

        fun skipWhitespace() {
            while (i < s.length && s[i] in " \t\r\n") i++
        }

        fun readValue(): Any? {
            if (atEnd()) fail("unexpected end")
            return when (val c = s[i]) {
                '{' -> readObject()
                '[' -> readArray()
                '"' -> readString()
                't' -> literal("true", true)
                'f' -> literal("false", false)
                'n' -> literal("null", null)
                else -> if (c == '-' || c.isDigit()) readNumber() else fail("unexpected '$c'")
            }
        }

        private fun literal(word: String, value: Any?): Any? {
            if (!s.startsWith(word, i)) fail("expected $word")
            i += word.length
            return value
        }

        private fun expect(c: Char) {
            if (atEnd() || s[i] != c) fail("expected '$c'")
            i++
        }

        private fun readObject(): Map<String, Any?> {
            expect('{')
            val out = LinkedHashMap<String, Any?>()
            skipWhitespace()
            if (!atEnd() && s[i] == '}') {
                i++
                return out
            }
            while (true) {
                skipWhitespace()
                val key = readString()
                skipWhitespace()
                expect(':')
                skipWhitespace()
                out[key] = readValue()
                skipWhitespace()
                if (atEnd()) fail("unterminated object")
                when (s[i++]) {
                    ',' -> continue
                    '}' -> return out
                    else -> fail("expected ',' or '}'")
                }
            }
        }

        private fun readArray(): List<Any?> {
            expect('[')
            val out = ArrayList<Any?>()
            skipWhitespace()
            if (!atEnd() && s[i] == ']') {
                i++
                return out
            }
            while (true) {
                skipWhitespace()
                out += readValue()
                skipWhitespace()
                if (atEnd()) fail("unterminated array")
                when (s[i++]) {
                    ',' -> continue
                    ']' -> return out
                    else -> fail("expected ',' or ']'")
                }
            }
        }

        private fun readString(): String {
            expect('"')
            val sb = StringBuilder()
            while (true) {
                if (atEnd()) fail("unterminated string")
                when (val c = s[i++]) {
                    '"' -> return sb.toString()
                    '\\' -> {
                        if (atEnd()) fail("bad escape")
                        when (val e = s[i++]) {
                            '"', '\\', '/' -> sb.append(e)
                            'b' -> sb.append('\b')
                            'f' -> sb.append('\u000c')
                            'n' -> sb.append('\n')
                            'r' -> sb.append('\r')
                            't' -> sb.append('\t')
                            'u' -> {
                                if (i + 4 > s.length) fail("bad \\u escape")
                                val code = s.substring(i, i + 4).toIntOrNull(16) ?: fail("bad \\u escape")
                                sb.append(code.toChar())
                                i += 4
                            }
                            else -> fail("bad escape '\\$e'")
                        }
                    }
                    else -> sb.append(c)
                }
            }
        }

        private fun readNumber(): Double {
            val start = i
            while (i < s.length && (s[i].isDigit() || s[i] in "+-.eE")) i++
            return s.substring(start, i).toDoubleOrNull() ?: fail("bad number")
        }
    }
}
