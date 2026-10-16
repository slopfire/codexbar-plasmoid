package org.slopfire.codexbar.ui.devices

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import java.time.Instant
import org.slopfire.codexbar.data.model.DeviceRecord
import org.slopfire.codexbar.data.net.SyncServerUrls
import org.slopfire.codexbar.ui.AppUiState
import org.slopfire.codexbar.ui.common.EmptyState
import org.slopfire.codexbar.ui.common.ErrorBanner
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.common.InfoChip
import org.slopfire.codexbar.ui.common.KeyValueRow
import org.slopfire.codexbar.ui.common.SectionHeader

/**
 * The Devices tab: the sync-server connection summary, the list of devices that
 * have published a snapshot, and a short note on what the server actually keeps.
 * Forgetting a device deletes its snapshot on the server after a confirmation.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun DevicesScreen(
    state: AppUiState,
    onRefresh: () -> Unit,
    onForgetDevice: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    LazyColumn(
        modifier = modifier.fillMaxSize(),
        contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item(key = "server") {
            ServerCard(state = state, onRefresh = onRefresh)
        }

        if (!state.configured) {
            item(key = "not-configured") {
                EmptyState(
                    title = "No sync server configured",
                    message = "Add the sync server address on the Settings tab. Once this app " +
                        "can reach it, every device that publishes a snapshot will show up here.",
                    icon = Icons.Default.Dns,
                )
            }
        } else {
            item(key = "devices") {
                DevicesCard(
                    state = state,
                    onRefresh = onRefresh,
                    onForgetDevice = onForgetDevice,
                )
            }
        }

        item(key = "storage") { StorageCard() }

        item(key = "bottom") { Spacer(modifier = Modifier.height(96.dp)) }
    }
}

@Composable
private fun ServerCard(state: AppUiState, onRefresh: () -> Unit) {
    val host = SyncServerUrls.displayHost(state.settings.baseUrl).ifBlank { "Not configured" }
    val health = state.health
    val healthError = state.healthError
    val status = serverStatus(state)

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SectionHeader(title = "Sync server", subtitle = host)
            StatusDotRow(color = status.first, text = status.second)
            KeyValueRow(label = "Status", value = health?.status ?: Fmt.PLACEHOLDER)
            KeyValueRow(label = "Version", value = health?.version ?: Fmt.PLACEHOLDER)
            KeyValueRow(label = "Health checked", value = epochRelative(state.healthCheckedAt))
            KeyValueRow(label = "Last snapshot", value = epochRelative(state.lastLoadedAt))
            KeyValueRow(label = "Providers", value = Fmt.count(state.entries.size))
            KeyValueRow(label = "Tracked devices", value = Fmt.count(state.totalTrackedDevices))
            if (healthError != null) {
                ErrorBanner(message = healthError)
            }
            FilledTonalButton(onClick = onRefresh, enabled = !state.refreshing) {
                Icon(
                    imageVector = Icons.Default.Refresh,
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(modifier = Modifier.width(8.dp))
                Text("Refresh")
            }
        }
    }
}

@Composable
private fun DevicesCard(
    state: AppUiState,
    onRefresh: () -> Unit,
    onForgetDevice: (String) -> Unit,
) {
    val devices = state.devices?.devices
    val corrupt = state.devices?.corrupt ?: 0

    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            SectionHeader(title = "Devices", subtitle = "One snapshot per device, merged on read")

            when {
                devices == null -> {
                    Text(
                        text = "No device list yet.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    FilledTonalButton(onClick = onRefresh, enabled = !state.refreshing) {
                        Icon(
                            imageVector = Icons.Default.Refresh,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(modifier = Modifier.width(8.dp))
                        Text("Refresh")
                    }
                }

                devices.isEmpty() -> {
                    Text(
                        text = "No device has published a snapshot to this server yet. The " +
                            "desktop widget publishes one on every successful refresh.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }

                else -> {
                    devices.forEachIndexed { index, record ->
                        DeviceRow(
                            record = record,
                            isThisDevice = state.settings.deviceName.isNotBlank() &&
                                record.device == state.settings.deviceName,
                            forgetting = state.forgetting == record.device,
                            onForget = onForgetDevice,
                        )
                        if (index != devices.lastIndex) {
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        }
                    }
                }
            }

            if (corrupt > 0) {
                WarningBanner(message = "$corrupt unreadable snapshot file(s) on the server")
            }
        }
    }
}

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun DeviceRow(
    record: DeviceRecord,
    isThisDevice: Boolean,
    forgetting: Boolean,
    onForget: (String) -> Unit,
) {
    var confirming by rememberSaveable(record.device) { mutableStateOf(false) }
    val caption = deviceCaption(record)

    Row(
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Column(
            modifier = Modifier.weight(1f),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    text = record.device,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (isThisDevice) {
                    InfoChip(text = "this device")
                }
            }
            InfoChip(text = "${Fmt.count(record.entryCount)} entries")
            if (caption.isNotBlank()) {
                Text(
                    text = caption,
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }

        if (forgetting) {
            LoadingIndicator(modifier = Modifier.size(28.dp))
        } else {
            IconButton(onClick = { confirming = true }) {
                Icon(imageVector = Icons.Default.Delete, contentDescription = "Forget device")
            }
        }
    }

    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            title = { Text("Forget ${record.device}?") },
            text = {
                Text(
                    text = "This deletes the snapshot published by \"${record.device}\" on " +
                        "the server with DELETE /v1/snapshot/<device>. Other devices are " +
                        "unaffected, and this cannot be undone.",
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        confirming = false
                        onForget(record.device)
                    },
                    enabled = !forgetting,
                ) {
                    Text("Forget")
                }
            },
            dismissButton = {
                TextButton(onClick = { confirming = false }) {
                    Text("Cancel")
                }
            },
        )
    }
}

@Composable
private fun StorageCard() {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainer),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SectionHeader(title = "What the server stores")
            KeyValueRow(
                label = "Credentials",
                value = "API keys and tokens never leave the publishing device.",
            )
            KeyValueRow(
                label = "Published entries",
                value = "Only entries after anonymization are uploaded.",
            )
            KeyValueRow(
                label = "Daily history",
                value = "Summed across devices, with per-device provenance.",
            )
            KeyValueRow(
                label = "Snapshots",
                value = "One file per device, merged by the server on read.",
            )
        }
    }
}

/** A colored dot plus label; the devices screen's own health pill. */
@Composable
private fun StatusDotRow(color: Color, text: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Box(
            modifier = Modifier
                .size(8.dp)
                .clip(RoundedCornerShape(50))
                .background(color),
        )
        Text(
            text = text,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun WarningBanner(message: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.tertiaryContainer,
        contentColor = MaterialTheme.colorScheme.onTertiaryContainer,
    ) {
        Row(
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Icon(
                imageVector = Icons.Default.Warning,
                contentDescription = null,
                modifier = Modifier.size(16.dp),
            )
            Text(
                text = message,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.weight(1f),
            )
        }
    }
}

/** Dot color and label for the server card, matching the app bar subtitle. */
private fun serverStatus(state: AppUiState): Pair<Color, String> = when {
    state.health != null -> Health.Good to "Online"
    state.healthError != null -> Health.Tight to "Unreachable"
    else -> Health.Neutral to "Unknown"
}

/** `received … · generated …`, skipping whichever timestamp is missing. */
private fun deviceCaption(record: DeviceRecord): String {
    val parts = buildList {
        Fmt.relativeTime(record.receivedAt)?.let { add("received $it") }
        Fmt.relativeTime(record.generatedAt)?.let { add("generated $it") }
    }
    return parts.joinToString(" · ")
}

/** Relative time from an epoch-millis value, never blank. */
private fun epochRelative(millis: Long?): String {
    if (millis == null) return "never"
    return Fmt.relativeTime(Instant.ofEpochMilli(millis).toString()) ?: "never"
}
