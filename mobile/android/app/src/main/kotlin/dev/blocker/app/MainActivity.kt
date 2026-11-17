package dev.blocker.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import dev.blocker.app.ui.BlockerApp
import dev.blocker.app.ui.BlockerTheme

class MainActivity : ComponentActivity() {
    private val viewModel: BlockerViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            BlockerTheme {
                BlockerApp(viewModel)
            }
        }
    }
}
