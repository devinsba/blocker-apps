package dev.blocker.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.blocker.app.CheckResult
import dev.blocker.core.Verdict

/** Asks the engine whether a host would be blocked right now. */
@Composable
fun CheckHostScreen(result: CheckResult?, onCheck: (String) -> Unit) {
    var input by rememberSaveable { mutableStateOf("") }
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 640.dp).fillMaxWidth().padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Check a host", style = MaterialTheme.typography.headlineMedium)
            OutlinedTextField(
                value = input,
                onValueChange = { input = it },
                label = { Text("Host or URL") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { if (input.isNotBlank()) onCheck(input) }),
                modifier = Modifier.fillMaxWidth(),
            )
            Button(onClick = { onCheck(input) }, enabled = input.isNotBlank()) {
                Icon(Icons.Default.Search, contentDescription = null)
                Spacer(Modifier.width(8.dp))
                Text("Check")
            }
            result?.let { VerdictCard(it) }
        }
    }
}

@Composable
private fun VerdictCard(result: CheckResult) {
    val blocked = result.verdict is Verdict.Block
    val colors = if (blocked) {
        CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        )
    } else {
        CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.primaryContainer)
    }
    Card(Modifier.fillMaxWidth(), colors = colors) {
        Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(if (blocked) Icons.Default.Lock else Icons.Default.CheckCircle, contentDescription = null)
            Spacer(Modifier.width(12.dp))
            Column {
                val title = when (val v = result.verdict) {
                    is Verdict.Block -> "Blocked by \"${v.rule}\""
                    Verdict.Allow -> "Allowed"
                    Verdict.Peek -> "Needs a peek at the connection"
                }
                Text(title, style = MaterialTheme.typography.titleMedium)
                Text(result.host, style = MaterialTheme.typography.bodyMedium)
                (result.verdict as? Verdict.Block)?.let {
                    Text("Matched ${it.matched}", style = MaterialTheme.typography.bodySmall)
                }
                Text("Evaluated at ${result.at}", style = MaterialTheme.typography.bodySmall)
            }
        }
    }
}
