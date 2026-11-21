package dev.blocker.app.data

import dev.blocker.core.BlockerEngine
import java.io.File

/**
 * Owns `<filesDir>/blocker/config.toml` (plus `lists/` for downloaded
 * category sets, laid out as blocker-core's `Store` expects).
 *
 * Shared by the UI and [dev.blocker.app.vpn.BlockerVpnService], which both
 * build engines with [openEngine].
 */
class ConfigRepository(filesDir: File) {
    val dir: File = File(filesDir, "blocker")
    val configFile: File = File(dir, "config.toml")

    /** Writes [SAMPLE_CONFIG] if there is no config yet. */
    fun seedIfMissing() {
        if (!configFile.exists()) {
            write(SAMPLE_CONFIG)
        }
    }

    fun read(): String {
        seedIfMissing()
        return configFile.readText()
    }

    /**
     * Validates [text] with the Rust core (parse + compile) and atomically
     * replaces config.toml. Throws [dev.blocker.core.BlockerException] if invalid.
     */
    fun save(text: String) {
        BlockerEngine.validateConfig(text)?.let { throw dev.blocker.core.BlockerException(it) }
        BlockerEngine.fromToml(text).close() // also catches compile errors (bad CIDRs, ...)
        write(text)
    }

    /** Compiles config.toml plus any downloaded lists. Caller closes the engine. */
    fun openEngine(): BlockerEngine {
        seedIfMissing()
        return BlockerEngine.openDir(dir.path)
    }

    private fun write(text: String) {
        dir.mkdirs()
        val tmp = File(dir, "config.toml.tmp")
        tmp.writeText(text)
        if (!tmp.renameTo(configFile)) {
            tmp.delete()
            throw java.io.IOException("could not replace $configFile")
        }
    }

    companion object {
        val SAMPLE_CONFIG = """
            # Blocker rules. Same format as the desktop apps; see blocker-core config.rs.
            # On Android, bundle_ids are package names.

            [settings]
            block_quic = true

            [[rule]]
            name = "focus"
            domains = ["reddit.com", "news.ycombinator.com"]
            schedule = { type = "windows", windows = [{ days = ["mon", "tue", "wed", "thu", "fri"], start = "09:00", end = "17:00" }] }

            [[rule]]
            name = "always"
            domains = ["doubleclick.net"]
        """.trimIndent() + "\n"
    }
}
