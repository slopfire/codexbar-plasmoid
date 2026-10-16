package org.slopfire.codexbar.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.ButtonGroup
import androidx.compose.material3.ButtonGroupDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.net.SyncServerUrls
import org.slopfire.codexbar.data.settings.SyncSettings
import org.slopfire.codexbar.ui.AppUiState
import org.slopfire.codexbar.ui.common.ErrorBanner
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.common.InfoChip
import org.slopfire.codexbar.ui.common.KeyValueRow
import org.slopfire.codexbar.ui.common.SectionHeader
import org.slopfire.codexbar.ui.theme.ExpressiveType
import org.slopfire.codexbar.widget.WidgetPinner

/**
 * The Settings tab: the sync-server connection fields, the auto-refresh cadence,
 * the current connection status, a plain-language note on where the data comes
 * from, and the security posture of the plain-HTTP sync server.
 *
 * The editable fields are local so typing never fights the saved state; they are
 * re-seeded from [AppUiState.settings] only when that saved value changes.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun SettingsScreen(
    state: AppUiState,
    onSave: (SyncSettings) -> Unit,
    onTestConnection: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var url by rememberSaveable(state.settings.baseUrl) {
        mutableStateOf(state.settings.baseUrl)
    }
    var token by rememberSaveable(state.settings.token) {
        mutableStateOf(state.settings.token)
    }
    var deviceName by rememberSaveable(state.settings.deviceName) {
        mutableStateOf(state.settings.deviceName)
    }
    var refreshSeconds by rememberSaveable(state.settings.autoRefreshSeconds) {
        mutableStateOf(state.settings.autoRefreshSeconds)
    }

    val localSettings = state.settings.copy(
        baseUrl = url,
        token = token,
        deviceName = deviceName,
        autoRefreshSeconds = refreshSeconds,
    )

    LazyColumn(
        modifier = modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item(key = "connection") {
            ConnectionCard(
                url = url,
                onUrlChange = { url = it },
                token = token,
                onTokenChange = { token = it },
                deviceName = deviceName,
                onDeviceNameChange = { deviceName = it },
                refreshing = state.refreshing,
                onSave = { onSave(localSettings) },
                onTestConnection = onTestConnection,
            )
        }

        item(key = "auto-refresh") {
            AutoRefreshCard(
                selected = refreshSeconds,
                onSelect = { refreshSeconds = it },
            )
        }

        item(key = "status") {
            ConnectionStatusCard(health = state.health, healthError = state.healthError)
        }

        item(key = "how") { HowItWorksCard() }

        item(key = "widgets") { WidgetsCard() }

        item(key = "security") {
            SecurityCard(
                onReset = {
                    url = ""
                    token = ""
                    deviceName = ""
                    refreshSeconds = 300
                },
            )
        }

        item(key = "bottom") { Spacer(modifier = Modifier.height(96.dp)) }
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun ConnectionCard(
    url: String,
    onUrlChange: (String) -> Unit,
    token: String,
    onTokenChange: (String) -> Unit,
    deviceName: String,
    onDeviceNameChange: (String) -> Unit,
    refreshing: Boolean,
    onSave: () -> Unit,
    onTestConnection: () -> Unit,
) {
    var tokenVisible by rememberSaveable { mutableStateOf(false) }
    val urlError = url.isNotBlank() && SyncServerUrls.normalize(url) == null

    SettingsCard {
        SectionHeader(title = "Sync server")
        OutlinedTextField(
            value = url,
            onValueChange = onUrlChange,
            label = { Text("Server address") },
            placeholder = { Text("http://nas.local:8188") },
            singleLine = true,
            isError = urlError,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            supportingText = {
                Text(
                    text = if (urlError) {
                        "That does not look like a usable address. A scheme is optional; " +
                            "http:// is assumed."
                    } else {
                        "A scheme is optional; http:// is assumed."
                    },
                )
            },
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = token,
            onValueChange = onTokenChange,
            label = { Text("Bearer token") },
            singleLine = true,
            visualTransformation = if (tokenVisible) {
                VisualTransformation.None
            } else {
                PasswordVisualTransformation()
            },
            trailingIcon = {
                IconButton(onClick = { tokenVisible = !tokenVisible }) {
                    Icon(
                        imageVector = if (tokenVisible) {
                            Icons.Default.VisibilityOff
                        } else {
                            Icons.Default.Visibility
                        },
                        contentDescription = if (tokenVisible) "Hide token" else "Show token",
                    )
                }
            },
            supportingText = {
                Text(
                    text = "Required for every /v1 route. Stored in this app's private " +
                        "preferences.",
                )
            },
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = deviceName,
            onValueChange = onDeviceNameChange,
            label = { Text("Device name") },
            singleLine = true,
            supportingText = {
                Text("The name this device publishes under; defaults to the Android model.")
            },
            modifier = Modifier.fillMaxWidth(),
        )
        // Connected button group: pressing one item enlarges it, the pair reads
        // as one control, and the primary action stays first.
        ButtonGroup(
            overflowIndicator = { menuState -> ButtonGroupDefaults.OverflowIndicator(menuState) },
        ) {
            clickableItem(onClick = onSave, label = "Save", enabled = !refreshing)
            clickableItem(
                onClick = onTestConnection,
                label = "Test connection",
                enabled = !refreshing,
            )
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AutoRefreshCard(selected: Int, onSelect: (Int) -> Unit) {
    SettingsCard {
        SectionHeader(title = "Auto-refresh", subtitle = "How often to poll the merged view")
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            SyncSettings.AUTO_REFRESH_CHOICES.forEach { seconds ->
                FilterChip(
                    selected = selected == seconds,
                    onClick = { onSelect(seconds) },
                    label = { Text(SyncSettings.label(seconds)) },
                )
            }
        }
        Text(
            text = "The desktop widget publishes on its own refresh, so polling more " +
                "often than every few minutes rarely helps.",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun ConnectionStatusCard(health: HealthInfo?, healthError: String?) {
    SettingsCard {
        SectionHeader(title = "Connection status")
        if (health != null) {
            InfoChip(
                text = "Connected",
                containerColor = Health.Good.copy(alpha = 0.18f),
                contentColor = Health.Good,
            )
            KeyValueRow(label = "Status", value = health.status)
            KeyValueRow(label = "Version", value = health.version ?: Fmt.PLACEHOLDER)
        }
        if (healthError != null) {
            ErrorBanner(message = healthError)
        }
        if (health == null && healthError == null) {
            Text(
                text = "Not checked yet.",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun HowItWorksCard() {
    SettingsCard {
        SectionHeader(title = "How it works")
        KeyValueRow(
            label = "Merged view",
            value = "GET /v1/usage returns limits, status, credits and summed daily history.",
        )
        KeyValueRow(
            label = "Devices",
            value = "GET /v1/devices lists every device that has published a snapshot.",
        )
        KeyValueRow(
            label = "Credentials",
            value = "API keys and tokens stay on the desktop devices; the phone never sends them.",
        )
        KeyValueRow(
            label = "Presentation",
            value = "Provider icons and colors are bundled locally in the app.",
        )
    }
}

@Composable
private fun SecurityCard(onReset: () -> Unit) {
    SettingsCard {
        SectionHeader(title = "Security")
        KeyValueRow(
            label = "Transport",
            value = "The sync server is plain HTTP; keep it on a LAN or a VPN such as " +
                "Tailscale or WireGuard.",
        )
        KeyValueRow(
            label = "Token",
            value = "The bearer token travels on every /v1 request.",
        )
        KeyValueRow(
            label = "Backups",
            value = "The token is excluded from Android backup and device transfer.",
        )
        TextButton(onClick = onReset) {
            Text("Reset to defaults")
        }
    }
}

/**
 * Offers the home screen widgets.
 *
 * Pinning goes through the launcher's own confirmation dialog, so the app never
 * adds a widget silently. Launchers that do not support pinning fall back to the
 * widget picker, which the caption explains.
 */
@Composable
private fun WidgetsCard() {
    val context = LocalContext.current
    val canPin = remember(context) { WidgetPinner.canPin(context) }
    var requested by remember { mutableStateOf<String?>(null) }

    SettingsCard {
        SectionHeader(
            title = "Home screen widgets",
            subtitle = "Same Material You palette and the same merged data as the app",
        )
        Text(
            text = "Each widget is configured on its own: providers, order, colours and " +
                "detail lines. Long-press a placed widget and choose Edit to change it.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        WidgetPinner.all.forEach { target ->
            val placed = WidgetPinner.placedCount(context, target)
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(text = target.label, style = ExpressiveType.titleSmall)
                    Text(
                        text = if (placed > 0) {
                            "${target.description} · $placed on the home screen"
                        } else {
                            target.description
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (placed > 0) {
                    TextButton(
                        onClick = {
                            WidgetPinner.firstPlacedId(context, target)?.let { id ->
                                WidgetPinner.configure(context, id)
                            }
                        },
                    ) {
                        Text("Configure")
                    }
                }
                FilledTonalButton(
                    onClick = {
                        if (WidgetPinner.pin(context, target)) {
                            requested = target.label
                        }
                    },
                    enabled = canPin,
                ) {
                    Text("Add")
                }
            }
        }
        if (!canPin) {
            Text(
                text = "This launcher does not support adding widgets from an app. " +
                    "Long-press the home screen and pick CodexBar from the widget list instead.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        requested?.let { label ->
            Text(
                text = "$label requested — confirm the prompt to place it.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

/** One settings card: the published CornerMedium shape on the container surface. */
@Composable
private fun SettingsCard(content: @Composable ColumnScope.() -> Unit) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
            content = content,
        )
    }
}
