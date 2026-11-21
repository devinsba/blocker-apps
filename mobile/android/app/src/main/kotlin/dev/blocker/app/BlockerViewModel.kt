package dev.blocker.app

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.blocker.app.data.ConfigRepository
import dev.blocker.app.data.RuleSummary
import dev.blocker.app.data.RulesToml
import dev.blocker.app.vpn.BlockerVpnService
import dev.blocker.core.BlockerEngine
import dev.blocker.core.Verdict
import dev.blocker.core.WeekTime
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

data class RulesState(
    val loading: Boolean = true,
    val rules: List<RuleSummary> = emptyList(),
    val activeRules: Set<String> = emptySet(),
    val warnings: List<String> = emptyList(),
    val error: String? = null,
    val coreVersion: String = "",
    val evaluatedAt: WeekTime? = null,
)

data class CheckResult(val input: String, val host: String, val verdict: Verdict, val at: WeekTime)

class BlockerViewModel(app: Application) : AndroidViewModel(app) {
    private val repo = ConfigRepository(app.filesDir)
    private val engineLock = Mutex()
    private var engine: BlockerEngine? = null
    private var configText: String = ""

    private val _rules = MutableStateFlow(RulesState())
    val rules: StateFlow<RulesState> = _rules.asStateFlow()

    private val _check = MutableStateFlow<CheckResult?>(null)
    val check: StateFlow<CheckResult?> = _check.asStateFlow()

    val vpnRunning: StateFlow<Boolean> = BlockerVpnService.running

    init {
        reload()
    }

    /** Re-reads config.toml and recompiles the engine. */
    fun reload() {
        viewModelScope.launch(Dispatchers.IO) {
            engineLock.withLock {
                try {
                    val text = repo.read()
                    val fresh = repo.openEngine()
                    engine?.close()
                    engine = fresh
                    configText = text
                    val now = WeekTime.now()
                    _rules.value = RulesState(
                        loading = false,
                        rules = RulesToml.parse(text),
                        activeRules = fresh.activeRules(now).toSet(),
                        warnings = fresh.warnings,
                        coreVersion = BlockerEngine.version,
                        evaluatedAt = now,
                    )
                } catch (t: Throwable) {
                    // BlockerException, IOException, or UnsatisfiedLinkError if
                    // libblocker_jni.so is missing for this ABI.
                    _rules.update { it.copy(loading = false, error = t.message ?: t.toString()) }
                }
            }
        }
    }

    /** Normalizes a user-entered domain/URL; null if invalid. */
    fun normalize(input: String): String? =
        if (input.isBlank()) null else runCatching { BlockerEngine.normalizeDomain(input.trim()) }.getOrNull()

    /**
     * Appends a rule and saves it. Calls [onDone] with null on success or an
     * error message.
     */
    fun addRule(name: String, domains: List<String>, onDone: (String?) -> Unit) {
        viewModelScope.launch(Dispatchers.IO) {
            val error = engineLock.withLock {
                val trimmed = name.trim()
                when {
                    trimmed.isEmpty() -> "Name is required"
                    domains.isEmpty() -> "Add at least one domain"
                    RulesToml.parse(configText).any { it.name == trimmed } -> "A rule named \"$trimmed\" already exists"
                    else -> runCatching { repo.save(RulesToml.appendRule(configText, trimmed, domains)) }
                        .exceptionOrNull()?.let { it.message ?: it.toString() }
                }
            }
            if (error == null) {
                reload()
                BlockerVpnService.reload(getApplication())
            }
            launch(Dispatchers.Main) { onDone(error) }
        }
    }

    fun checkHost(input: String) {
        viewModelScope.launch(Dispatchers.IO) {
            engineLock.withLock {
                val e = engine ?: return@withLock
                val host = normalize(input) ?: input.trim().lowercase()
                val now = WeekTime.now()
                val verdict = runCatching { e.checkHost(host, now) }.getOrElse {
                    _rules.update { s -> s.copy(error = it.message) }
                    return@withLock
                }
                _check.value = CheckResult(input, host, verdict, now)
            }
        }
    }

    override fun onCleared() {
        engine?.close()
        engine = null
    }
}
