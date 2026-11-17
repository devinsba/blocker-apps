package dev.blocker.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.GridItemSpan
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Box
import dev.blocker.app.RulesState
import dev.blocker.app.data.RuleSummary

@Composable
fun RulesScreen(
    state: RulesState,
    vpnRunning: Boolean,
    onVpnChange: (Boolean) -> Unit,
    onReload: () -> Unit,
    onAddRule: () -> Unit,
) {
    Box(Modifier.fillMaxSize()) {
        // Adaptive columns: one on phones, two or three on tablets.
        LazyVerticalGrid(
            columns = GridCells.Adaptive(minSize = 320.dp),
            modifier = Modifier.fillMaxSize(),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item(span = { GridItemSpan(maxLineSpan) }) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Rules", style = MaterialTheme.typography.headlineMedium)
                        val subtitle = buildString {
                            append("${state.activeRules.size} active")
                            state.evaluatedAt?.let { append(" at $it") }
                            if (state.coreVersion.isNotEmpty()) append(" · core ${state.coreVersion}")
                        }
                        Text(subtitle, style = MaterialTheme.typography.bodySmall)
                    }
                    IconButton(onClick = onReload) { Icon(Icons.Default.Refresh, contentDescription = "Reload") }
                }
            }
            item(span = { GridItemSpan(maxLineSpan) }) {
                VpnCard(running = vpnRunning, onChange = onVpnChange)
            }
            state.error?.let { error ->
                item(span = { GridItemSpan(maxLineSpan) }) {
                    MessageCard(title = "Error", text = error, error = true)
                }
            }
            if (state.warnings.isNotEmpty()) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    MessageCard(title = "Warnings", text = state.warnings.joinToString("\n"), error = false)
                }
            }
            if (state.loading) {
                item(span = { GridItemSpan(maxLineSpan) }) {
                    Box(Modifier.fillMaxWidth().padding(32.dp), contentAlignment = Alignment.Center) {
                        CircularProgressIndicator()
                    }
                }
            }
            items(state.rules, key = { it.name }) { rule ->
                RuleCard(rule, active = rule.name in state.activeRules)
            }
            // Room for the FAB.
            item(span = { GridItemSpan(maxLineSpan) }) { Spacer(Modifier.size(72.dp)) }
        }
        ExtendedFloatingActionButton(
            onClick = onAddRule,
            icon = { Icon(Icons.Default.Add, contentDescription = null) },
            text = { Text("Add rule") },
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
        )
    }
}

@Composable
private fun VpnCard(running: Boolean, onChange: (Boolean) -> Unit) {
    Card(Modifier.fillMaxWidth()) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text("DNS filter", style = MaterialTheme.typography.titleMedium)
                Text(
                    if (running) "Running as a local VPN: blocked domains do not resolve."
                    else "Off. Uses Android's VPN slot (only one VPN can run at a time).",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
            Spacer(Modifier.width(12.dp))
            Switch(checked = running, onCheckedChange = onChange)
        }
    }
}

@Composable
private fun RuleCard(rule: RuleSummary, active: Boolean) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    rule.name,
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                if (rule.locked) Icon(Icons.Default.Lock, contentDescription = "Locked", Modifier.size(18.dp))
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                val status = when {
                    !rule.enabled -> "Disabled"
                    active -> "Active now"
                    rule.scheduled -> "Scheduled"
                    else -> "Inactive"
                }
                AssistChip(onClick = {}, label = { Text(status) })
                if (rule.scheduled) AssistChip(onClick = {}, label = { Text("Schedule") })
            }
            if (rule.domains.isNotEmpty()) {
                Text(rule.domains.joinToString(", "), style = MaterialTheme.typography.bodyMedium)
            }
            if (rule.categories.isNotEmpty()) {
                Text("Categories: ${rule.categories.joinToString(", ")}", style = MaterialTheme.typography.bodySmall)
            }
            if (rule.bundleIds.isNotEmpty()) {
                Text("Apps: ${rule.bundleIds.joinToString(", ")}", style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}

@Composable
internal fun MessageCard(title: String, text: String, error: Boolean) {
    val colors = if (error) {
        CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        )
    } else {
        CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer)
    }
    Card(Modifier.fillMaxWidth(), colors = colors) {
        Row(Modifier.padding(16.dp)) {
            Icon(Icons.Default.Warning, contentDescription = null)
            Spacer(Modifier.width(12.dp))
            Column {
                Text(title, style = MaterialTheme.typography.titleSmall)
                Text(text, style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}
