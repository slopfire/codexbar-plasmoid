package org.slopfire.codexbar.ui.detail

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Sync
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MediumFlexibleTopAppBar
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.slopfire.codexbar.data.model.DailyUsagePoint
import org.slopfire.codexbar.data.model.LimitReset
import org.slopfire.codexbar.data.model.LimitResetCredits
import org.slopfire.codexbar.data.model.ModelUsage
import org.slopfire.codexbar.data.model.ProviderEntry
import org.slopfire.codexbar.data.model.RemoteDevice
import org.slopfire.codexbar.data.model.ResetCredit
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.common.InfoChip
import org.slopfire.codexbar.ui.common.KeyValueRow
import org.slopfire.codexbar.ui.common.MetricTile
import org.slopfire.codexbar.ui.common.ProviderAvatar
import org.slopfire.codexbar.ui.common.SectionHeader
import org.slopfire.codexbar.ui.common.StatusPill
import org.slopfire.codexbar.ui.theme.providerAccent
import org.slopfire.codexbar.ui.theme.providerLabel
import org.slopfire.codexbar.ui.theme.providerContainerColor
import org.slopfire.codexbar.ui.usage.UsageLimitBar

/**
 * Full-surface detail for one merged [ProviderEntry]: identity and status, the
 * reported limit windows, reset credits, spend, trailing history, a merged model
 * breakdown, contributing devices, and the raw metadata. The screen owns its own
 * [Scaffold] so a long history chart gets the whole window.
 */
@OptIn(
    ExperimentalMaterial3Api::class,
    ExperimentalMaterial3ExpressiveApi::class,
    ExperimentalLayoutApi::class,
)
@Composable
fun ProviderDetailScreen(
    entry: ProviderEntry,
    onBack: () -> Unit,
    onRefresh: () -> Unit,
    refreshing: Boolean,
    modifier: Modifier = Modifier,
) {
    val accent = providerAccent(entry.provider)
    var selectedDay by remember(entry.key) { mutableStateOf<DailyUsagePoint?>(null) }

    val credits = entry.limitResetCredits
    val hasCredits = credits != null && (credits.items.isNotEmpty() || credits.availableCount > 0)
    val hasSpend = entry.tokenUsage != null || entry.dailyUsage.isNotEmpty()
    val hasHistory = entry.dailyUsage.isNotEmpty()
    val hasNotes = entry.stale ||
        entry.error?.isNotBlank() == true ||
        entry.usageUnavailable?.isNotBlank() == true
    val mergedModels = remember(entry.dailyUsage) { mergeModels(entry.dailyUsage) }
    val currency = entry.tokenUsage?.currencyCode

    // Same fold as the list screen: the headline compresses into a small bar
    // while the history chart scrolls under it.
    val scrollBehavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()

    Scaffold(
        modifier = modifier.nestedScroll(scrollBehavior.nestedScrollConnection),
        containerColor = MaterialTheme.colorScheme.surface,
        topBar = {
            MediumFlexibleTopAppBar(
                title = {
                    Text(
                        text = providerLabel(entry.provider),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                subtitle = {
                    Text(
                        text = appBarSubtitle(entry),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                        )
                    }
                },
                actions = {
                    FilledTonalIconButton(
                        onClick = onRefresh,
                        enabled = !refreshing,
                        modifier = Modifier.padding(end = 4.dp),
                    ) {
                        if (refreshing) {
                            LoadingIndicator(
                                modifier = Modifier.size(22.dp),
                                color = LocalContentColor.current,
                            )
                        } else {
                            Icon(Icons.Default.Refresh, contentDescription = "Refresh")
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                    scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
                scrollBehavior = scrollBehavior,
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(padding),
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { HeaderCard(entry = entry) }

            if (hasNotes) {
                item { StateNotes(entry = entry) }
            }

            item { SectionHeader(title = "Limits") }
            if (entry.rows.isEmpty()) {
                item {
                    Text(
                        text = "No limit windows were reported.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            } else {
                items(entry.rows) { row ->
                    UsageLimitBar(row = row, modifier = Modifier.fillMaxWidth())
                }
            }

            if (hasCredits) {
                item {
                    SectionHeader(
                        title = "Reset credits",
                        subtitle = creditsSubtitle(credits),
                    )
                }
                item {
                    MetricTile(
                        label = "Available",
                        value = Fmt.count(credits.availableCount),
                        caption = credits.nextExpiresAt?.let { "Next expires ${relative(it)}" },
                        accent = if (credits.availableCount > 0) Health.Good else null,
                    )
                }
                items(credits.items) { credit -> ResetCreditRow(credit = credit) }
            }

            if (hasSpend) {
                item { SectionHeader(title = "Spend") }
                item { SpendSection(entry = entry) }
            }

            if (hasHistory) {
                item { SectionHeader(title = "History", subtitle = "Tap a day for details") }
                item {
                    HistoryChart(
                        points = entry.dailyUsage,
                        accent = accent,
                        onSelect = { selectedDay = it },
                    )
                }
                selectedDay?.let { day ->
                    item(key = "selected-day-${day.dayKey}") {
                        DayDetailCard(day = day, currency = currency)
                    }
                }
            }

            if (mergedModels.isNotEmpty()) {
                item { SectionHeader(title = "Models") }
                val totalCost = mergedModels.sumOf { it.costUSD }
                items(mergedModels) { model ->
                    ModelShareRow(
                        model = model,
                        totalCost = totalCost,
                        accent = accent,
                        currency = currency,
                    )
                }
            }

            if (entry.remoteDevices.isNotEmpty()) {
                item {
                    SectionHeader(
                        title = "Contributing devices",
                        subtitle = "Merged by the sync server",
                    )
                }
                items(entry.remoteDevices) { device -> DeviceRow(device = device) }
            }

            item { MetadataCard(entry = entry) }

            item { Spacer(modifier = Modifier.height(96.dp)) }
        }
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

@Composable
private fun HeaderCard(entry: ProviderEntry) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = providerContainerColor(entry.provider),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                ProviderAvatar(provider = entry.provider, size = 56.dp)
                Column(
                    modifier = Modifier.weight(1f),
                    verticalArrangement = Arrangement.spacedBy(4.dp),
                ) {
                    Text(
                        text = providerLabel(entry.provider),
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                    )
                    StatusPill(status = entry.status)
                }
            }
            HeaderChips(entry = entry)
            entry.siteUrl?.takeIf { it.isNotBlank() }?.let { url -> SiteUrlRow(url = url) }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun HeaderChips(entry: ProviderEntry) {
    val chips = buildList {
        entry.source?.takeIf { it.isNotBlank() }?.let { add(it) }
        entry.plan?.takeIf { it.isNotBlank() }?.let { add(it) }
        entry.version?.takeIf { it.isNotBlank() }?.let { add("v$it") }
        entry.historySource?.takeIf { it.isNotBlank() }?.let { add(it) }
        entry.organization?.takeIf { it.isNotBlank() }?.let { add(it) }
    }
    if (chips.isEmpty()) return
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        chips.forEach { chip -> InfoChip(text = chip) }
    }
}

@Composable
private fun SiteUrlRow(url: String) {
    val uriHandler = LocalUriHandler.current
    SelectionContainer {
        Text(
            text = url,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.primary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .clip(MaterialTheme.shapes.small)
                .clickable { runCatching { uriHandler.openUri(url) } }
                .padding(vertical = 2.dp),
        )
    }
}

// ---------------------------------------------------------------------------
// State notes
// ---------------------------------------------------------------------------

@Composable
private fun StateNotes(entry: ProviderEntry) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (entry.stale) {
            NoteRow(
                icon = Icons.Default.Sync,
                text = "Showing last good data, retrying…",
                tint = Health.Tight,
            )
            entry.lastAttemptError?.takeIf { it.isNotBlank() }?.let { message ->
                NoteRow(
                    icon = Icons.Default.Info,
                    text = message,
                    tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    small = true,
                )
            }
        }
        entry.error?.takeIf { it.isNotBlank() }?.let { message ->
            NoteRow(
                icon = Icons.Default.Error,
                text = message,
                tint = MaterialTheme.colorScheme.error,
            )
        }
        entry.usageUnavailable?.takeIf { it.isNotBlank() }?.let { message ->
            NoteRow(
                icon = Icons.Default.Error,
                text = message,
                tint = MaterialTheme.colorScheme.error,
            )
        }
    }
}

@Composable
private fun NoteRow(
    icon: ImageVector,
    text: String,
    tint: Color,
    small: Boolean = false,
) {
    Row(
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Icon(
            imageVector = icon,
            contentDescription = null,
            tint = tint,
            modifier = Modifier.size(18.dp),
        )
        Text(
            text = text,
            style = if (small) {
                MaterialTheme.typography.labelSmall
            } else {
                MaterialTheme.typography.bodySmall
            },
            color = tint,
        )
    }
}

// ---------------------------------------------------------------------------
// Reset credits
// ---------------------------------------------------------------------------

@Composable
private fun ResetCreditRow(credit: ResetCredit) {
    val available = credit.status.equals("available", ignoreCase = true)
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        Column(
            modifier = Modifier.padding(14.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    text = credit.title,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.weight(1f),
                )
                InfoChip(
                    text = credit.status,
                    containerColor = if (available) Health.Good.copy(alpha = 0.18f) else null,
                    contentColor = if (available) Health.Good else null,
                )
            }
            credit.description?.takeIf { it.isNotBlank() }?.let { description ->
                Text(
                    text = description,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            credit.resetType?.takeIf { it.isNotBlank() }?.let { resetType ->
                Text(
                    text = "Type: $resetType",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            credit.expiresAt?.let { iso ->
                Text(
                    text = "Expires ${relative(iso)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Spend
// ---------------------------------------------------------------------------

@Composable
private fun SpendSection(entry: ProviderEntry) {
    val usage = entry.tokenUsage
    val currency = usage?.currencyCode
    val showSession = usage?.sessionCostUSD != null || usage?.sessionTokens != null
    val showLast30 = usage?.last30DaysCostUSD != null || usage?.last30DaysTokens != null
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            if (showSession) {
                MetricTile(
                    label = usage?.sessionLabel?.takeIf { it.isNotBlank() } ?: "Today",
                    value = Fmt.money(usage?.sessionCostUSD, currency),
                    caption = usage?.sessionTokens?.let { Fmt.tokens(it) + " tokens" },
                    modifier = Modifier.weight(1f),
                )
            }
            if (showLast30) {
                MetricTile(
                    label = usage?.last30DaysLabel?.takeIf { it.isNotBlank() } ?: "30d",
                    value = Fmt.money(usage?.last30DaysCostUSD, currency),
                    caption = usage?.last30DaysTokens?.let { Fmt.tokens(it) + " tokens" },
                    modifier = Modifier.weight(1f),
                )
            }
            if (entry.dailyUsage.isNotEmpty()) {
                MetricTile(
                    label = "Trailing",
                    value = Fmt.tokens(entry.dailyUsage.sumOf { it.totalTokens }),
                    caption = "${entry.dailyUsage.size} days",
                    modifier = Modifier.weight(1f),
                )
            }
        }
        usage?.provenance?.takeIf { it.isNotBlank() }?.let { provenance ->
            Text(
                text = provenanceLabel(provenance),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (usage?.historyCoverageIsEstablished == false) {
            NoteRow(
                icon = Icons.Default.Info,
                text = "History coverage is still being established.",
                tint = Health.Tight,
                small = true,
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Selected-day detail
// ---------------------------------------------------------------------------

@Composable
private fun DayDetailCard(day: DailyUsagePoint, currency: String?) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(
                text = Fmt.date(day.dayKey) ?: Fmt.dayKey(day.dayKey),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                MetricTile(
                    label = "Cost",
                    value = Fmt.money(day.costUSD, currency),
                    modifier = Modifier.weight(1f),
                )
                MetricTile(
                    label = "Tokens",
                    value = Fmt.tokens(day.totalTokens),
                    modifier = Modifier.weight(1f),
                )
            }
            val models = day.models.sortedByDescending { it.costUSD }
            if (models.isNotEmpty()) {
                Text(
                    text = "Models",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                models.forEach { model ->
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        horizontalArrangement = Arrangement.SpaceBetween,
                    ) {
                        Text(
                            text = model.name,
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Text(
                            text = "${Fmt.money(model.costUSD, currency)} · " +
                                Fmt.tokens(model.totalTokens),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            if (day.limitResets.isNotEmpty()) {
                Text(
                    text = "Unused limit resets",
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                day.limitResets.forEach { reset -> LimitResetRow(reset = reset) }
            }
        }
    }
}

@Composable
private fun LimitResetRow(reset: LimitReset) {
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(
            text = reset.title,
            style = MaterialTheme.typography.bodyMedium,
        )
        val parts = buildList {
            add("${Fmt.percent(reset.percentLeft)} left")
            Fmt.dateTime(reset.resetsAt)?.let { add("resets $it") }
        }
        Text(
            text = parts.joinToString(" · "),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// ---------------------------------------------------------------------------
// Merged model breakdown
// ---------------------------------------------------------------------------

@Composable
private fun ModelShareRow(
    model: ModelUsage,
    totalCost: Double,
    accent: Color,
    currency: String?,
) {
    val share = if (totalCost > 0.0) {
        (model.costUSD / totalCost).toFloat().coerceIn(0f, 1f)
    } else {
        0f
    }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Text(
                text = model.name,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                modifier = Modifier.weight(1f),
            )
            Text(
                text = Fmt.money(model.costUSD, currency),
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.SemiBold,
            )
        }
        Text(
            text = Fmt.tokens(model.totalTokens),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(6.dp)
                .clip(RoundedCornerShape(50))
                .background(MaterialTheme.colorScheme.surfaceVariant),
        ) {
            Box(
                modifier = Modifier
                    .fillMaxHeight()
                    .fillMaxWidth(share)
                    .clip(RoundedCornerShape(50))
                    .background(accent),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Devices and metadata
// ---------------------------------------------------------------------------

@Composable
private fun DeviceRow(device: RemoteDevice) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = device.device,
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f),
        )
        val whenText = Fmt.relativeTime(device.updatedAt)
            ?: Fmt.dateTime(device.receivedAt)
            ?: "unknown"
        Text(
            text = "updated $whenText",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun MetadataCard(entry: ProviderEntry) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text(
                text = "Metadata",
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            KeyValueRow(label = "Provider", value = entry.provider)
            KeyValueRow(label = "Account", value = entry.account ?: Fmt.PLACEHOLDER)
            KeyValueRow(label = "Source", value = entry.source ?: Fmt.PLACEHOLDER)
            KeyValueRow(label = "Updated", value = Fmt.dateTime(entry.updatedAt) ?: Fmt.PLACEHOLDER)
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** App-bar second line: organization (or account) plus how fresh the entry is. */
private fun appBarSubtitle(entry: ProviderEntry): String {
    val identity = entry.organization?.takeIf { it.isNotBlank() }
        ?: entry.account?.takeIf { it.isNotBlank() }
    val updated = Fmt.relativeTime(entry.updatedAt)
    return listOfNotNull(identity, updated).joinToString(" · ").ifBlank { entry.provider }
}

/** `relativeTime` with an absolute timestamp fallback, never blank. */
private fun relative(iso: String): String = Fmt.relativeTime(iso) ?: Fmt.dateTime(iso) ?: iso

private fun creditsSubtitle(credits: LimitResetCredits): String {
    val next = credits.nextExpiresAt
    return if (next != null) "Next expires ${relative(next)}" else "Free rate-limit resets"
}

/** Sums each model across every day, sorted by total cost descending. */
private fun mergeModels(days: List<DailyUsagePoint>): List<ModelUsage> {
    val cost = LinkedHashMap<String, Double>()
    val tokens = LinkedHashMap<String, Long>()
    for (day in days) {
        for (model in day.models) {
            cost[model.name] = (cost[model.name] ?: 0.0) + model.costUSD
            tokens[model.name] = (tokens[model.name] ?: 0L) + model.totalTokens
        }
    }
    return cost.keys
        .map { name -> ModelUsage(name = name, costUSD = cost[name] ?: 0.0, totalTokens = tokens[name] ?: 0L) }
        .sortedByDescending { it.costUSD }
}

/** Turns `listPriceEstimate` into `List Price Estimate` for the provenance caption. */
private fun provenanceLabel(raw: String): String {
    val spaced = raw
        .replace('_', ' ')
        .replace('-', ' ')
        .replace(Regex("([a-z0-9])([A-Z])"), "$1 $2")
        .trim()
    if (spaced.isBlank()) return raw
    return spaced.replaceFirstChar { it.uppercaseChar() }
}
