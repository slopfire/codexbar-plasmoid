package org.slopfire.codexbar.widget

import android.content.Intent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.glance.ColorFilter
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.Image
import androidx.glance.ImageProvider
import androidx.glance.LocalContext
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.action.actionStartActivity
import androidx.glance.appwidget.appWidgetBackground
import androidx.glance.appwidget.cornerRadius
import androidx.glance.background
import androidx.glance.layout.Alignment
import androidx.glance.layout.Box
import androidx.glance.layout.Column
import androidx.glance.layout.ColumnScope
import androidx.glance.layout.ContentScale
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.size
import androidx.glance.layout.width
import androidx.glance.text.Text
import androidx.glance.text.TextAlign
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.MainActivity
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.theme.providerAccentPair
import org.slopfire.codexbar.ui.theme.providerIconRes
import org.slopfire.codexbar.ui.theme.providerLabel
import org.slopfire.codexbar.ui.theme.providerMonogram

/**
 * Shared widget building blocks.
 *
 * Design rules (M3 / Material You):
 * - **Tonal containment**: the shell is a `surface container`; the statement that
 *   matters sits in a tonal card (`primary` / `tertiary` / `error container`) whose
 *   colour *is* the status, so no traffic-light ink is needed on top of it.
 * - **Concentric shape**: cards use `outer radius − shell padding`, so they nest
 *   inside the launcher's widget corner instead of fighting it.
 * - **Expressive progress**: a thick rounded indicator, a gap, the track, and a stop
 *   dot at the end — the M3 Expressive linear progress anatomy.
 * - **One number**: the hero percentage is the only display-size text.
 */

/** The palette for the widget currently being rendered. */
internal val LocalWidgetPalette = staticCompositionLocalOf<WidgetPalette> {
    error("WidgetPalette was not provided")
}

/** The shell's resolved fill and text colors, provided by [WidgetShell]. */
internal val LocalWidgetSurface = staticCompositionLocalOf<WidgetSurface> {
    error("WidgetSurface was not provided")
}

/** Radius for cards and tiles inside the shell, provided by [WidgetShell]. */
internal val LocalCardRadius = staticCompositionLocalOf { 16.dp }

/** Horizontal padding inside a card or tile. */
private val CARD_PADDING = 12.dp

/** Space between the hero card and the first tile, and between tiles. */
private val TILE_GAP = 6.dp

/** Narrowest a tile keeps its provider name (about "OpenC…") before the pairs give way. */
private const val TILE_NAME_MIN = 44f

/** Gap between the progress indicator and its track (M3 Expressive). */
private val INDICATOR_GAP = 4.dp

/** The stop indicator at the end of the track (M3 Expressive: 4dp). */
private val STOP_DOT = 4.dp

/** Time-marker tick width. */
private val MARKER_WIDTH = 2.dp

/** Window-name column inside the hero card. */
private val WINDOW_LABEL_WIDTH = 72.dp

/** Percent column inside the hero card: `100%` at label large. */
private val WINDOW_VALUE_WIDTH = 44.dp

/** Space around a window row's bar. */
private val WINDOW_GAP = 8.dp

/** Windows a provider tile lists. */
private const val MAX_TILE_WINDOWS = 3

/** Theme, palette and shell for one widget render — the frame every widget shares. */
@Composable
fun WidgetFrame(
    settings: WidgetSettings,
    content: @Composable ColumnScope.() -> Unit,
) {
    val context = LocalContext.current
    val palette = remember(settings.colorMode) { WidgetTheme.palette(context, settings.colorMode) }
    CompositionLocalProvider(LocalWidgetPalette provides palette) {
        GlanceTheme(colors = palette.colors) {
            WidgetShell(settings = settings, content = content)
        }
    }
}

/**
 * The widget shell: the launcher's app-widget background, the configured fill and
 * radius, and a tap target that opens the app.
 */
@Composable
private fun WidgetShell(
    settings: WidgetSettings,
    content: @Composable ColumnScope.() -> Unit,
) {
    val context = LocalContext.current
    val surface = LocalWidgetPalette.current.surfaceFor(settings.background)
    val outer = WidgetTheme.shellRadius(context, settings.corners)
    var modifier = GlanceModifier
        .fillMaxSize()
        .appWidgetBackground()
        .cornerRadius(outer)
    surface.fill?.let { modifier = modifier.background(it) }
    modifier = modifier
        .padding(WidgetTheme.SHELL_PADDING)
        .clickable(actionStartActivity(Intent(context, MainActivity::class.java)))
    CompositionLocalProvider(
        LocalWidgetSurface provides surface,
        LocalCardRadius provides WidgetTheme.innerRadius(outer),
    ) {
        Column(modifier = modifier, content = content)
    }
}

/** The shell's content height in dp, or NaN when the host did not report a size. */
@Composable
fun widgetInnerHeight(): Float {
    val height = LocalSize.current.height
    if (height.value.isNaN() || height <= 0.dp) return Float.NaN
    return (height - WidgetTheme.SHELL_PADDING * 2).value.coerceAtLeast(0f)
}

/** The shell's content width, or null when the host did not report a size. */
@Composable
fun widgetInnerWidth(): Dp? {
    val width = LocalSize.current.width
    if (width.value.isNaN() || width <= 0.dp) return null
    return (width - WidgetTheme.SHELL_PADDING * 2).coerceAtLeast(0.dp)
}

/**
 * A provider badge: the bundled brand glyph tinted with the provider's accent, on
 * a round plate. Falls back to initials when no icon is bundled for the id.
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
            .background(LocalWidgetPalette.current.plate),
        contentAlignment = Alignment.Center,
    ) {
        if (iconRes == 0) {
            Text(
                text = providerMonogram(provider),
                style = WidgetType.colored(WidgetType.labelMedium, accent),
            )
        } else {
            Image(
                provider = ImageProvider(iconRes),
                contentDescription = providerLabel(provider),
                modifier = GlanceModifier.size(size * 0.6f),
                contentScale = ContentScale.Fit,
                colorFilter = ColorFilter.tint(accent),
            )
        }
    }
}

/** Display options shared by the usage widgets, read once from the settings. */
data class UsageStyle(
    val showBadge: Boolean,
    val showReset: Boolean,
    val showMarker: Boolean,
    val barHeight: Dp,
) {
    companion object {
        fun from(settings: WidgetSettings) = UsageStyle(
            showBadge = settings.showBadges,
            showReset = settings.showCaptions,
            showMarker = settings.showTimeMarker,
            barHeight = WidgetTheme.barHeight(settings.barHeight),
        )
    }
}

/** The card tone for a provider: its budget or its verdict, whichever is worse. */
fun heroTone(provider: WidgetProvider): WidgetTone = worseTone(
    toneForRemaining(provider.tightest?.percentLeft),
    toneForVerdict(WidgetVerdict.forProvider(provider).tone),
)

/**
 * The hero card: one provider's tightest window as a tonal card — badge, name and
 * verdict; the remaining percentage as the one big number; what it measures and
 * when it resets; the bar; and the provider's other windows underneath.
 *
 * When [fill] is set the caller has given the card the spare height (via
 * `defaultWeight`), and the number drops to the bottom of it so the card reads
 * top-to-bottom as name → number → evidence.
 */
@Composable
fun HeroCard(
    provider: WidgetProvider,
    style: UsageStyle,
    plan: HeroPlan,
    modifier: GlanceModifier = GlanceModifier,
    fill: Boolean = false,
) {
    val palette = LocalWidgetPalette.current
    val row = provider.tightest
    val tone = heroTone(provider)
    val ink = palette.onContainer(tone)
    val inkVariant = palette.onContainerVariant(tone)
    val contentWidth = widgetInnerWidth()?.minus(CARD_PADDING * 2)?.coerceAtLeast(0.dp)

    Column(
        modifier = modifier
            .fillMaxWidth()
            .cornerRadius(LocalCardRadius.current)
            .background(palette.container(tone))
            .padding(horizontal = CARD_PADDING, vertical = 10.dp),
    ) {
        Row(
            modifier = GlanceModifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (style.showBadge) {
                ProviderBadge(provider.id)
                Spacer(modifier = GlanceModifier.width(8.dp))
            }
            Text(
                text = providerLabel(provider.id),
                style = WidgetType.colored(WidgetType.titleSmall, ink),
                maxLines = 1,
                modifier = GlanceModifier.defaultWeight(),
            )
            Spacer(modifier = GlanceModifier.width(8.dp))
            VerdictChip(WidgetVerdict.forProvider(provider))
        }
        if (fill) {
            Spacer(modifier = GlanceModifier.defaultWeight())
        } else {
            Spacer(modifier = GlanceModifier.height(2.dp))
        }
        Row(verticalAlignment = Alignment.Bottom) {
            Text(
                text = Fmt.percent(row?.percentLeft),
                style = WidgetType.colored(
                    if (plan.bigNumber) WidgetType.displayMedium else WidgetType.displaySmall,
                    ink,
                ),
                maxLines = 1,
            )
            Spacer(modifier = GlanceModifier.width(6.dp))
            Text(
                text = "left",
                style = WidgetType.colored(WidgetType.titleMedium, inkVariant),
                maxLines = 1,
                // Glance has no baseline alignment; lifting the unit approximates it.
                modifier = GlanceModifier.padding(bottom = if (plan.bigNumber) 9.dp else 7.dp),
            )
        }
        if (plan.showMeta) {
            Spacer(modifier = GlanceModifier.height(2.dp))
            Text(
                text = heroMeta(row, provider, style.showReset, contentWidth),
                style = WidgetType.colored(WidgetType.bodySmall, inkVariant),
                maxLines = 1,
            )
        }
        Spacer(modifier = GlanceModifier.height(WidgetLayout.BAR_GAP.dp))
        ExpressiveBar(
            percentLeft = row?.percentLeft,
            markerFraction = markerFor(row, style.showMarker),
            width = contentWidth,
            height = style.barHeight,
            active = palette.accent(tone),
            track = palette.containerTrack(tone),
            marker = ink,
        )
        // One child for the whole list: Glance drops a Row/Column's children
        // past the tenth, and the card already has seven.
        val siblings = siblingsOf(provider).take(plan.siblings)
        if (siblings.isNotEmpty()) {
            Column(modifier = GlanceModifier.fillMaxWidth()) {
                siblings.forEach { sibling ->
                    WindowRow(
                        row = sibling,
                        style = style,
                        contentWidth = contentWidth,
                        labelInk = inkVariant,
                        valueInk = ink,
                        track = palette.containerTrack(tone),
                        marker = ink,
                        modifier = GlanceModifier.padding(top = 8.dp),
                    )
                }
            }
        }
    }
}

/** The hero provider's other windows, tightest first, without the headline's own. */
fun siblingsOf(provider: WidgetProvider): List<UsageRow> =
    provider.rows.sortedBy { it.percentLeft }.drop(1)

/** `Session · resets in 2 h 16 m`, or the provider's degraded state. */
@Composable
private fun heroMeta(
    row: UsageRow?,
    provider: WidgetProvider,
    showReset: Boolean,
    width: Dp?,
): String {
    val fontScale = LocalContext.current.resources.configuration.fontScale
    val maxChars = width?.let { (it.value / (WidgetLayout.SMALL_CHAR * fontScale)).toInt() }
        ?: Int.MAX_VALUE
    val text = when {
        row == null && provider.hasError -> "last refresh failed"
        row == null && provider.stale -> "showing last good data"
        row == null -> "no limits reported"
        showReset -> WidgetVerdict.headlineMeta(row, maxChars = maxChars)
        else -> WidgetVerdict.windowLabel(row)
    }
    return text.replaceFirstChar { it.uppercase() }
}

private fun markerFor(row: UsageRow?, show: Boolean): Double? =
    if (!show || row == null) null else WidgetPace.timeLeftFraction(row.resetsAt, row.windowMinutes)

/** One window of a provider: its name, its bar, and what is left of it. */
@Composable
private fun WindowRow(
    row: UsageRow,
    style: UsageStyle,
    contentWidth: Dp?,
    labelInk: ColorProvider,
    valueInk: ColorProvider,
    track: ColorProvider,
    marker: ColorProvider,
    modifier: GlanceModifier = GlanceModifier,
) {
    val palette = LocalWidgetPalette.current
    val barWidth = contentWidth
        ?.minus(WINDOW_LABEL_WIDTH + WINDOW_VALUE_WIDTH + WINDOW_GAP * 2)
        ?.coerceAtLeast(0.dp)
    Row(
        modifier = modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text = WidgetVerdict.windowLabel(row).replaceFirstChar { it.uppercase() },
            style = WidgetType.colored(WidgetType.bodySmall, labelInk),
            maxLines = 1,
            modifier = GlanceModifier.width(WINDOW_LABEL_WIDTH),
        )
        Spacer(modifier = GlanceModifier.width(WINDOW_GAP))
        ExpressiveBar(
            percentLeft = row.percentLeft,
            markerFraction = markerFor(row, style.showMarker),
            width = barWidth,
            height = style.barHeight.coerceAtMost(6.dp),
            active = palette.accent(toneForRemaining(row.percentLeft)),
            track = track,
            marker = marker,
            modifier = if (barWidth != null) GlanceModifier else GlanceModifier.defaultWeight(),
        )
        Spacer(modifier = GlanceModifier.width(WINDOW_GAP))
        Text(
            text = Fmt.percent(row.percentLeft),
            style = WidgetType.colored(WidgetType.labelLarge, valueInk)
                .copy(textAlign = TextAlign.End),
            maxLines = 1,
            modifier = GlanceModifier.width(WINDOW_VALUE_WIDTH),
        )
    }
}

/** A provider's windows as a tile lists them: tightest first, at most three. */
fun tileWindowsOf(provider: WidgetProvider): List<UsageRow> =
    provider.rows.sortedBy { it.percentLeft }.take(MAX_TILE_WINDOWS)

/**
 * A provider the widget is not leading with, as one tile that names every limit.
 *
 * Compact, it is one line: badge, name, then each window as `5h 80%  wk 55%` —
 * short enough to show all of them, and each number says which limit it is.
 * Expanded (the widget has height to spare), each window gets its own labelled row
 * with a bar, the same anatomy as the hero card's list.
 *
 * A warning verdict ("dry in 1h") is shown when there is room for it, and always
 * colours the tightest number.
 */
@Composable
fun ProviderTile(provider: WidgetProvider, style: UsageStyle, expanded: Boolean = false) {
    val palette = LocalWidgetPalette.current
    val surface = LocalWidgetSurface.current
    val windows = tileWindowsOf(provider)
    val verdict = WidgetVerdict.forProvider(provider)
    val verdictTone = toneForVerdict(verdict.tone)
    val warn = verdictTone == WidgetTone.Error || verdictTone == WidgetTone.Tertiary
    // Expanded tiles have a whole header for the verdict. A compact line keeps
    // its room for what is left; the verdict only tints the first number there.
    val showVerdict = windows.isEmpty() || (warn && expanded)
    // The gap is padding on a wrapper rather than a sibling Spacer, so each tile
    // is one child of the widget's column (Glance keeps at most ten).
    Column(modifier = GlanceModifier.fillMaxWidth().padding(top = TILE_GAP)) {
        Column(
            modifier = GlanceModifier
                .fillMaxWidth()
                .cornerRadius(LocalCardRadius.current.coerceAtMost(16.dp))
                .background(palette.tile)
                .padding(horizontal = 8.dp, vertical = 8.dp),
        ) {
            Row(
                modifier = GlanceModifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (style.showBadge) {
                    ProviderBadge(provider.id)
                    Spacer(modifier = GlanceModifier.width(10.dp))
                } else {
                    Spacer(modifier = GlanceModifier.width(4.dp))
                }
                Text(
                    text = providerLabel(provider.id),
                    style = WidgetType.colored(WidgetType.titleSmall, surface.onShell),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
                if (showVerdict) {
                    Spacer(modifier = GlanceModifier.width(8.dp))
                    Text(
                        text = verdict.text,
                        style = WidgetType.colored(
                            WidgetType.bodySmall,
                            if (warn) palette.accent(verdictTone) else surface.onShellVariant,
                        ),
                        maxLines = 1,
                    )
                }
                if (!expanded) {
                    // Badge, the shortest useful name and the tile's own padding come
                    // out of the row first; the pairs get the rest, and the name
                    // ellipsizes into whatever they leave.
                    val context = LocalContext.current
                    val fixed = (if (style.showBadge) 34f else 4f) + TILE_NAME_MIN + 20f
                    val available = widgetInnerWidth()?.value?.minus(fixed) ?: 0f
                    val values = windows.map { Fmt.percent(it.percentLeft) }
                    val (labels, shown) = WidgetLayout.fitPairs(
                        available = available,
                        labels = WidgetVerdict.shortWindowLabels(windows),
                        values = values,
                        fontScale = context.resources.configuration.fontScale,
                    )
                    // Each pair is its own Row so the header stays under Glance's
                    // ten-children limit however many windows there are.
                    windows.take(shown).forEachIndexed { index, row ->
                        Row(
                            modifier = GlanceModifier.padding(start = if (index == 0) 10.dp else 12.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text(
                                text = labels[index],
                                style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant),
                                maxLines = 1,
                            )
                            Spacer(modifier = GlanceModifier.width(4.dp))
                            Text(
                                text = values[index],
                                style = WidgetType.colored(
                                    WidgetType.labelLarge,
                                    palette.accent(
                                        if (index == 0 && warn) {
                                            worseTone(toneForRemaining(row.percentLeft), verdictTone)
                                        } else {
                                            toneForRemaining(row.percentLeft)
                                        },
                                    ),
                                ),
                                maxLines = 1,
                            )
                        }
                    }
                    if (shown < windows.size) {
                        // The rest exist; say so rather than pretend there are none.
                        Text(
                            text = "+${windows.size - shown}",
                            style = WidgetType.colored(WidgetType.labelLarge, surface.onShellVariant),
                            maxLines = 1,
                            modifier = GlanceModifier.padding(start = 10.dp),
                        )
                    }
                }
                Spacer(modifier = GlanceModifier.width(4.dp))
            }
            if (expanded) {
                // Tile padding (8 each side) plus the 4dp inset on each side of the rows.
                val contentWidth = widgetInnerWidth()?.minus(24.dp)?.coerceAtLeast(0.dp)
                Column(modifier = GlanceModifier.fillMaxWidth().padding(horizontal = 4.dp)) {
                    windows.forEach { row ->
                        WindowRow(
                            row = row,
                            style = style,
                            contentWidth = contentWidth,
                            labelInk = surface.onShellVariant,
                            valueInk = surface.onShell,
                            track = palette.track,
                            marker = surface.onShell,
                            modifier = GlanceModifier.padding(top = 6.dp),
                        )
                    }
                }
            }
        }
    }
}

/**
 * The verdict, as a small chip on a light plate. The plate stays neutral so the
 * chip reads on every card tone; the ink carries the verdict's own tone.
 */
@Composable
fun VerdictChip(verdict: Verdict) {
    val palette = LocalWidgetPalette.current
    Box(
        modifier = GlanceModifier
            .cornerRadius(10.dp)
            .background(palette.plate)
            .padding(horizontal = 8.dp, vertical = 3.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = verdict.text,
            style = WidgetType.colored(WidgetType.labelMedium, palette.accent(toneForVerdict(verdict.tone))),
            maxLines = 1,
        )
    }
}

/**
 * The M3 Expressive linear progress indicator: rounded active indicator, a gap,
 * the rounded track, a stop dot at the track's end, and an optional time-marker
 * tick where the clock sits within the window.
 *
 * RemoteViews has no fractional widths, so every segment is a hard dp width taken
 * from the measured [width] (`SizeMode.Exact`). Without a width there is nothing
 * to divide, so only the track is drawn.
 */
@Composable
fun ExpressiveBar(
    percentLeft: Double?,
    markerFraction: Double?,
    width: Dp?,
    height: Dp,
    active: ColorProvider,
    track: ColorProvider,
    marker: ColorProvider,
    modifier: GlanceModifier = GlanceModifier,
) {
    val rowHeight = if (markerFraction != null) height + WidgetLayout.MARKER.dp else height
    if (width == null) {
        Box(
            modifier = modifier
                .fillMaxWidth()
                .height(height)
                .cornerRadius(height / 2)
                .background(track),
        ) {}
        return
    }
    if (width <= 0.dp) {
        Spacer(modifier = modifier.height(rowHeight))
        return
    }
    val raw = (percentLeft ?: 0.0) / 100.0
    val fraction = (if (raw.isNaN()) 0.0 else raw).coerceIn(0.0, 1.0)
    // A sliver below the bar's own height would draw as a squashed oval; a dot is
    // the honest minimum for "something is left".
    val activeWidth = if (fraction > 0.0) (width * fraction.toFloat()).coerceAtLeast(height) else 0.dp
    val gap = if (fraction > 0.0) INDICATOR_GAP else 0.dp
    val trackWidth = width - activeWidth - gap
    val showTrack = trackWidth >= height

    Box(
        modifier = modifier.width(width).height(rowHeight),
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(
            modifier = GlanceModifier.width(width).height(rowHeight),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (fraction > 0.0) {
                Box(
                    modifier = GlanceModifier
                        .width(if (showTrack) activeWidth else width)
                        .height(height)
                        .cornerRadius(height / 2)
                        .background(active),
                ) {}
            }
            if (showTrack) {
                Spacer(modifier = GlanceModifier.width(gap))
                Box(
                    modifier = GlanceModifier
                        .width(trackWidth)
                        .height(height)
                        .cornerRadius(height / 2)
                        .background(track),
                ) {}
            }
        }
        if (showTrack) {
            // The stop sits centred in the track's end cap: it marks where the
            // window's budget ends even when little of it is left.
            val dot = STOP_DOT.coerceAtMost(height)
            Row(
                modifier = GlanceModifier.width(width).height(rowHeight),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Spacer(modifier = GlanceModifier.width(width - (height + dot) / 2))
                Box(
                    modifier = GlanceModifier
                        .size(dot)
                        .cornerRadius(dot / 2)
                        .background(active),
                ) {}
            }
        }
        if (markerFraction != null) {
            val x = width * markerFraction.coerceIn(0.0, 1.0).toFloat()
            Row(
                modifier = GlanceModifier.width(width).height(rowHeight),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Spacer(
                    modifier = GlanceModifier.width(
                        (x - MARKER_WIDTH / 2).coerceIn(0.dp, width - MARKER_WIDTH),
                    ),
                )
                Box(
                    modifier = GlanceModifier
                        .width(MARKER_WIDTH)
                        .height(rowHeight)
                        .cornerRadius(MARKER_WIDTH / 2)
                        .background(marker),
                ) {}
            }
        }
    }
}

/**
 * A spend figure as a tonal tile: label, value, optional token count.
 */
@Composable
fun MetricTile(
    label: String,
    value: String,
    caption: String?,
    tone: WidgetTone,
    modifier: GlanceModifier = GlanceModifier,
) {
    val palette = LocalWidgetPalette.current
    Column(
        modifier = modifier
            .cornerRadius(LocalCardRadius.current)
            .background(palette.container(tone))
            .padding(CARD_PADDING),
    ) {
        Text(
            text = label,
            style = WidgetType.colored(WidgetType.labelMedium, palette.onContainerVariant(tone)),
            maxLines = 1,
        )
        Spacer(modifier = GlanceModifier.height(2.dp))
        Text(
            text = value,
            style = WidgetType.colored(WidgetType.titleLarge, palette.onContainer(tone)),
            maxLines = 1,
        )
        if (caption != null) {
            Text(
                text = caption,
                style = WidgetType.colored(WidgetType.bodySmall, palette.onContainerVariant(tone)),
                maxLines = 1,
            )
        }
    }
}

/** Centered message used for the loading, empty and error states. */
@Composable
fun WidgetMessage(title: String, detail: String) {
    val surface = LocalWidgetSurface.current
    Column(
        modifier = GlanceModifier.fillMaxSize(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = title,
            style = WidgetType.colored(WidgetType.titleMedium, surface.onShell)
                .copy(textAlign = TextAlign.Center),
            maxLines = 1,
        )
        Spacer(modifier = GlanceModifier.height(4.dp))
        Text(
            text = detail,
            style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant)
                .copy(textAlign = TextAlign.Center),
            maxLines = 2,
        )
    }
}
