package org.slopfire.codexbar

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.lifecycle.viewmodel.compose.viewModel
import org.slopfire.codexbar.ui.AppViewModel
import org.slopfire.codexbar.ui.CodexBarApp
import org.slopfire.codexbar.ui.theme.CodexBarTheme

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val container = (application as CodexBarApplication).container

        setContent {
            CodexBarTheme {
                val viewModel: AppViewModel = viewModel(
                    factory = AppViewModel.factory(container.repository, container.settingsStore),
                )
                CodexBarApp(viewModel)
            }
        }
    }
}
