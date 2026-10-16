package org.slopfire.codexbar.ui.common

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.Icon
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.MaterialShapes
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.toShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import org.slopfire.codexbar.data.model.ProviderStatus
import org.slopfire.codexbar.ui.theme.ExpressiveType
import org.slopfire.codexbar.ui.theme.providerAccent
import org.slopfire.codexbar.ui.theme.providerIconRes
import org.slopfire.codexbar.ui.theme.providerMonogram
import org.slopfire.codexbar.ui.theme.providerLabel

/**
 * Shared building blocks used by more than one screen. Keeping them here (rather
 * than in a screen file) means the usage, detail, devices and settings screens
 * all render provider identity and status identically.
 */

/**
 * A provider's bundled icon tinted with its accent color, on an expressive
 * cookie-shaped tile. Falls back to a monogram when this app bundles no icon for
 * the provider id.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun ProviderAvatar(
    provider: String,
    modifier: Modifier = Modifier,
    size: Dp = 44.dp,
    contentColor: Color? = null,
) {
    val context = LocalContext.current
    val accent = providerAccent(provider)
    val tint = contentColor ?: accent
    val iconRes = remember(provider) { providerIconRes(context, provider) }
    val shape = MaterialShapes.Cookie9Sided.toShape()

    Box(
        modifier = modifier
            .size(size)
            .clip(shape)
            .background(accent.copy(alpha = 0.18f)),
        contentAlignment = Alignment.Center,
    ) {
        if (iconRes == 0) {
            // No bundled icon for this provider id: fall back to initials.
            Text(
                text = providerMonogram(provider),
                color = tint,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
        } else {
            Image(
                painter = painterResource(iconRes),
                contentDescription = providerLabel(provider),
                modifier = Modifier
                    .fillMaxSize()
                    .padding(size * 0.24f),
                colorFilter = ColorFilter.tint(tint),
            )
        }
    }
}

/** Provider status-page indicator as a compact dot + label pill. */
@Composable
fun StatusPill(
    status: ProviderStatus?,
    modifier: Modifier = Modifier,
) {
    val indicator = status?.indicator.orEmpty()
    if (indicator.isBlank()) return
    val color = statusColor(indicator)
    Row(
        modifier = modifier,
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
            text = statusText(status, indicator),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun statusColor(indicator: String): Color = when (indicator.lowercase()) {
    "none" -> Health.Good
    "minor", "maintenance" -> Health.Tight
    "major", "critical" -> Health.Critical
    else -> Health.Neutral
}

private fun statusText(status: ProviderStatus?, indicator: String): String {
    status?.description?.takeIf { it.isNotBlank() }?.let { return it }
    return when (indicator.lowercase()) {
        "none" -> "Operational"
        "minor" -> "Partial outage"
        "major" -> "Major outage"
        "critical" -> "Critical issue"
        "maintenance" -> "Maintenance"
        else -> "Status unknown"
    }
}

/** A small tinted chip for source, plan, or device metadata. */
@Composable
fun InfoChip(
    text: String,
    modifier: Modifier = Modifier,
    containerColor: Color? = null,
    contentColor: Color? = null,
) {
    val container = containerColor ?: MaterialTheme.colorScheme.surfaceVariant
    val content = contentColor ?: MaterialTheme.colorScheme.onSurfaceVariant
    Surface(
        modifier = modifier,
        shape = RoundedCornerShape(50),
        color = container,
        contentColor = content,
    ) {
        Text(
            text = text,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 4.dp),
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
        )
    }
}

/** Section title with an optional caption, used between grouped cards. */
@Composable
fun SectionHeader(
    title: String,
    modifier: Modifier = Modifier,
    subtitle: String? = null,
) {
    Column(modifier = modifier.fillMaxWidth()) {
        Text(
            text = title,
            style = ExpressiveType.titleMedium,
        )
        if (!subtitle.isNullOrBlank()) {
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** Label / value pair used in the detail and device screens. */
@Composable
fun KeyValueRow(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
) {
    Row(
        modifier = modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.Top,
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.weight(1f),
        )
        Text(
            text = value,
            style = MaterialTheme.typography.bodyMedium,
            fontWeight = FontWeight.Medium,
            textAlign = TextAlign.End,
            modifier = Modifier.weight(1.4f),
        )
    }
}

/** A metric with a big expressive value, e.g. today's spend. */
@Composable
fun MetricTile(
    label: String,
    value: String,
    modifier: Modifier = Modifier,
    caption: String? = null,
    accent: Color? = null,
    animateValue: Boolean = false,
) {
    Column(modifier = modifier.animateContentSize()) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (animateValue) {
            // Values roll in from the direction of the change on every refresh,
            // so a quiet polling refresh still reads as "the data moved".
            AnimatedMetricValue(value = value, accent = accent)
        } else {
            Text(
                text = value,
                style = ExpressiveType.titleLarge,
                color = accent ?: MaterialTheme.colorScheme.onSurface,
            )
        }
        if (!caption.isNullOrBlank()) {
            Text(
                text = caption,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
private fun AnimatedMetricValue(value: String, accent: Color?) {
    // Hoisted out of the transition spec: the motion scheme is composable state.
    val effectsSpec = MaterialTheme.motionScheme.defaultEffectsSpec<Float>()
    val spatialSpec = MaterialTheme.motionScheme.fastSpatialSpec<IntOffset>()
    AnimatedContent(
        targetState = value,
        transitionSpec = {
            (
                fadeIn(animationSpec = effectsSpec) +
                    slideInVertically(animationSpec = spatialSpec) { height -> height / 2 }
                ) togetherWith (
                fadeOut(animationSpec = effectsSpec) +
                    slideOutVertically(animationSpec = spatialSpec) { height -> -height / 2 }
                )
        },
        label = "metric-value",
    ) { text ->
        Text(
            text = text,
            style = ExpressiveType.titleLarge,
            color = accent ?: MaterialTheme.colorScheme.onSurface,
        )
    }
}

/** Full-screen state while the first load runs. */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun LoadingBlock(
    message: String,
    modifier: Modifier = Modifier,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        LoadingIndicator()
        Text(
            text = message,
            modifier = Modifier.padding(top = 20.dp),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
    }
}

/** Centered empty / error state with an optional action slot. */
@Composable
fun EmptyState(
    title: String,
    message: String,
    modifier: Modifier = Modifier,
    icon: ImageVector? = null,
    action: (@Composable () -> Unit)? = null,
) {
    Column(
        modifier = modifier
            .fillMaxSize()
            .padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        if (icon != null) {
            Icon(
                imageVector = icon,
                contentDescription = null,
                modifier = Modifier.size(44.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Text(
            text = title,
            modifier = Modifier.padding(top = if (icon != null) 16.dp else 0.dp),
            style = ExpressiveType.titleLarge,
            textAlign = TextAlign.Center,
        )
        Text(
            text = message,
            modifier = Modifier.padding(top = 8.dp),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            textAlign = TextAlign.Center,
        )
        if (action != null) {
            Box(modifier = Modifier.padding(top = 24.dp)) { action() }
        }
    }
}

/** Inline error surface that keeps the last good data on screen. */
@Composable
fun ErrorBanner(
    message: String,
    modifier: Modifier = Modifier,
) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.errorContainer,
        contentColor = MaterialTheme.colorScheme.onErrorContainer,
    ) {
        Text(
            text = message,
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 12.dp),
            style = MaterialTheme.typography.bodySmall,
        )
    }
}
