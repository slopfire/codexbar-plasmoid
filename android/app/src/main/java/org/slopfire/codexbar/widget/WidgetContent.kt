package org.slopfire.codexbar.widget

import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.LocalContext
import androidx.glance.LocalSize
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
import androidx.glance.text.TextAlign
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.MainActivity
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.data.widget.WidgetPaceStyle
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.theme.providerAccentPair
import org.slopfire.codexbar.ui.theme.providerIconRes
import org.slopfire.codexbar.ui.theme.providerLabel
import org.slopfire.codexbar.ui.theme.providerMonogram

/**
 * The palette for the widget currently being rendered. Provided by each widget so
 * the shell, bars and tiles all agree without every signature taking a palette.
 */
internal val LocalWidgetPalette = androidx.compose.runtime.staticCompositionLocalOf<WidgetPalette> {
    error("WidgetPalette was not provided")
}

/**
 * The shell's resolved fill and text colors, provided by [WidgetShell] so every
 * descendant paints readable text for the background the user actually chose.
 */
internal val LocalWidgetSurface = androidx.compose.runtime.staticCompositionLocalOf<WidgetSurface> {
    error("WidgetSurface was not provided")
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
 * Strength of the tinted tile behind a provider badge. Raised from 0.18 because a
 * see-through shell gives the badge less to sit on than an opaque one.
 */
private const val BADGE_CHIP_ALPHA = 0.22f

/**
 * The widget shell: the launcher's app-widget background (which owns the outer
 * radius on Android 12+), an optional Material You fill, and a tap target that
 * opens the app.
 *
 * From M3: containment is one container per widget with generous internal
 * padding. My design decision (not in M3): the background/corners options —
 * Transparent skips the fill so the wallpaper shows through, Translucent uses
 * the same role at a scrim alpha, and System leaves the radius to the launcher.
 *
 * The shell is also what makes text readable: it resolves the palette for the
 * chosen background and provides it, so a Translucent or Transparent widget
 * paints stronger text instead of the opaque-surface roles.
 */
@Composable
fun WidgetShell(
    background: WidgetBackground = WidgetBackground.Solid,
    corners: WidgetCorners = WidgetCorners.Large,
    contentPadding: Dp = 12.dp,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val surface = LocalWidgetPalette.current.surfaceFor(background)
    val radius = WidgetTheme.shellRadius(corners)
    var modifier = GlanceModifier
        .fillMaxSize()
        .appWidgetBackground()
    if (radius != null) modifier = modifier.cornerRadius(radius)
    surface.fill?.let { modifier = modifier.background(it) }
    modifier = modifier
        .padding(contentPadding)
        .clickable(actionStartActivity(Intent(context, MainActivity::class.java)))
    CompositionLocalProvider(LocalWidgetSurface provides surface) {
        Box(
            modifier = modifier,
            contentAlignment = Alignment.TopStart,
        ) {
            Column(modifier = GlanceModifier.fillMaxSize()) { content() }
        }
    }
}

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
            .background(
                androidx.glance.color.ColorProvider(
                    day = day.copy(alpha = BADGE_CHIP_ALPHA),
                    night = night.copy(alpha = BADGE_CHIP_ALPHA),
                ),
            ),
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
                    style = WidgetType.colored(WidgetType.titleSmall, LocalWidgetSurface.current.onShell),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
            } else {
                Text(
                    text = row?.title ?: "No limits reported",
                    style = WidgetType.colored(WidgetType.titleSmall, LocalWidgetSurface.current.onShellVariant),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
            }
            Spacer(modifier = GlanceModifier.width(8.dp))
            Text(
                text = Fmt.percent(row?.percentLeft),
                style = WidgetType.colored(
                    if (compact) WidgetType.titleMedium else WidgetType.titleLarge,
                    LocalWidgetPalette.current.healthFor(Health.remainingColor(row?.percentLeft)),
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
                style = WidgetType.colored(
                    WidgetType.labelSmall,
                    LocalWidgetPalette.current.healthFor(caption.color),
                ),
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
    val fill = LocalWidgetPalette.current.healthFor(Health.remainingColor(percentLeft))
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
    WidgetPaceStyle.Position ->
        LocalWidgetPalette.current.healthFor(Health.remainingColor(markerFraction * 100.0))
    WidgetPaceStyle.Bar -> fill
    WidgetPaceStyle.Provider -> {
        val (day, night) = providerAccentPair(providerId)
        androidx.glance.color.ColorProvider(day = day, night = night)
    }
    WidgetPaceStyle.Theme -> LocalWidgetSurface.current.onShell
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

/**
 * Geometry shared by the redesigned providers widget.
 *
 * The bar math is dp-based rather than weighted, so every width here is a hard
 * number derived from the widget's own size. [SHELL_PADDING] must stay equal to
 * [WidgetShell]'s default `contentPadding`: the fraction is applied to the space
 * the shell actually leaves, not to the host's reported size.
 */
private val SHELL_PADDING = 12.dp

/** Window label column: wide enough for `5h session`, narrow enough to leave a bar. */
private val WINDOW_LABEL_WIDTH = 76.dp

/** Percent column: `100%` at label medium, right aligned. */
private val WINDOW_VALUE_WIDTH = 40.dp

/** Gap on each side of a window row's bar. */
private val WINDOW_GAP = 8.dp

/** The stop cap at the end of a track (`CornerFull` at this size). */
private val STOP_DOT = 4.dp

/** Clock notch thickness — half the 4dp bar, so it reads as a tick, not a block. */
private val NOTCH_WIDTH = 2.dp

/** Verdict pill radius (`CornerSmall`-ish; the pill is 11sp tall text plus padding). */
private val PILL_RADIUS = 8.dp

/**
 * Estimated widths of the 11sp scale labels.
 *
 * RemoteViews cannot measure text before layout, so the `clock` label is nudged
 * with a spacer derived from the same fraction as the notch and an estimate of
 * its own width. Overestimating only moves the label slightly off-centre; it can
 * never push the `100%` label out of the widget.
 */
private val SCALE_LABEL_WIDTH = 30.dp
private val CLOCK_LABEL_WIDTH = 56.dp

/**
 * The width the shell leaves for content, or null when the host did not report a
 * usable size (preview, or a launcher that ignores `SizeMode.Exact`).
 */
@Composable
private fun widgetInnerWidth(): Dp? {
    val width = LocalSize.current.width
    if (width.value.isNaN() || width <= 0.dp) return null
    return (width - SHELL_PADDING * 2).coerceAtLeast(0.dp)
}

/**
 * One provider's headline for the providers widget: identity and verdict, the one
 * big remaining number, the reset line, and the continuous bar under it.
 *
 * From M3: one hero number per container and one type step between it and the
 * supporting copy. My design decision (not in M3): only the headline gets a solid
 * verdict pill — repeating it down a list would turn the widget into a wall of
 * color, so the collapsed rows keep the pill but drop every other accent.
 */
@Composable
fun ProviderHeadline(
    provider: WidgetProvider,
    barHeight: Dp = 4.dp,
    paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
    showBadge: Boolean = true,
    showMeta: Boolean = true,
    showScale: Boolean = true,
) {
    val surface = LocalWidgetSurface.current
    val palette = LocalWidgetPalette.current
    val row = provider.tightest
    val severity = palette.healthFor(Health.remainingColor(row?.percentLeft))
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        Row(
            modifier = GlanceModifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (showBadge) {
                ProviderBadge(provider.id, size = 20.dp)
                Spacer(modifier = GlanceModifier.width(8.dp))
            }
            Text(
                text = providerLabel(provider.id),
                style = WidgetType.colored(WidgetType.titleSmall, surface.onShell),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight(),
            )
            Spacer(modifier = GlanceModifier.width(8.dp))
            VerdictPill(verdict = WidgetVerdict.forProvider(provider))
        }
        Spacer(modifier = GlanceModifier.height(4.dp))
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                text = Fmt.percent(row?.percentLeft),
                style = WidgetType.colored(WidgetType.displaySmall, severity),
                maxLines = 1,
            )
            Spacer(modifier = GlanceModifier.width(4.dp))
            Text(
                text = "left",
                style = WidgetType.colored(WidgetType.titleSmall, severity),
                maxLines = 1,
                // Glance has no baseline alignment; lifting the unit off the bottom
                // edge approximates the big number's baseline instead.
                modifier = GlanceModifier.padding(bottom = 6.dp),
            )
        }
        if (showMeta) {
            Spacer(modifier = GlanceModifier.height(2.dp))
            Text(
                text = headlineMeta(row, provider),
                style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant),
                maxLines = 1,
            )
        }
        Spacer(modifier = GlanceModifier.height(6.dp))
        ContinuousBar(
            percentLeft = row?.percentLeft,
            resetsAt = row?.resetsAt,
            windowMinutes = row?.windowMinutes,
            providerId = provider.id,
            barHeight = barHeight,
            paceStyle = paceStyle,
            showScale = showScale,
        )
    }
}

/** `monthly · resets in 11d 14h`, or the provider's degraded state when it has no rows. */
private fun headlineMeta(row: UsageRow?, provider: WidgetProvider): String = when {
    row != null -> WidgetVerdict.headlineMeta(row)
    provider.hasError -> "last refresh failed"
    provider.stale -> "showing last good data"
    else -> "no limits reported"
}

/**
 * One window of the expanded provider: label, continuous bar, remaining percent.
 *
 * The list is the evidence behind the headline, so it keeps every window — the
 * headline's own window included — at the same visual weight, with no divider
 * between rows: they are one list, not three sections.
 */
@Composable
fun WindowRow(
    row: UsageRow,
    providerId: String,
    barHeight: Dp = 4.dp,
    paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
) {
    val surface = LocalWidgetSurface.current
    val severity = LocalWidgetPalette.current.healthFor(Health.remainingColor(row.percentLeft))
    val innerWidth = widgetInnerWidth()
    val barWidth = innerWidth?.minus(WINDOW_LABEL_WIDTH + WINDOW_VALUE_WIDTH + WINDOW_GAP * 2)
        ?.coerceAtLeast(0.dp)
    Row(
        modifier = GlanceModifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = WidgetVerdict.windowLabel(row),
            style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant),
            maxLines = 1,
            modifier = GlanceModifier.width(WINDOW_LABEL_WIDTH),
        )
        Spacer(modifier = GlanceModifier.width(WINDOW_GAP))
        ContinuousBar(
            percentLeft = row.percentLeft,
            resetsAt = row.resetsAt,
            windowMinutes = row.windowMinutes,
            providerId = providerId,
            barHeight = barHeight,
            paceStyle = paceStyle,
            availableWidth = barWidth,
            modifier = if (barWidth != null) {
                GlanceModifier.width(barWidth)
            } else {
                GlanceModifier.defaultWeight()
            },
        )
        Spacer(modifier = GlanceModifier.width(WINDOW_GAP))
        Text(
            text = Fmt.percent(row.percentLeft),
            style = WidgetType.colored(WidgetType.labelMedium, severity)
                // The RemoteViews TextView right-aligns inside its fixed column.
                .copy(textAlign = TextAlign.End),
            maxLines = 1,
            modifier = GlanceModifier.width(WINDOW_VALUE_WIDTH),
        )
    }
}

/**
 * A provider the widget is not expanding: badge, name, tightest window, percent
 * and verdict, all on one line. At most two of these are rendered, so the row can
 * afford the verdict that the window list drops.
 *
 * [WidgetVerdict.forRow] answers the window ("tight", "on pace"); [forProvider]
 * is the fallback when the window itself has nothing to say.
 */
@Composable
fun CollapsedProviderRow(
    provider: WidgetProvider,
    showBadge: Boolean = true,
) {
    val surface = LocalWidgetSurface.current
    val row = provider.tightest
    val severity = LocalWidgetPalette.current.healthFor(Health.remainingColor(row?.percentLeft))
    val verdict = WidgetVerdict.forRow(row, provider) ?: WidgetVerdict.forProvider(provider)
    Row(
        modifier = GlanceModifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (showBadge) {
            ProviderBadge(provider.id, size = 20.dp)
            Spacer(modifier = GlanceModifier.width(8.dp))
        }
        Text(
            text = providerLabel(provider.id),
            style = WidgetType.colored(WidgetType.titleSmall, surface.onShell),
            maxLines = 1,
            modifier = GlanceModifier.defaultWeight(),
        )
        Spacer(modifier = GlanceModifier.width(8.dp))
        if (row != null) {
            Text(
                text = WidgetVerdict.windowLabel(row),
                style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant),
                maxLines = 1,
            )
            Spacer(modifier = GlanceModifier.width(8.dp))
        }
        Text(
            text = Fmt.percent(row?.percentLeft),
            style = WidgetType.colored(WidgetType.titleSmall, severity),
            maxLines = 1,
        )
        Spacer(modifier = GlanceModifier.width(6.dp))
        VerdictPill(verdict = verdict)
    }
}

/**
 * The verdict, as one container/on-container pill.
 *
 * From M3: a status statement is a `* container` fill with its `on * container`
 * ink, not ink on ink. The tone picks the health step, the step picks the pair.
 * This is deliberately independent of the percentage's severity: a provider at
 * 60% that is pacing to run dry must still read as an error, because the pill
 * states the outcome, not the budget.
 */
@Composable
fun VerdictPill(verdict: Verdict) {
    val palette = LocalWidgetPalette.current
    val health = verdictToneHealth(verdict.tone)
    Box(
        modifier = GlanceModifier
            .cornerRadius(PILL_RADIUS)
            .background(palette.healthContainerFor(health))
            .padding(horizontal = 6.dp, vertical = 2.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = verdict.text,
            style = WidgetType.colored(WidgetType.labelSmall, palette.onHealthContainerFor(health)),
            maxLines = 1,
        )
    }
}

/** Maps the verdict's semantic tone onto the remaining-limit health steps. */
private fun verdictToneHealth(tone: VerdictTone): Color = when (tone) {
    VerdictTone.SUPPORTED -> Health.Good
    VerdictTone.TERTIARY -> Health.Tight
    VerdictTone.ERROR -> Health.Critical
    VerdictTone.MUTED -> Health.Neutral
}

/**
 * The continuous remaining-budget bar: one track, one fill, a clock notch and a
 * stop dot at the far end.
 *
 * This replaces the 24-cell bar for the providers widget. RemoteViews has no
 * fractional widths, so the old bar was quantized into equal weighted cells and
 * read as a row of sausage links. `SizeMode.Exact` gives [LocalSize] a real width,
 * so the fill can instead be one `Box` whose width is `fraction * innerWidth` —
 * the same shape the desktop plasmoid draws, without the cells.
 *
 * The notch is dp-positioned for the same reason: it is a spacer plus a 2dp box in
 * a row that spans the track, not a weighted share of it. [availableWidth] lets a
 * caller that already carved columns out of the row (the window list) pass the
 * width it actually has.
 */
@Composable
fun ContinuousBar(
    percentLeft: Double?,
    resetsAt: String?,
    windowMinutes: Double?,
    providerId: String = "",
    barHeight: Dp = 4.dp,
    paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
    showScale: Boolean = false,
    availableWidth: Dp? = null,
    modifier: GlanceModifier = GlanceModifier,
) {
    val palette = LocalWidgetPalette.current
    val fill = palette.healthFor(Health.remainingColor(percentLeft))
    val track = palette.track
    // A NaN percentage would turn every dp computation below into NaN; JSON can
    // not produce one today, but a malformed snapshot must still paint a track.
    val rawFraction = (percentLeft ?: 0.0) / 100.0
    val fraction = (if (rawFraction.isNaN()) 0.0 else rawFraction).coerceIn(0.0, 1.0)
    val markerFraction = if (paceStyle == WidgetPaceStyle.Off) {
        null
    } else {
        WidgetPace.timeLeftFraction(resetsAt, windowMinutes)?.coerceIn(0.0, 1.0)
    }
    // Gaps has no ink of its own; a continuous bar cannot split for a gap, so a
    // single text-role tick is the closest the shape allows.
    val notchColor = if (markerFraction == null) {
        null
    } else {
        paceMarkerColor(paceStyle, markerFraction, fill, providerId)
            ?: LocalWidgetSurface.current.onShell
    }
    val rowHeight = if (notchColor == null) barHeight else barHeight + 4.dp
    val trackWidth = availableWidth ?: widgetInnerWidth()

    Column(modifier = modifier) {
        if (trackWidth == null) {
            // No measured width: the fill is dp-positioned, so there is nothing
            // meaningful to divide up. Draw the track rather than a broken fill.
            Box(
                modifier = GlanceModifier
                    .fillMaxWidth()
                    .height(barHeight)
                    .cornerRadius(barHeight / 2)
                    .background(track),
            ) {}
        } else if (trackWidth <= 0.dp) {
            // A narrow slot can spend its whole width on the label and value
            // columns. Keep the row's height so the list does not collapse.
            Spacer(modifier = GlanceModifier.height(rowHeight))
        } else {
            Box(
                modifier = GlanceModifier.width(trackWidth).height(rowHeight),
                contentAlignment = Alignment.CenterStart,
            ) {
                Box(
                    modifier = GlanceModifier
                        .width(trackWidth)
                        .height(barHeight)
                        .cornerRadius(barHeight / 2)
                        .background(track),
                ) {}
                if (fraction > 0.0) {
                    Box(
                        modifier = GlanceModifier
                            .width(trackWidth * fraction.toFloat())
                            .height(barHeight)
                            .cornerRadius(barHeight / 2)
                            .background(fill),
                    ) {}
                }
                // The stop sits at the end of the *track*, not of the fill: it
                // marks where the window ends even when the budget runs out early.
                // At zero remaining it would read as "there is still something at
                // the end", so it is omitted in that one case.
                if (fraction > 0.0) {
                    Box(
                        modifier = GlanceModifier.width(trackWidth).height(rowHeight),
                        contentAlignment = Alignment.CenterEnd,
                    ) {
                        Box(
                            modifier = GlanceModifier
                                .size(STOP_DOT)
                                .cornerRadius(STOP_DOT / 2)
                                .background(fill),
                        ) {}
                    }
                }
                if (notchColor != null && markerFraction != null) {
                    val notchX = trackWidth * markerFraction.toFloat()
                    Row(
                        modifier = GlanceModifier.width(trackWidth).height(rowHeight),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Spacer(
                            modifier = GlanceModifier.width(
                                (notchX - NOTCH_WIDTH / 2).coerceAtLeast(0.dp),
                            ),
                        )
                        Box(
                            modifier = GlanceModifier
                                .width(NOTCH_WIDTH)
                                .height(rowHeight)
                                .cornerRadius(NOTCH_WIDTH / 2)
                                .background(notchColor),
                        ) {}
                    }
                }
            }
        }
        if (showScale && trackWidth != null && trackWidth > 0.dp) {
            Spacer(modifier = GlanceModifier.height(2.dp))
            Row(
                modifier = GlanceModifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Text(
                    text = "0%",
                    style = WidgetType.colored(
                        WidgetType.labelSmall,
                        LocalWidgetSurface.current.onShellVariant,
                    ),
                    maxLines = 1,
                )
                if (markerFraction != null) {
                    // Same fraction as the notch, nudged left by half the label's
                    // estimated width so the text sits under the tick it explains.
                    val notchX = trackWidth * markerFraction.toFloat()
                    val maxPad = (trackWidth - CLOCK_LABEL_WIDTH - SCALE_LABEL_WIDTH * 2)
                        .coerceAtLeast(0.dp)
                    val pad = (notchX - CLOCK_LABEL_WIDTH / 2).coerceIn(0.dp, maxPad)
                    Spacer(modifier = GlanceModifier.width(pad))
                    Text(
                        text = "clock ${Fmt.percent(markerFraction * 100.0)}",
                        style = WidgetType.colored(
                            WidgetType.labelSmall,
                            LocalWidgetSurface.current.onShellVariant,
                        ),
                        maxLines = 1,
                    )
                }
                // The weight spacer only pushes `100%` to the right edge; it never
                // sizes the fill.
                Spacer(modifier = GlanceModifier.defaultWeight())
                Text(
                    text = "100%",
                    style = WidgetType.colored(
                        WidgetType.labelSmall,
                        LocalWidgetSurface.current.onShellVariant,
                    ),
                    maxLines = 1,
                )
            }
        }
    }
}

/** The 1dp hairline between the widget's sections. */
@Composable
fun SectionDivider() {
    Box(
        modifier = GlanceModifier
            .fillMaxWidth()
            .height(1.dp)
            .background(LocalWidgetPalette.current.divider),
    ) {}
}

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
            style = WidgetType.colored(WidgetType.labelSmall, LocalWidgetSurface.current.onShellVariant),
            maxLines = 1,
        )
        Text(
            text = value,
            style = WidgetType.colored(WidgetType.titleLarge, LocalWidgetSurface.current.onShell),
            maxLines = 1,
        )
        if (caption != null) {
            Text(
                text = caption,
                style = WidgetType.colored(WidgetType.labelSmall, LocalWidgetSurface.current.onShellVariant),
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
            style = WidgetType.colored(WidgetType.titleMedium, LocalWidgetSurface.current.onShell),
            maxLines = 1,
        )
        Spacer(modifier = GlanceModifier.height(4.dp))
        Text(
            text = detail,
            style = WidgetType.colored(WidgetType.bodySmall, LocalWidgetSurface.current.onShellVariant),
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
