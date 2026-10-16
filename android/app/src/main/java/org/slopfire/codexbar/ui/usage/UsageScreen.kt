package org.slopfire.codexbar.ui.usage

import androidx.compose.foundation.layout.Arrangement
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
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CloudOff
import androidx.compose.material.icons.filled.Dns
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.slopfire.codexbar.ui.AppUiState
import org.slopfire.codexbar.ui.SortMode
import org.slopfire.codexbar.ui.common.EmptyState
import org.slopfire.codexbar.ui.common.ErrorBanner
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.common.MetricTile
import org.slopfire.codexbar.ui.theme.providerAccent
import org.slopfire.codexbar.ui.theme.providerLabel

/**
 * The Usage tab: first-run guidance, an error view, or the merged provider list
 * with its filter/sort controls and a summary strip.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun UsageScreen(
    state: AppUiState,
    onRefresh: () -> Unit,
    onSelectProvider: (String?) -> Unit,
    onSortChange: (SortMode) -> Unit,
    onOpenEntry: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val error = state.error
    when {
        !state.configured -> EmptyState(
            title = "No sync server configured",
            message = "Open the Settings tab and enter your sync server address, for example " +
                "http://nas.local:8188, plus the bearer token. This app and the desktop widget " +
                "read the same server.",
            icon = Icons.Default.Dns,
            modifier = modifier,
        )

        error != null && state.snapshot == null -> EmptyState(
            title = "Could not load usage",
            message = error,
            icon = Icons.Default.CloudOff,
            modifier = modifier,
            action = {
                Button(onClick = onRefresh) {
                    Icon(
                        imageVector = Icons.Default.Refresh,
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                    )
                    Spacer(Modifier.width(8.dp))
                    Text("Try again")
                }
            },
        )

        else -> UsageContent(
            state = state,
            onRefresh = onRefresh,
            onSelectProvider = onSelectProvider,
            onSortChange = onSortChange,
            onOpenEntry = onOpenEntry,
            modifier = modifier,
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
private fun UsageContent(
    state: AppUiState,
    onRefresh: () -> Unit,
    onSelectProvider: (String?) -> Unit,
    onSortChange: (SortMode) -> Unit,
    onOpenEntry: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val error = state.error
    val entries = state.entries

    Column(modifier = modifier.fillMaxSize()) {
        if (error != null) {
            ErrorBanner(
                message = error,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
        }

        if (entries.isEmpty()) {
            EmptyState(
                title = "No snapshots yet",
                message = "No device has published a snapshot to this server yet. The desktop widget " +
                    "publishes one on every successful refresh, and so does this app once it can " +
                    "reach the server.",
                icon = Icons.Default.Info,
                modifier = Modifier.weight(1f),
                action = {
                    Button(onClick = onRefresh) {
                        Icon(
                            imageVector = Icons.Default.Refresh,
                            contentDescription = null,
                            modifier = Modifier.size(18.dp),
                        )
                        Spacer(Modifier.width(8.dp))
                        Text("Refresh")
                    }
                },
            )
            return@Column
        }

        // Filter, sort and summary live inside the list so they scroll away
        // under the app bar instead of pinning a second control band on screen.
        PullToRefreshBox(
            isRefreshing = state.refreshing,
            onRefresh = onRefresh,
            modifier = Modifier.fillMaxSize(),
        ) {
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = 8.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                if (state.providerIds.size > 1) {
                    item(key = "provider-filter") {
                        ProviderFilterRow(
                            providerIds = state.providerIds,
                            selected = state.selectedProvider,
                            onSelectProvider = onSelectProvider,
                        )
                    }
                }

                item(key = "sort") {
                    SortRow(sortMode = state.sortMode, onSortChange = onSortChange)
                }

                item(key = "summary") { SummaryStrip(state = state) }

                items(items = state.visibleEntries, key = { entry -> entry.key }) { entry ->
                    ProviderCard(
                        entry = entry,
                        onClick = { onOpenEntry(entry.key) },
                        modifier = Modifier.animateItem(),
                    )
                }

                item(key = "bottom-spacer") { Spacer(Modifier.height(96.dp)) }
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProviderFilterRow(
    providerIds: List<String>,
    selected: String?,
    onSelectProvider: (String?) -> Unit,
) {
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        item(key = "all") {
            FilterChip(
                selected = selected == null,
                onClick = { onSelectProvider(null) },
                label = { Text("All") },
            )
        }
        items(items = providerIds, key = { id -> id }) { id ->
            val accent = providerAccent(id)
            FilterChip(
                selected = selected == id,
                onClick = { onSelectProvider(id) },
                label = {
                    Text(
                        text = providerLabel(id),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                colors = FilterChipDefaults.filterChipColors(
                    selectedContainerColor = accent.copy(alpha = 0.20f),
                    selectedLabelColor = accent,
                ),
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SortRow(sortMode: SortMode, onSortChange: (SortMode) -> Unit) {
    LazyRow(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(items = SortMode.entries, key = { mode -> mode.name }) { mode ->
            FilterChip(
                selected = sortMode == mode,
                onClick = { onSortChange(mode) },
                label = {
                    Text(
                        text = mode.label,
                        style = MaterialTheme.typography.labelSmall,
                        maxLines = 1,
                    )
                },
            )
        }
    }
}

@Composable
private fun SummaryStrip(state: AppUiState) {
    val snapshot = state.snapshot
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerHigh,
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.spacedBy(20.dp),
            ) {
                MetricTile(
                    label = "Providers",
                    value = Fmt.count(state.entries.size),
                    animateValue = true,
                    modifier = Modifier.weight(1f),
                )
                MetricTile(
                    label = "Tightest left",
                    value = Fmt.percent(state.tightestPercentLeft),
                    accent = Health.remainingColor(state.tightestPercentLeft),
                    animateValue = true,
                    modifier = Modifier.weight(1f),
                )
                MetricTile(
                    label = "Updated",
                    value = Fmt.relativeTime(snapshot?.generatedAt) ?: "unknown",
                    animateValue = true,
                    modifier = Modifier.weight(1f),
                )
            }

            val costError = snapshot?.costError
            if (!costError.isNullOrBlank()) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        imageVector = Icons.Default.Warning,
                        contentDescription = null,
                        modifier = Modifier.size(14.dp),
                        tint = Health.Tight,
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = costError,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}
