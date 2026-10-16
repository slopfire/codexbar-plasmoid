package org.slopfire.codexbar.widget

import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.LocalContext
import androidx.glance.action.clickable
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ColumnScope
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.layout.ContentScale
import androidx.glance.text.Text
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.MainActivity
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.data.widget.WidgetPaceStyle
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetSnapshot
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.theme.providerAccentPair
import org.slopfire.codexbar.ui.theme.providerIconRes
import org.slopfire.codexbar.ui.theme.providerLabel
import org.slopfire.codexbar.ui.theme.providerMonogram
import kotlin.math.roundToInt

/**
 * The palette for the widget currently being rendered. Provided by each widget so
 * the shell, bars and tiles all agree without every signature taking a palette.
 */
internal val LocalWidgetPalette = androidx.compose.runtime.staticCompositionLocalOf<WidgetPalette> {
    error("WidgetPalette was not provided")
}

/**
 * Shared widget building blocks.
 *
 * Design rules applied from the M3 Expressive skill:
 * - **Containment**: one container per widget, with generous internal padding and
 *   the brightest surface reserved for the number that matters.
 * - **Gaps, not dividers**: rows in a contained list are separated by space.
 * - **Shape**: cards/containers use the published `CornerMedium` (12dp) step —
 *   large radii clip information-dense content. The widget's own outer radius
 *   defaults to `CornerLarge` (16dp) with `System/Medium/XL` options, so the
 *   nested 12dp satisfies `inner = outer − padding` at the default.
 * - **Typography**: emphasized styles only on the headline number and row titles,
 *   never as the whole scale.
 * - **Color**: remaining-limit health keeps its semantic scale so "about to run
 *   out" reads the same under any wallpaper palette.
 */

/** Container radius for a grouping inside the widget (`CornerMedium`, 12dp). */
private val GROUP_RADIUS = 12.dp

/** Pacing-marker width (`smallSpacing / 2`-scale tick, readable at a glance). */
private val MARKER_WIDTH = 3.dp

/** Transparent split around the marker so the tick divides track and fill. */
private val MARKER_GAP = 2.dp

/**
 * The widget shell: the launcher's app-widget background (which owns the outer
 * radius on Android 12+), an optional Material You fill, and a tap target that
 * opens the app.
 *
 * From M3: containment is one container per widget with generous internal
 * padding. My design decision (not in M3): the background/corners options —
 * Transparent skips the fill so the wallpaper shows through, Translucent uses
 * the same role at reduced alpha, and System leaves the radius to the launcher.
 */
@Composable
fun WidgetShell(
    background: WidgetBackground = WidgetBackground.Solid,
    corners: WidgetCorners = WidgetCorners.Large,
    contentPadding: Dp = 12.dp,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val fill = LocalWidgetPalette.current.shellFor(background)
    val radius = WidgetTheme.shellRadius(corners)
    var modifier = GlanceModifier
        .fillMaxSize()
        .appWidgetBackground()
    if (radius != null) modifier = modifier.cornerRadius(radius)
    if (fill != null) modifier = modifier.background(fill)
    modifier = modifier
        .padding(contentPadding)
        .clickable(actionStartActivity(Intent(context, MainActivity::class.java)))
    Box(
        modifier = modifier,
        contentAlignment = Alignment.TopStart,
    ) {
        Column(modifier = GlanceModifier.fillMaxSize()) { content() }
    }
}

/**
 * Header line: the product name and how fresh the data is.
 *
 * Deliberately no server address — it is noise on a home screen. Freshness is the
 * part worth a glance, and it is tinted once the payload goes stale or a refresh
 * fails, which is the signal that actually matters.
 */
@Composable
fun WidgetHeader(snapshot: WidgetSnapshot?, trailing: (@Composable () -> Unit)? = null) {
    val freshness = freshnessLabel(snapshot)
    Row(
        modifier = GlanceModifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = "CodexBar",
            style = WidgetType.colored(WidgetType.labelMedium, GlanceTheme.colors.onSurface),
        )
        if (freshness != null) {
            Spacer(modifier = GlanceModifier.width(8.dp))
            Text(
                text = freshness.text,
                style = WidgetType.colored(WidgetType.labelSmall, freshness.color),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight(),
            )
        } else {
            Spacer(modifier = GlanceModifier.defaultWeight())
        }
        trailing?.invoke()
    }
}

private class Freshness(val text: String, val color: ColorProvider)

/** Text and semantic color for the payload's age, or null when there is none. */
@Composable
private fun freshnessLabel(snapshot: WidgetSnapshot?): Freshness? {
    if (snapshot == null) return null
    val ageText = Fmt.relativeTime(
        java.time.Instant.ofEpochMilli(snapshot.generatedAtMillis).toString(),
    )
    if (snapshot.error != null) {
        return Freshness(snapshot.error, solidColor(Health.Critical))
    }
    if (ageText == null) return null
    val ageMillis = System.currentTimeMillis() - snapshot.generatedAtMillis
    // Past two refresh periods (the platform floor is 30 minutes) the number on
    // screen is old enough to be worth flagging.
    val stale = ageMillis > STALE_AFTER_MILLIS
    return Freshness(
        if (stale) "stale · $ageText" else ageText,
        if (stale) solidColor(Health.Tight) else GlanceTheme.colors.onSurfaceVariant,
    )
}

private const val STALE_AFTER_MILLIS = 35 * 60 * 1000L

/**
 * A provider badge: the bundled brand glyph, tinted with the provider's accent
 * color, on a tinted fully rounded tile.
 *
 * The glyphs are white silhouettes rasterized from the plasmoid's own icon set,
 * so the same art renders here as in the app and on the desktop widget. Tinting
 * is required rather than optional: the source art is white, which would be
 * invisible on a light surface, and the provider accent *is* the identity color.
 *
 * Falls back to initials when this build bundles no icon for the provider id.
 */
@Composable
fun ProviderBadge(provider: String, size: Dp = 24.dp) {
    val context = LocalContext.current
    val (day, night) = providerAccentPair(provider)
    val accent = androidx.glance.color.ColorProvider(day = day, night = night)
    val iconRes = remember(provider) { providerIconRes(context, provider) }

    Box(
        modifier = GlanceModifier
            .size(size)
            .cornerRadius(size / 2)
            .background(day.copy(alpha = 0.18f)),
        contentAlignment = Alignment.Center,
    ) {
        if (iconRes == 0) {
            Text(
                text = providerMonogram(provider),
                style = WidgetType.colored(WidgetType.labelSmall, accent),
            )
        } else {
            Image(
                provider = ImageProvider(iconRes),
                contentDescription = providerLabel(provider),
                modifier = GlanceModifier.size(size * 0.62f),
                contentScale = ContentScale.Fit,
                colorFilter = ColorFilter.tint(accent),
            )
        }
    }
}

/**
 * One provider's tightest window: title, remaining percent, and a full-round
 * bar. The percent is the emphasized headline for the row.
 */
@Composable
fun LimitLine(
    provider: WidgetProvider,
    showProviderName: Boolean = true,
    compact: Boolean = false,
    showCaption: Boolean = true,
    showBadge: Boolean = true,
    barHeight: Dp = 8.dp,
    paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
) {
    val row = provider.tightest
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Row(
            modifier = GlanceModifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showProviderName) {
                if (showBadge) {
                    ProviderBadge(provider.id)
                    Spacer(modifier = GlanceModifier.width(8.dp))
                }
                Text(
                    text = providerLabel(provider.id),
                    style = WidgetType.colored(WidgetType.titleSmall, GlanceTheme.colors.onSurface),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
            } else {
                Text(
                    text = row?.title ?: "No limits reported",
                    style = WidgetType.colored(WidgetType.titleSmall, GlanceTheme.colors.onSurfaceVariant),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
            }
            Spacer(modifier = GlanceModifier.width(8.dp))
            Text(
                text = Fmt.percent(row?.percentLeft),
                style = WidgetType.colored(
                    if (compact) WidgetType.titleMedium else WidgetType.titleLarge,
                    solidColor(Health.remainingColor(row?.percentLeft)),
                ),
            )
        }
        Spacer(modifier = GlanceModifier.height(if (compact) 4.dp else 6.dp))
        LimitBar(
            percentLeft = row?.percentLeft,
            resetsAt = row?.resetsAt,
            windowMinutes = row?.windowMinutes,
            providerId = provider.id,
            barHeight = barHeight,
            paceStyle = paceStyle,
        )
        val caption = captionFor(row, provider, compact, showCaption)
        if (caption != null) {
            Spacer(modifier = GlanceModifier.height(4.dp))
            Text(
                text = caption.text,
                style = WidgetType.colored(WidgetType.labelSmall, solidColor(caption.color)),
                maxLines = 1,
            )
        }
    }
}

private class Caption(val text: String, val color: androidx.compose.ui.graphics.Color)

private fun captionFor(
    row: UsageRow?,
    provider: WidgetProvider,
    compact: Boolean,
    showCaption: Boolean,
): Caption? {
    if (row == null) {
        return when {
            provider.hasError -> Caption("last refresh failed", Health.Critical)
            provider.stale -> Caption("showing last good data", Health.Tight)
            else -> null
        }
    }
    // The reset/pace line is optional; a degraded-state note never is.
    if (!showCaption) return null
    val parts = buildList {
        add(row.title)
        Fmt.resetCountdown(row.resetsAt)?.let { add(it) }
        Fmt.paceText(row.pace)?.let { add(it) }
    }
    if (parts.isEmpty()) return null
    val paceColor = Health.paceColor(row.pace?.willLastToReset)
    val text = if (compact) parts.take(3).joinToString(" · ") else parts.joinToString(" · ")
    val color = if (row.pace?.willLastToReset == false) paceColor else Health.Neutral
    return Caption(text, color)
}

/**
 * The remaining-budget bar: a full-round track with the remaining fraction
 * filled, split at the pacing marker like the desktop plasmoid's
 * `UsageBarRow`. The fill reaching past the marker means the budget outlasts
 * the clock. Full-round is the published `CornerFull` step for progress
 * indicators; the marker colors mirror the desktop `paceIndicatorStyle`.
 *
 * From M3: active indicator → Primary, track → Secondary container. My design
 * decision (not in M3): the fill keeps the semantic remaining-limit scale so
 * "about to run out" reads the same under any wallpaper palette, while only
 * the marker follows the pacing style.
 *
 * Glance's RemoteViews layout has no fractional widths — every weighted child
 * shares the row equally — so the bar is quantized into [BAR_CELLS] equal
 * cells. Adjacent same-color cells merge into a smooth pill; the marker (or a
 * bare gap) takes the slot where the clock sits.
 */
@Composable
fun LimitBar(
    percentLeft: Double?,
    resetsAt: String?,
    windowMinutes: Double?,
    providerId: String,
    barHeight: Dp = 8.dp,
    paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
) {
    val fillFraction = ((percentLeft ?: 0.0) / 100.0).coerceIn(0.0, 1.0)
    val fill = solidColor(Health.remainingColor(percentLeft))
    val track = LocalWidgetPalette.current.track
    val markerFraction = if (paceStyle == WidgetPaceStyle.Off) {
        null
    } else {
        WidgetPace.timeLeftFraction(resetsAt, windowMinutes)
    }
    // Marker slot index: cells before it go left, the rest go right.
    val slot = WidgetPace.markerSlot(markerFraction, BAR_CELLS)
    val fillSlotCount = WidgetPace.fillSlots(fillFraction, BAR_CELLS)
    val marker = if (markerFraction == null) {
        null
    } else {
        paceMarkerColor(
            style = paceStyle,
            markerFraction = markerFraction,
            fill = fill,
            providerId = providerId,
        )
    }
    val rowHeight = if (slot == null) barHeight else barHeight + 4.dp
    Row(
        modifier = GlanceModifier.fillMaxWidth().height(rowHeight),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        val leftCount = slot ?: BAR_CELLS
        repeat(leftCount) { index ->
            BarCell(filled = index < fillSlotCount, fill = fill, track = track, barHeight = barHeight)
        }
        if (slot != null) {
            if (marker == null) {
                // Gaps only: a bare transparent split, like the desktop style.
                Spacer(modifier = GlanceModifier.width(MARKER_WIDTH))
            } else {
                Spacer(modifier = GlanceModifier.width(MARKER_GAP))
                Box(
                    modifier = GlanceModifier
                        .width(MARKER_WIDTH)
                        .height(rowHeight)
                        .cornerRadius(MARKER_WIDTH / 2)
                        .background(marker),
                ) {}
                Spacer(modifier = GlanceModifier.width(MARKER_GAP))
            }
            for (index in slot until BAR_CELLS) {
                BarCell(filled = index < fillSlotCount, fill = fill, track = track, barHeight = barHeight)
            }
        }
    }
}

/** Equal-share cells per bar (RemoteViews weighting is equal-share only). */
private const val BAR_CELLS = 24

/** Marker color for a pacing style, or null for the bare-gap style. */
@Composable
private fun paceMarkerColor(
    style: WidgetPaceStyle,
    markerFraction: Double,
    fill: ColorProvider,
    providerId: String,
): ColorProvider? = when (style) {
    WidgetPaceStyle.Off -> null
    WidgetPaceStyle.Gaps -> null
    // The gradient evaluated where the clock is: ahead reads green, behind red.
    WidgetPaceStyle.Position -> solidColor(Health.remainingColor(markerFraction * 100.0))
    WidgetPaceStyle.Bar -> fill
    WidgetPaceStyle.Provider -> {
        val (day, night) = providerAccentPair(providerId)
        androidx.glance.color.ColorProvider(day = day, night = night)
    }
    WidgetPaceStyle.Theme -> GlanceTheme.colors.onSurface
}

/**
 * One bar cell: full-round track or fill. Neighbouring same-color cells merge
 * into a smooth pill, so a split bar reads as expressive segments.
 */
@Composable
private fun androidx.glance.layout.RowScope.BarCell(
    filled: Boolean,
    fill: ColorProvider,
    track: ColorProvider,
    barHeight: Dp,
) {
    Box(
        modifier = GlanceModifier
            .defaultWeight()
            .height(barHeight)
            .cornerRadius(barHeight / 2)
            .background(if (filled) fill else track),
    ) {}
}

/** A color that is identical in day and night mode — the semantic health scale. */
internal fun solidColor(color: androidx.compose.ui.graphics.Color): ColorProvider =
    androidx.glance.color.ColorProvider(day = color, night = color)

/** A small metric block: label above, emphasized value, optional caption. */
@Composable
fun MetricBlock(
    label: String,
    value: String,
    caption: String? = null,
    modifier: GlanceModifier = GlanceModifier,
) {
    Column(modifier = modifier) {
        Text(
            text = label,
            style = WidgetType.colored(WidgetType.labelSmall, GlanceTheme.colors.onSurfaceVariant),
            maxLines = 1,
        )
        Text(
            text = value,
            style = WidgetType.colored(WidgetType.titleLarge, GlanceTheme.colors.onSurface),
            maxLines = 1,
        )
        if (caption != null) {
            Text(
                text = caption,
                style = WidgetType.colored(WidgetType.labelSmall, GlanceTheme.colors.onSurfaceVariant),
                maxLines = 1,
            )
        }
    }
}

/** Centered message used for the not-configured and error states. */
@Composable
fun WidgetMessage(title: String, detail: String) {
    Column(
        modifier = GlanceModifier.fillMaxSize(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = title,
            style = WidgetType.colored(WidgetType.titleMedium, GlanceTheme.colors.onSurface),
            maxLines = 1,
        )
        Spacer(modifier = GlanceModifier.height(4.dp))
        Text(
            text = detail,
            style = WidgetType.colored(WidgetType.bodySmall, GlanceTheme.colors.onSurfaceVariant),
            maxLines = 2,
        )
    }
}

/** A single muted or semantic caption line under a block. */
@Composable
fun WidgetCaption(text: String, color: androidx.glance.unit.ColorProvider) {
    Text(
        text = text,
        style = WidgetType.colored(WidgetType.labelSmall, color),
        maxLines = 1,
    )
}
