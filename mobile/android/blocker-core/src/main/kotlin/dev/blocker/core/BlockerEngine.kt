package dev.blocker.core

import java.util.concurrent.locks.ReentrantReadWriteLock
import kotlin.concurrent.read
import kotlin.concurrent.write

/**
 * A compiled set of blocking rules, backed by a Rust `BlockerEngine`.
 *
 * Immutable and thread-safe: any thread may query it concurrently. Call
 * [close] (or use `use {}`) to free the native engine; later calls throw
 * [IllegalStateException]. To change rules, build a new engine and close the
 * old one.
 *
 * Every query takes the [WeekTime] to evaluate schedules at; it defaults to
 * the device's local time now.
 */
class BlockerEngine private constructor(handle: Long) : AutoCloseable {
    private val lock = ReentrantReadWriteLock()
    private var handle: Long = handle

    val isClosed: Boolean get() = lock.read { handle == 0L }

    /** Problems found while compiling (e.g. missing category lists). */
    val warnings: List<String>
        get() = withHandle { Json.stringList(NativeBridge.warningsJson(it)) }

    /** Would [host] be blocked at [at]? */
    @JvmOverloads
    fun checkHost(host: String, at: WeekTime = WeekTime.now()): Verdict = withHandle {
        Verdict.fromNative(NativeBridge.checkHost(it, host, at.day, at.minute))
    }

    /**
     * Decides a network flow. If the result is [Verdict.Peek], read the first
     * bytes of the flow, call [peekHost] and decide again with
     * `peeked = true` and the host it found (or null).
     */
    @JvmOverloads
    fun decide(
        flow: Flow,
        peeked: Boolean = false,
        peekedHost: String? = null,
        at: WeekTime = WeekTime.now(),
    ): Verdict = withHandle {
        Verdict.fromNative(
            NativeBridge.decide(
                it, flow.hostname, flow.remoteIp, flow.remotePort, flow.proto.code,
                flow.appId, flow.ech, peeked, peekedHost, at.day, at.minute,
            ),
        )
    }

    /** True if no rule is active at [at] (the VPN can pass traffic untouched). */
    @JvmOverloads
    fun isIdle(at: WeekTime = WeekTime.now()): Boolean = withHandle {
        NativeBridge.isIdle(it, at.day, at.minute)
    }

    /** Names of rules active at [at]. */
    @JvmOverloads
    fun activeRules(at: WeekTime = WeekTime.now()): List<String> = withHandle {
        Json.stringList(NativeBridge.activeRulesJson(it, at.day, at.minute))
    }

    /** Apps blocked by rules active at [at]. */
    @JvmOverloads
    fun blockedApps(at: WeekTime = WeekTime.now()): BlockedApps = withHandle {
        Json.blockedApps(NativeBridge.blockedAppsJson(it, at.day, at.minute))
    }

    override fun close() {
        lock.write {
            if (handle != 0L) {
                NativeBridge.engineFree(handle)
                handle = 0L
            }
        }
    }

    private inline fun <T> withHandle(block: (Long) -> T): T = lock.read {
        check(handle != 0L) { "BlockerEngine is closed" }
        block(handle)
    }

    companion object {
        /** Version of the Rust core. */
        @JvmStatic
        val version: String get() = NativeBridge.version()

        /** Compiles `config.toml` text (domain rules only; no category lists). */
        @JvmStatic
        fun fromToml(toml: String): BlockerEngine = BlockerEngine(NativeBridge.engineFromToml(toml))

        /** Compiles `<dir>/config.toml` plus downloaded `<dir>/lists/<category>.fst`. */
        @JvmStatic
        fun openDir(dir: String): BlockerEngine = BlockerEngine(NativeBridge.engineOpenDir(dir))

        /**
         * Normalizes a user-entered domain or URL
         * (`"https://WWW.Example.com/x"` -> `"www.example.com"`), or null if invalid.
         */
        @JvmStatic
        fun normalizeDomain(input: String): String? = NativeBridge.normalizeDomain(input)

        /** Hostname from the first outbound bytes of a TCP stream (TLS SNI / HTTP Host). */
        @JvmStatic
        fun peekHost(bytes: ByteArray): PeekResult {
            val wantMore = IntArray(1)
            val host = NativeBridge.peekHost(bytes, wantMore)
            return PeekResult(host, wantMore[0])
        }

        /** Returns null if [toml] is a valid config, otherwise the parse error. */
        @JvmStatic
        fun validateConfig(toml: String): String? = NativeBridge.validateConfig(toml)
    }
}
