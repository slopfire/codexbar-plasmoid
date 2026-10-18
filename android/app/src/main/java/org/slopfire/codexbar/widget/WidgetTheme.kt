package org.slopfire.codexbar.widget

import android.content.Context
import androidx.compose.material3.ColorScheme
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetColorMode
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.theme.codexBarColorScheme
import androidx.glance.text.FontWeight
import androidx.glance.text.TextStyle

/**
 * Widget text styles, written out from the published type-scale tokens
 * (`references/tokens.md`) because Glance has no Typography object.
 *
 * Sizes and line heights are the baseline tokens; the emphasized variants are
 * used only where M3 says they belong — the headline number and selection.
 */
object WidgetType {

    /** label small — 11sp/16sp/0.5sp, Bold (emphasized). Metadata captions. */
    val labelSmall = TextStyle(
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
    )

    /** label medium — 12sp/16sp/0.5sp, Bold (emphasized). Badges and chips. */
    val labelMedium = TextStyle(
        fontSize = 12.sp,
        fontWeight = FontWeight.Bold,
    )

    /** title small — 14sp/20sp/0.1sp, Bold (emphasized). Row titles. */
    val titleSmall = TextStyle(
        fontSize = 14.sp,
        fontWeight = FontWeight.Bold,
    )

    /** title medium — 16sp/24sp/0.15sp, Bold (emphasized). Card titles. */
    val titleMedium = TextStyle(
        fontSize = 16.sp,
        fontWeight = FontWeight.Bold,
    )

    /** title large — 22sp/28sp/0sp, Medium (emphasized). The one big number. */
    val titleLarge = TextStyle(
        fontSize = 22.sp,
        fontWeight = FontWeight.Medium,
    )

    /** body small — 12sp/16sp/0.4sp, Regular. Baseline, for supporting prose. */
    val bodySmall = TextStyle(
        fontSize = 12.sp,
        fontWeight = FontWeight.Normal,
    )

    /** The same style carrying an explicit color, which Glance requires per run. */
    fun colored(style: TextStyle, color: ColorProvider): TextStyle = style.copy(color = color)
}

/**
 * The colors a widget paints its **text** in, resolved for one shell.
 *
 * A widget has no opaque backdrop guarantee: the user can pick a Translucent or
 * Transparent fill, and then the wallpaper shows through. The generic theme roles
 * are tuned for an opaque surface, so `onSurfaceVariant` in particular drops below
 * a comfortable contrast once the shell is see-through. [WidgetPalette.surfaceFor]
 * resolves the pair per render and leans the secondary role toward the primary one
 * exactly when the shell is not opaque.
 */
class WidgetSurface(
    /** Shell fill to draw, or null when the launcher/wallpaper owns the backdrop. */
    val fill: ColorProvider?,
    /** Primary text: titles and the numbers that matter. */
    val onShell: ColorProvider,
    /** Secondary text: captions and labels. */
    val onShellVariant: ColorProvider,
)

/**
 * The widget palette, resolved once per render.
 *
 * A widget has no Compose theme to inherit, so the palette is built from the same
 * [codexBarColorScheme] the app uses — and can be pinned to light or dark when the
 * user does not want it following the system.
 *
 * Glance's Material 3 provider exposes the baseline role set but not the
 * Expressive `surface container` steps, so those are supplied explicitly from the
 * real ColorScheme rather than substituted with a wrong role. The two schemes are
 * kept so [surfaceFor] and [healthFor] can derive day/night pairs on demand.
 */
class WidgetPalette(
    val colors: androidx.glance.color.ColorProviders,
    val shell: ColorProvider,
    val container: ColorProvider,
    val track: ColorProvider,
    private val day: ColorScheme,
    private val night: ColorScheme,
) {
    /**
     * The shell fill and the text colors that survive it.
     *
     * From M3: containment uses a `surface container` role. My design decision
     * (not in M3): translucent is the same role at [WidgetTheme.TRANSLUCENT_ALPHA]
     * so the wallpaper breathes through — but still an opaque-*enough* scrim that
     * text keeps its contrast, which is the role M3 gives a scrim over imagery.
     * Transparent draws no fill at all and leaves the launcher's own widget
     * background in charge.
     */
    fun surfaceFor(background: WidgetBackground): WidgetSurface {
        val opaque = background == WidgetBackground.Solid
        // The weaker secondary role is what fails first on a see-through shell, so
        // it is the only one that moves; the primary role is already at full strength.
        val emphasis = if (opaque) 0f else WidgetTheme.SECONDARY_TEXT_EMPHASIS
        return WidgetSurface(
            fill = shellFor(background),
            onShell = dayNight(day.onSurface, night.onSurface),
            onShellVariant = dayNight(
                lerp(day.onSurfaceVariant, day.onSurface, emphasis),
                lerp(night.onSurfaceVariant, night.onSurface, emphasis),
            ),
        )
    }

    private fun shellFor(background: WidgetBackground): ColorProvider? = when (background) {
        WidgetBackground.Solid -> shell
        WidgetBackground.Translucent -> dayNight(
            day.surfaceContainerHigh.copy(alpha = WidgetTheme.TRANSLUCENT_ALPHA),
            night.surfaceContainerHigh.copy(alpha = WidgetTheme.TRANSLUCENT_ALPHA),
        )
        WidgetBackground.Transparent -> null
    }

    /**
     * The remaining-limit health scale as a day/night pair.
     *
     * The app can paint these on an opaque `surface container`, so one value works
     * there. A widget cannot assume that, so each semantic step gets a light and a
     * dark value with the same meaning and enough contrast against its own shell.
     * The shared scale's Neutral is a mid-gray that all but disappears on a
     * translucent dark widget, and it is the color of the caption under every bar.
     */
    fun healthFor(color: Color): ColorProvider = when (color) {
        Health.Good -> dayNight(Color(0xFF1E6B4C), Color(0xFF86DDB4))
        Health.Tight -> dayNight(Color(0xFF8A6100), Color(0xFFFFCF70))
        Health.Low -> dayNight(Color(0xFFA8431F), Color(0xFFFFAD84))
        Health.Critical -> dayNight(Color(0xFFB3261E), Color(0xFFFFB4AB))
        Health.Neutral -> dayNight(Color(0xFF49555A), Color(0xFFC3CDCF))
        else -> dayNight(color, color)
    }
}

object WidgetTheme {

    /**
     * Scrim alpha for the `Translucent` background style (not an M3 token).
     *
     * M3's answer for text over imagery is a scrim, and this is that scrim: high
     * enough that the surface roles still describe what is behind the text, low
     * enough that the wallpaper is still visibly there. The previous 0.55 let the
     * wallpaper dominate and pushed secondary text under a readable contrast.
     */
    const val TRANSLUCENT_ALPHA = 0.82f

    /**
     * How far the secondary text role leans toward the primary one on a
     * see-through shell (not an M3 token — a contrast correction).
     */
    const val SECONDARY_TEXT_EMPHASIS = 0.35f

    fun palette(context: Context, mode: WidgetColorMode): WidgetPalette {
        val day = codexBarColorScheme(
            context,
            dark = mode == WidgetColorMode.Dark,
            dynamicColor = mode == WidgetColorMode.Dynamic,
        )
        // Light and Dark pin both day and night to the same scheme so the widget
        // ignores the system appearance; Dynamic keeps the day/night pair.
        val night = when (mode) {
            WidgetColorMode.Dynamic -> codexBarColorScheme(context, dark = true)
            WidgetColorMode.Light -> codexBarColorScheme(context, dark = false, dynamicColor = false)
            WidgetColorMode.Dark -> codexBarColorScheme(context, dark = true, dynamicColor = false)
        }
        return WidgetPalette(
            colors = androidx.glance.material3.ColorProviders(day, night),
            shell = dayNight(day.surfaceContainerHigh, night.surfaceContainerHigh),
            container = dayNight(day.surfaceContainer, night.surfaceContainer),
            track = dayNight(day.surfaceVariant, night.surfaceVariant),
            day = day,
            night = night,
        )
    }

    /**
     * Outer radius for the widget shell.
     *
     * From M3 shape scale: Medium 12dp, Large 16dp, ExtraLarge 28dp. Null means
     * System — no override, the launcher's `appWidgetBackground` owns the radius.
     */
    fun shellRadius(corners: org.slopfire.codexbar.data.widget.WidgetCorners): Dp? =
        when (corners) {
            org.slopfire.codexbar.data.widget.WidgetCorners.System -> null
            org.slopfire.codexbar.data.widget.WidgetCorners.Medium -> 12.dp
            org.slopfire.codexbar.data.widget.WidgetCorners.Large -> 16.dp
            org.slopfire.codexbar.data.widget.WidgetCorners.ExtraLarge -> 28.dp
        }

    /**
     * Bar thickness.
     *
     * From M3: linear progress default thickness 4dp, configurable in
     * Expressive. My design decision (not in M3): 6/8/12dp so the pacing marker
     * stays legible at widget sizes.
     */
    fun barHeight(bar: org.slopfire.codexbar.data.widget.WidgetBarHeight): Dp =
        when (bar) {
            org.slopfire.codexbar.data.widget.WidgetBarHeight.Compact -> 6.dp
            org.slopfire.codexbar.data.widget.WidgetBarHeight.Standard -> 8.dp
            org.slopfire.codexbar.data.widget.WidgetBarHeight.Comfortable -> 12.dp
        }
}

private fun dayNight(day: Color, night: Color): ColorProvider =
    androidx.glance.color.ColorProvider(day = day, night = night)
