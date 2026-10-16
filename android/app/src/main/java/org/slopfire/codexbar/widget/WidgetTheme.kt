package org.slopfire.codexbar.widget

import android.content.Context
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.glance.unit.ColorProvider
import org.slopfire.codexbar.data.widget.WidgetColorMode
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
 * The widget palette, resolved once per render.
 *
 * A widget has no Compose theme to inherit, so the palette is built from the same
 * [codexBarColorScheme] the app uses — and can be pinned to light or dark when the
 * user does not want it following the system.
 *
 * Glance's Material 3 provider exposes the baseline role set but not the
 * Expressive `surface container` steps, so those are supplied explicitly from the
 * real ColorScheme rather than substituted with a wrong role.
 */
class WidgetPalette(
    val colors: androidx.glance.color.ColorProviders,
    val shell: ColorProvider,
    val container: ColorProvider,
    val track: ColorProvider,
    private val shellDay: Color,
    private val shellNight: Color,
) {
    /**
     * Shell fill for a background style.
     *
     * From M3: containment uses a `surface container` role. My design decision
     * (not in M3): translucent is the same role at 55% alpha so the wallpaper
     * breathes through; transparent leaves the launcher's own widget background
     * in charge. Returns null when no fill should be drawn.
     */
    fun shellFor(background: org.slopfire.codexbar.data.widget.WidgetBackground): ColorProvider? =
        when (background) {
            org.slopfire.codexbar.data.widget.WidgetBackground.Solid -> shell
            org.slopfire.codexbar.data.widget.WidgetBackground.Translucent -> dayNight(
                shellDay.copy(alpha = 0.55f),
                shellNight.copy(alpha = 0.55f),
            )
            org.slopfire.codexbar.data.widget.WidgetBackground.Transparent -> null
        }
}

object WidgetTheme {

    /** Translucency for the `Translucent` background style (not an M3 token). */
    const val TRANSLUCENT_ALPHA = 0.55f

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
            shellDay = day.surfaceContainerHigh,
            shellNight = night.surfaceContainerHigh,
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
