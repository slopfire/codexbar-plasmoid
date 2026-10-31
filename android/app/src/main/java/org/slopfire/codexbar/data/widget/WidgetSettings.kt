package org.slopfire.codexbar.data.widget

import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.core.stringSetPreferencesKey

/** Which palette a widget renders with. */
enum class WidgetColorMode(val label: String) {
    /** Wallpaper-derived Material You colours (Android 12+). */
    Dynamic("Material You"),
    Light("Light"),
    Dark("Dark"),
}

enum class WidgetSort(val label: String) {
    Tightest("Tightest"),
    Name("A–Z"),
}

/**
 * Widget surface fill.
 *
 * From M3: widget bodies use a `surface container` role for containment.
 * My design decision (not in M3): the translucency level and the clear
 * option, which let the wallpaper show through between the tonal cards.
 */
enum class WidgetBackground(val label: String) {
    Solid("Solid"),
    Translucent("Translucent"),
    Transparent("Clear"),
}

/**
 * Outer corner radius for the widget shell.
 *
 * System follows the launcher's own widget radius
 * (`system_app_widget_background_radius` on Android 12+), which is what makes a
 * widget sit flush with the rest of the home screen. The fixed steps are the
 * M3 shape scale: Medium 12dp, Large 16dp, Extra large 28dp.
 */
enum class WidgetCorners(val label: String) {
    System("Launcher"),
    Medium("12 dp"),
    Large("16 dp"),
    ExtraLarge("28 dp"),
}

/**
 * Thickness of the remaining-budget bars.
 *
 * From M3 Expressive: linear progress is 4dp by default with a thick 8dp
 * variant. The 12dp step is a home-screen extra (not in M3).
 */
enum class WidgetBarHeight(val label: String) {
    Compact("Thin"),
    Standard("Thick"),
    Comfortable("Bold"),
}

/** Which of the three widgets is being configured; they expose different options. */
enum class WidgetKind { Compact, Providers, Spend }

/**
 * Per-widget configuration.
 *
 * Stored in the widget's own Glance preferences (keyed by app widget id), so two
 * instances of the same widget can be configured differently — one panel widget
 * watching codex, a desktop one showing everything.
 */
data class WidgetSettings(
    /** Provider ids to include; empty means every provider in the snapshot. */
    val providers: Set<String> = emptySet(),
    val sort: WidgetSort = WidgetSort.Tightest,
    /** The reset countdown next to each window name. */
    val showCaptions: Boolean = true,
    val colorMode: WidgetColorMode = WidgetColorMode.Dynamic,
    /** Usage widget: pin one provider instead of following the tightest. */
    val heroProvider: String = "",
    val background: WidgetBackground = WidgetBackground.Solid,
    val corners: WidgetCorners = WidgetCorners.System,
    val barHeight: WidgetBarHeight = WidgetBarHeight.Standard,
    /** Provider brand badges next to provider names. */
    val showBadges: Boolean = true,
    /** The tick on each bar marking how much of the window is still ahead. */
    val showTimeMarker: Boolean = true,
) {
    companion object {
        val PROVIDER_KEY = stringSetPreferencesKey("providers")
        val SORT_KEY = stringPreferencesKey("sort")
        val CAPTIONS_KEY = booleanPreferencesKey("captions")
        val COLOR_KEY = stringPreferencesKey("colorMode")
        val HERO_KEY = stringPreferencesKey("heroProvider")
        val BACKGROUND_KEY = stringPreferencesKey("background")
        val CORNERS_KEY = stringPreferencesKey("corners")
        val BAR_HEIGHT_KEY = stringPreferencesKey("barHeight")
        val BADGES_KEY = booleanPreferencesKey("showBadges")

        /**
         * Kept under its old name: it used to hold a six-way marker style, and
         * every style except `Off` drew a marker, so old widgets keep theirs.
         */
        val PACE_KEY = stringPreferencesKey("paceStyle")
        private const val MARKER_ON = "Position"
        private const val MARKER_OFF = "Off"

        fun from(preferences: Preferences): WidgetSettings = WidgetSettings(
            providers = preferences[PROVIDER_KEY].orEmpty(),
            sort = preferences[SORT_KEY]
                ?.let { name -> WidgetSort.entries.firstOrNull { it.name == name } }
                ?: WidgetSort.Tightest,
            showCaptions = preferences[CAPTIONS_KEY] ?: true,
            colorMode = preferences[COLOR_KEY]
                ?.let { name -> WidgetColorMode.entries.firstOrNull { it.name == name } }
                ?: WidgetColorMode.Dynamic,
            heroProvider = preferences[HERO_KEY].orEmpty(),
            background = preferences[BACKGROUND_KEY]
                ?.let { name -> WidgetBackground.entries.firstOrNull { it.name == name } }
                ?: WidgetBackground.Solid,
            corners = preferences[CORNERS_KEY]
                ?.let { name -> WidgetCorners.entries.firstOrNull { it.name == name } }
                ?: WidgetCorners.System,
            barHeight = preferences[BAR_HEIGHT_KEY]
                ?.let { name -> WidgetBarHeight.entries.firstOrNull { it.name == name } }
                ?: WidgetBarHeight.Standard,
            showBadges = preferences[BADGES_KEY] ?: true,
            showTimeMarker = preferences[PACE_KEY] != MARKER_OFF,
        )

        /** Writes into a widget's preference set, replacing every managed key. */
        fun applyTo(preferences: MutablePreferences, settings: WidgetSettings) {
            preferences[PROVIDER_KEY] = settings.providers
            preferences[SORT_KEY] = settings.sort.name
            preferences[CAPTIONS_KEY] = settings.showCaptions
            preferences[COLOR_KEY] = settings.colorMode.name
            preferences[HERO_KEY] = settings.heroProvider
            preferences[BACKGROUND_KEY] = settings.background.name
            preferences[CORNERS_KEY] = settings.corners.name
            preferences[BAR_HEIGHT_KEY] = settings.barHeight.name
            preferences[BADGES_KEY] = settings.showBadges
            preferences[PACE_KEY] = if (settings.showTimeMarker) MARKER_ON else MARKER_OFF
        }
    }
}

/**
 * Applies the configured filter and order.
 *
 * The filter is intersected with what the snapshot actually holds, so a provider
 * that stopped publishing does not leave a permanently empty widget.
 */
fun List<WidgetProvider>.configured(settings: WidgetSettings): List<WidgetProvider> {
    val filtered = if (settings.providers.isEmpty()) {
        this
    } else {
        filter { it.id in settings.providers }
    }
    return when (settings.sort) {
        WidgetSort.Tightest -> filtered.sortedBy { it.tightest?.percentLeft ?: 100.0 }
        WidgetSort.Name -> filtered.sortedBy { it.label.lowercase() }
    }
}
