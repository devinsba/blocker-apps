package dev.blocker.app.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.InputChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow

/**
 * New-rule form. Domains are normalized by the Rust core as the user types
 * ("https://WWW.Reddit.com/r/x" -> "www.reddit.com"), so what is saved is
 * exactly what the engine matches on.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun AddRuleScreen(
    normalize: (String) -> String?,
    onSave: (name: String, domains: List<String>, onError: (String) -> Unit) -> Unit,
) {
    var name by rememberSaveable { mutableStateOf("") }
    var domainInput by rememberSaveable { mutableStateOf("") }
    val domains = remember { mutableStateListOf<String>() }
    var error by remember { mutableStateOf<String?>(null) }
    var saving by remember { mutableStateOf(false) }

    val normalized = normalize(domainInput)
    val addDomain: () -> Unit = {
        if (normalized != null && normalized !in domains) domains += normalized
        if (normalized != null) domainInput = ""
    }

    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier
                .widthIn(max = 640.dp) // readable form width on tablets
                .fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Add rule", style = MaterialTheme.typography.headlineMedium)
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                label = { Text("Rule name") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = domainInput,
                    onValueChange = { domainInput = it },
                    label = { Text("Domain or URL") },
                    singleLine = true,
                    isError = domainInput.isNotBlank() && normalized == null,
                    supportingText = {
                        when {
                            domainInput.isBlank() -> Text("e.g. https://www.reddit.com/r/all")
                            normalized == null -> Text("Not a valid domain")
                            else -> Text("Will block $normalized and its subdomains")
                        }
                    },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { addDomain() }),
                    modifier = Modifier.weight(1f),
                )
                FilledTonalButton(onClick = addDomain, enabled = normalized != null) { Text("Add") }
            }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                domains.forEach { d ->
                    InputChip(
                        selected = false,
                        onClick = { domains.remove(d) },
                        label = { Text(d) },
                        trailingIcon = { Icon(Icons.Default.Close, contentDescription = "Remove $d") },
                    )
                }
            }
            error?.let { MessageCard(title = "Could not save", text = it, error = true) }
            Button(
                onClick = {
                    saving = true
                    error = null
                    onSave(name, domains.toList()) { message ->
                        saving = false
                        error = message
                    }
                },
                enabled = !saving && name.isNotBlank() && domains.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text(if (saving) "Saving…" else "Save rule") }
        }
    }
}
