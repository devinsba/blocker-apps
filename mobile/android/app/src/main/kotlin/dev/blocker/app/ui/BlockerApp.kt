package dev.blocker.app.ui

import android.app.Activity
import android.net.VpnService
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import dev.blocker.app.BlockerViewModel
import dev.blocker.app.vpn.BlockerVpnService
import kotlinx.coroutines.launch

enum class Destination(val label: String, val icon: ImageVector) {
    Rules("Rules", Icons.Default.Lock),
    AddRule("Add rule", Icons.Default.Add),
    Check("Check host", Icons.Default.Search),
}

/**
 * Root composable. Phones get a bottom navigation bar; anything at least
 * 600dp wide (tablets, foldables, landscape, desktop windows) gets a
 * navigation rail, and the screens themselves reflow into columns.
 */
@Composable
fun BlockerApp(viewModel: BlockerViewModel) {
    var destination by rememberSaveable { mutableStateOf(Destination.Rules) }
    val rules by viewModel.rules.collectAsStateWithLifecycle()
    val check by viewModel.check.collectAsStateWithLifecycle()
    val vpnRunning by viewModel.vpnRunning.collectAsStateWithLifecycle()
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current

    val vpnPermission = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) {
        if (it.resultCode == Activity.RESULT_OK) {
            BlockerVpnService.start(context)
        } else {
            scope.launch { snackbar.showSnackbar("VPN permission was not granted") }
        }
    }
    val setVpn: (Boolean) -> Unit = { enable ->
        if (!enable) {
            BlockerVpnService.stop(context)
        } else {
            // null = already prepared (the user approved this app before).
            val consent = VpnService.prepare(context)
            if (consent == null) BlockerVpnService.start(context) else vpnPermission.launch(consent)
        }
    }

    val content: @Composable (Modifier) -> Unit = { modifier ->
        Box(modifier) {
            when (destination) {
                Destination.Rules -> RulesScreen(
                    state = rules,
                    vpnRunning = vpnRunning,
                    onVpnChange = setVpn,
                    onReload = viewModel::reload,
                    onAddRule = { destination = Destination.AddRule },
                )
                Destination.AddRule -> AddRuleScreen(
                    normalize = viewModel::normalize,
                    onSave = { name, domains, onError ->
                        viewModel.addRule(name, domains) { error ->
                            if (error == null) {
                                destination = Destination.Rules
                                scope.launch { snackbar.showSnackbar("Saved rule \"$name\"") }
                            } else {
                                onError(error)
                            }
                        }
                    },
                )
                Destination.Check -> CheckHostScreen(result = check, onCheck = viewModel::checkHost)
            }
        }
    }

    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= 600.dp
        Scaffold(
            snackbarHost = { SnackbarHost(snackbar) },
            bottomBar = {
                if (!wide) {
                    NavigationBar {
                        Destination.entries.forEach { d ->
                            NavigationBarItem(
                                selected = destination == d,
                                onClick = { destination = d },
                                icon = { Icon(d.icon, contentDescription = null) },
                                label = { Text(d.label) },
                            )
                        }
                    }
                }
            },
        ) { padding ->
            if (wide) {
                Row(Modifier.fillMaxSize().padding(padding)) {
                    NavigationRail {
                        Destination.entries.forEach { d ->
                            NavigationRailItem(
                                selected = destination == d,
                                onClick = { destination = d },
                                icon = { Icon(d.icon, contentDescription = null) },
                                label = { Text(d.label) },
                            )
                        }
                    }
                    content(Modifier.weight(1f).fillMaxSize())
                }
            } else {
                content(Modifier.fillMaxSize().padding(padding))
            }
        }
    }
}
