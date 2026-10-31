package org.slopfire.codexbar.widget

import android.content.Context
import android.os.Build
import androidx.compose.material3.ColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.text.FontWeight
import androidx.glance.text.TextStyle
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetBarHeight
import org.slopfire.codexbar.data.widget.WidgetColorMode
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.ui.theme.codexBarColorScheme

/**
 * Widget text styles, written out from the published type-scale tokens because
 * Glance has no Typography object.
 *
 * Sizes are the baseline tokens; the emphasized (heavier) variants are used only
 * on the hero number and on names, never as the whole scale.
 */
object WidgetType {

    /** label medium — 12sp, Medium. Chips and column captions. */
    val labelMedium = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Medium)

    /** label large — 14sp, Medium. Percentages in lists. */
    val labelLarge = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Medium)

    /** body small — 12sp, Regular. Supporting lines. */
    val bodySmall = TextStyle(fontSize = 12.sp, fontWeight = FontWeight.Normal)

    /** body medium — 14sp, Regular. Window names. */
    val bodyMedium = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Normal)

    /** title small — 14sp, Medium. Provider names. */
    val titleSmall = TextStyle(fontSize = 14.sp, fontWeight = FontWeight.Medium)

    /** title medium — 16sp, Medium. Message titles and tile percentages. */
    val titleMedium = TextStyle(fontSize = 16.sp, fontWeight = FontWeight.Medium)

    /** title large — 22sp, Regular. Compact numbers and spend values. */
    val titleLarge = TextStyle(fontSize = 22.sp, fontWeight = FontWeight.Normal)

    /** display small — 36sp. The hero number in a regular card. */
    val displaySmall = TextStyle(fontSize = 36.sp, fontWeight = FontWeight.Normal)

    /** display medium — 45sp. The hero number when the card has room to spare. */
    val displayMedium = TextStyle(fontSize = 45.sp, fontWeight = FontWeight.Normal)

    /** The same style carrying an explicit color, which Glance requires per run. */
    fun colored(style: TextStyle, color: ColorProvider): TextStyle = style.copy(color = color)
}

/**
 * The colour family a statement is painted in.
 *
 * From M3: status is carried by the scheme's own accent roles — primary for the
 * normal state, tertiary for "pay attention", error for "this will hurt" — so the
 * widget follows the wallpaper instead of a fixed traffic-light palette.
 */
enum class WidgetTone { Primary, Tertiary, Error, Neutral }

/** Tone for a remaining percentage: healthy, getting tight, or about to run out. */
fun toneForRemaining(percentLeft: Double?): WidgetTone = when {
    percentLeft == null || percentLeft.isNaN() -> WidgetTone.Neutral
    percentLeft >= 50.0 -> WidgetTone.Primary
    percentLeft >= 20.0 -> WidgetTone.Tertiary
    else -> WidgetTone.Error
}

/** Tone for a verdict's semantic tone. */
fun toneForVerdict(tone: VerdictTone): WidgetTone = when (tone) {
    VerdictTone.SUPPORTED -> WidgetTone.Primary
    VerdictTone.TERTIARY -> WidgetTone.Tertiary
    VerdictTone.ERROR -> WidgetTone.Error
    VerdictTone.MUTED -> WidgetTone.Neutral
}

/**
 * The more urgent of two tones. A provider at 60% that is pacing to run dry reads
 * as an error: the verdict states the outcome, the percentage only the budget.
 */
fun worseTone(a: WidgetTone, b: WidgetTone): WidgetTone {
    fun rank(tone: WidgetTone) = when (tone) {
        WidgetTone.Error -> 3
        WidgetTone.Tertiary -> 2
        WidgetTone.Primary -> 1
        WidgetTone.Neutral -> 0
    }
    return if (rank(a) >= rank(b)) a else b
}

/**
 * The colors a widget paints directly on its shell, resolved for one background.
 *
 * A Translucent or Clear shell lets the wallpaper through, and the secondary
 * text role is tuned for an opaque surface, so it leans toward the primary role
 * exactly when the shell is see-through.
 */
class WidgetSurface(
    /** Shell fill to draw, or null when the wallpaper shows through. */
    val fill: ColorProvider?,
    /** Primary text: titles and the numbers that matter. */
    val onShell: ColorProvider,
    /** Secondary text: captions and labels. */
    val onShellVariant: ColorProvider,
)

/**
 * The widget palette, resolved once per render from the same [codexBarColorScheme]
 * the app uses.
 *
 * Every role is a day/night pair. A pinned Light or Dark palette builds both halves
 * from the same scheme, so the system night mode cannot flip it.
 */
class WidgetPalette(
    private val day: ColorScheme,
    private val night: ColorScheme,
) {
    val colors: androidx.glance.color.ColorProviders =
        androidx.glance.material3.ColorProviders(day, night)

    private fun role(pick: (ColorScheme) -> Color): ColorProvider = dayNight(pick(day), pick(night))

    private fun mix(from: (ColorScheme) -> Color, to: (ColorScheme) -> Color, amount: Float) =
        dayNight(lerp(from(day), to(day), amount), lerp(from(night), to(night), amount))

    /**
     * The shell fill and the text colors that survive it.
     *
     * From M3: the widget body is a `surface container`. Translucent is the same
     * role at [WidgetTheme.TRANSLUCENT_ALPHA]; Clear draws no fill and lets the
     * opaque tonal cards float on the wallpaper.
     */
    fun surfaceFor(background: WidgetBackground): WidgetSurface {
        val opaque = background == WidgetBackground.Solid
        val emphasis = if (opaque) 0f else WidgetTheme.SECONDARY_TEXT_EMPHASIS
        val fill = when (background) {
            WidgetBackground.Solid -> role { it.surfaceContainer }
            WidgetBackground.Translucent -> role {
                it.surfaceContainer.copy(alpha = WidgetTheme.TRANSLUCENT_ALPHA)
            }
            WidgetBackground.Transparent -> null
        }
        return WidgetSurface(
            fill = fill,
            onShell = role { it.onSurface },
            onShellVariant = mix({ it.onSurfaceVariant }, { it.onSurface }, emphasis),
        )
    }

    /** Accent ink: progress fills and coloured numbers on a neutral surface. */
    fun accent(tone: WidgetTone): ColorProvider = when (tone) {
        WidgetTone.Primary -> role { it.primary }
        WidgetTone.Tertiary -> role { it.tertiary }
        WidgetTone.Error -> role { it.error }
        WidgetTone.Neutral -> role { it.onSurfaceVariant }
    }

    /** The tonal card a hero statement sits in. */
    fun container(tone: WidgetTone): ColorProvider = when (tone) {
        WidgetTone.Primary -> role { it.primaryContainer }
        WidgetTone.Tertiary -> role { it.tertiaryContainer }
        WidgetTone.Error -> role { it.errorContainer }
        WidgetTone.Neutral -> role { it.surfaceContainerHighest }
    }

    /** Text on [container]. */
    fun onContainer(tone: WidgetTone): ColorProvider = role { onContainerOf(it, tone) }

    /** Secondary text on [container]: the on-colour pulled a quarter toward the fill. */
    fun onContainerVariant(tone: WidgetTone): ColorProvider =
        mix({ onContainerOf(it, tone) }, { containerOf(it, tone) }, 0.28f)

    /** Progress track inside a [container]: the fill leaned slightly toward its ink. */
    fun containerTrack(tone: WidgetTone): ColorProvider =
        mix({ containerOf(it, tone) }, { onContainerOf(it, tone) }, 0.16f)

    /** Progress track directly on the shell (M3: `secondary container`). */
    val track: ColorProvider get() = role { it.secondaryContainer }

    /** A list tile on the shell, one step brighter than the shell itself. */
    val tile: ColorProvider get() = role { it.surfaceContainerHighest }

    /** The round plate behind a provider glyph and behind a verdict chip. */
    val plate: ColorProvider get() = role { it.surfaceContainerLowest }

    private fun accentOf(scheme: ColorScheme, tone: WidgetTone): Color = when (tone) {
        WidgetTone.Primary -> scheme.primary
        WidgetTone.Tertiary -> scheme.tertiary
        WidgetTone.Error -> scheme.error
        WidgetTone.Neutral -> scheme.onSurfaceVariant
    }

    private fun containerOf(scheme: ColorScheme, tone: WidgetTone): Color = when (tone) {
        WidgetTone.Primary -> scheme.primaryContainer
        WidgetTone.Tertiary -> scheme.tertiaryContainer
        WidgetTone.Error -> scheme.errorContainer
        WidgetTone.Neutral -> scheme.surfaceContainerHighest
    }

    private fun onContainerOf(scheme: ColorScheme, tone: WidgetTone): Color = when (tone) {
        WidgetTone.Primary -> scheme.onPrimaryContainer
        WidgetTone.Tertiary -> scheme.onTertiaryContainer
        WidgetTone.Error -> scheme.onErrorContainer
        WidgetTone.Neutral -> scheme.onSurface
    }
}

object WidgetTheme {

    /**
     * Scrim alpha for the `Translucent` background (not an M3 token): high enough
     * that the surface roles still describe what is behind the text, low enough
     * that the wallpaper is visibly there.
     */
    const val TRANSLUCENT_ALPHA = 0.82f

    /** How far secondary text leans toward primary on a see-through shell. */
    const val SECONDARY_TEXT_EMPHASIS = 0.35f

    /** Padding between the shell edge and its content. */
    val SHELL_PADDING = 12.dp

    fun palette(context: Context, mode: WidgetColorMode): WidgetPalette {
        val day = codexBarColorScheme(
            context,
            dark = mode == WidgetColorMode.Dark,
            dynamicColor = mode == WidgetColorMode.Dynamic,
        )
        // Light and Dark pin both halves to the same scheme so the widget ignores
        // the system appearance; Material You keeps the day/night pair.
        val night = when (mode) {
            WidgetColorMode.Dynamic -> codexBarColorScheme(context, dark = true)
            WidgetColorMode.Light, WidgetColorMode.Dark -> day
        }
        return WidgetPalette(day = day, night = night)
    }

    /**
     * Outer radius for the widget shell.
     *
     * System reads the launcher's own widget radius on Android 12+ so the widget
     * matches its neighbours; the fixed steps are the M3 shape scale.
     */
    fun shellRadius(context: Context, corners: WidgetCorners): Dp = when (corners) {
        WidgetCorners.System -> systemRadius(context)
        WidgetCorners.Medium -> 12.dp
        WidgetCorners.Large -> 16.dp
        WidgetCorners.ExtraLarge -> 28.dp
    }

    /**
     * Radius for a card inside the shell: concentric with the outer corner
     * (`inner = outer − padding`), but never so square it stops reading as a card.
     */
    fun innerRadius(outer: Dp): Dp = (outer - SHELL_PADDING).coerceIn(8.dp, 24.dp)

    private fun systemRadius(context: Context): Dp {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return 24.dp
        val resources = context.resources
        val px = resources.getDimension(android.R.dimen.system_app_widget_background_radius)
        return (px / resources.displayMetrics.density).dp
    }

    /**
     * Bar thickness. From M3 Expressive: 4dp default, 8dp thick; 12dp is a
     * home-screen extra.
     */
    fun barHeight(bar: WidgetBarHeight): Dp = when (bar) {
        WidgetBarHeight.Compact -> 4.dp
        WidgetBarHeight.Standard -> 8.dp
        WidgetBarHeight.Comfortable -> 12.dp
    }
}

private fun dayNight(day: Color, night: Color): ColorProvider =
    androidx.glance.color.ColorProvider(day = day, night = night)
