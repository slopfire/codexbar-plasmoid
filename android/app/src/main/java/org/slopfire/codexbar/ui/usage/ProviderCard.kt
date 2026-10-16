package org.slopfire.codexbar.ui.usage

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.animation.animateContentSize
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.slopfire.codexbar.data.model.ProviderEntry
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.common.InfoChip
import org.slopfire.codexbar.ui.common.KeyValueRow
import org.slopfire.codexbar.ui.common.MetricTile
import org.slopfire.codexbar.ui.theme.ExpressiveType
import org.slopfire.codexbar.ui.common.ProviderAvatar
import org.slopfire.codexbar.ui.common.StatusPill
import org.slopfire.codexbar.ui.theme.providerAccent
import org.slopfire.codexbar.ui.theme.providerContainerColor
import org.slopfire.codexbar.ui.theme.providerLabel

/**
 * One provider+account row in the usage list.
 *
 * The card is the tappable summary: identity, every limit window, credits, local
 * token spend and any degraded-state notes. The whole container carries a
 * little of the provider's own color — enough to tell providers apart at a
 * glance, little enough that the text on top keeps its contrast.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ProviderCard(
    entry: ProviderEntry,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val accent = providerAccent(entry.provider)
    val subtitle = entry.account ?: entry.organization ?: entry.plan

    Card(
        onClick = onClick,
        modifier = modifier
            .fillMaxWidth()
            .animateContentSize(),
        shape = MaterialTheme.shapes.medium,
        colors = CardDefaults.cardColors(
            containerColor = providerContainerColor(entry.provider),
        ),
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            CardHeader(entry = entry, accent = accent, subtitle = subtitle)
            LimitWindows(entry = entry)
            CreditsSection(entry = entry)
            TokenSection(entry = entry)
            StateNotes(entry = entry)
            CardFooter(entry = entry, subtitle = subtitle)
        }
    }
}

@Composable
private fun CardHeader(entry: ProviderEntry, accent: Color, subtitle: String?) {
    Row(verticalAlignment = Alignment.Top) {
        ProviderAvatar(provider = entry.provider, size = 48.dp)
        Spacer(Modifier.width(12.dp))
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    text = providerLabel(entry.provider),
                    style = ExpressiveType.titleMedium,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
                if (entry.status?.indicator?.isNotBlank() == true) {
                    Spacer(Modifier.width(8.dp))
                    StatusPill(entry.status)
                }
            }
            if (!subtitle.isNullOrBlank()) {
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            MetadataChips(entry = entry, accent = accent)
        }
    }
}

@Composable
private fun MetadataChips(entry: ProviderEntry, accent: Color) {
    val source = entry.source?.takeIf { it.isNotBlank() }
    val deviceCount = entry.remoteDevices.size
    if (source == null && deviceCount == 0) return
    Row(
        modifier = Modifier.padding(top = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (source != null) {
            InfoChip(
                text = source,
                containerColor = accent.copy(alpha = 0.16f),
                contentColor = accent,
            )
        }
        if (deviceCount > 0) {
            InfoChip(text = if (deviceCount == 1) "1 device" else "$deviceCount devices")
        }
    }
}

@Composable
private fun LimitWindows(entry: ProviderEntry) {
    if (entry.rows.isEmpty()) {
        KeyValueRow(label = "Limit windows", value = "None reported")
        return
    }
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        entry.rows.forEach { row ->
            UsageLimitBar(row = row)
        }
    }
}

@Composable
private fun CreditsSection(entry: ProviderEntry) {
    val credits = entry.creditsRemaining
    val resetCredits = entry.limitResetCredits
    val resetCount = resetCredits?.availableCount ?: 0
    if (credits == null && resetCount <= 0) return

    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (credits != null) {
                MetricTile(
                    label = "Credits",
                    value = Fmt.money(credits, entry.tokenUsage?.currencyCode),
                )
            }
            if (resetCount > 0) {
                InfoChip(text = if (resetCount == 1) "1 reset credit" else "$resetCount reset credits")
            }
        }
        val expires = resetCredits?.nextExpiresAt
        if (resetCount > 0 && expires != null) {
            Fmt.relativeTime(expires)?.let { relative ->
                Text(
                    text = "expires $relative",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@Composable
private fun TokenSection(entry: ProviderEntry) {
    val usage = entry.tokenUsage ?: return
    val currency = usage.currencyCode
    val showSession = usage.sessionCostUSD != null || usage.sessionTokens != null
    val showLast30 = usage.last30DaysCostUSD != null || usage.last30DaysTokens != null
    if (!showSession && !showLast30) return
    Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
        if (showSession) {
            MetricTile(
                label = usage.sessionLabel ?: "Today",
                value = Fmt.money(usage.sessionCostUSD, currency),
                caption = usage.sessionTokens?.let { Fmt.tokens(it) + " tokens" },
                modifier = Modifier.weight(1f),
            )
        }
        if (showLast30) {
            MetricTile(
                label = usage.last30DaysLabel ?: "30d",
                value = Fmt.money(usage.last30DaysCostUSD, currency),
                caption = usage.last30DaysTokens?.let { Fmt.tokens(it) + " tokens" },
                modifier = Modifier.weight(1f),
            )
        }
    }
}

/** A degraded-state note: soft notes are recoverable, hard notes need attention. */
private data class EntryNote(
    val text: String,
    val caption: String? = null,
    val hard: Boolean = false,
)

private fun notesFor(entry: ProviderEntry): List<EntryNote> {
    val notes = mutableListOf<EntryNote>()
    if (entry.stale) {
        val base = "Showing last good data, retrying"
        val since = Fmt.relativeTime(entry.staleSince)
        notes += EntryNote(
            text = if (since != null) "$base · $since" else base,
            caption = entry.lastAttemptError?.takeIf { it.isNotBlank() },
        )
    }
    entry.error?.takeIf { it.isNotBlank() }?.let { notes += EntryNote(text = it, hard = true) }
    entry.usageUnavailable?.takeIf { it.isNotBlank() }?.let { notes += EntryNote(text = it, hard = true) }
    return notes
}

@Composable
private fun StateNotes(entry: ProviderEntry) {
    val notes = notesFor(entry)
    if (notes.isEmpty()) return
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        notes.forEach { note -> NoteRow(note) }
    }
}

@Composable
private fun NoteRow(note: EntryNote) {
    val tint = if (note.hard) MaterialTheme.colorScheme.error else Health.Tight
    Column {
        Row(verticalAlignment = Alignment.Top) {
            Icon(
                imageVector = if (note.hard) Icons.Default.Error else Icons.Default.Warning,
                contentDescription = null,
                modifier = Modifier
                    .padding(top = 2.dp)
                    .size(16.dp),
                tint = tint,
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = note.text,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.weight(1f),
            )
        }
        if (note.caption != null) {
            Text(
                text = note.caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 24.dp, top = 2.dp),
            )
        }
    }
}

@Composable
private fun CardFooter(entry: ProviderEntry, subtitle: String?) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "updated ${Fmt.relativeTime(entry.updatedAt) ?: "unknown"}",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        val plan = entry.plan
        if (!plan.isNullOrBlank() && plan != subtitle) {
            InfoChip(text = plan)
        }
    }
}
