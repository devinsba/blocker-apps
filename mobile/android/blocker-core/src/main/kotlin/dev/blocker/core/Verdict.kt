package dev.blocker.core

/** The engine's decision for a host or flow. */
sealed class Verdict {
    /** Let it through. */
    data object Allow : Verdict()

    /** Blocked by [rule]; [matched] is the domain / app id that matched. */
    data class Block(val rule: String, val matched: String) : Verdict()

    /**
     * The hostname is unknown: read the flow's first bytes, run
     * [BlockerEngine.peekHost] on them and call [BlockerEngine.decide] again
     * with `peeked = true`.
     */
    data object Peek : Verdict()

    val isBlocked: Boolean get() = this is Block

    companion object {
        /** Decodes the `String[3]` the JNI shim returns. */
        @JvmStatic
        internal fun fromNative(parts: Array<String?>): Verdict = when (parts.getOrNull(0)) {
            "allow" -> Allow
            "peek" -> Peek
            "block" -> Block(rule = parts.getOrNull(1).orEmpty(), matched = parts.getOrNull(2).orEmpty())
            else -> throw BlockerException("unexpected verdict from native code: ${parts.toList()}")
        }
    }
}
