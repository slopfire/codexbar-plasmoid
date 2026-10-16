package org.slopfire.codexbar.data.widget

import androidx.datastore.preferences.core.MutablePreferences
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.intPreferencesKey
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
    Tightest("Tightest first"),
    Name("Name"),
}

/**
 * Widget surface fill.
 *
 * From M3: widget bodies use a `surface container` role for containment.
 * My design decision (not in M3): the translucency level and the transparent
 * option, which let the wallpaper show through like a launcher widget.
 */
enum class WidgetBackground(val label: String) {
    Solid("Solid"),
    Translucent("Translucent"),
    Transparent("Transparent"),
}

/**
 * Outer corner radius for the widget shell.
 *
 * From M3: shape scale steps `CornerMedium` 12dp, `CornerLarge` 16dp and
 * `CornerExtraLarge` 28dp. System leaves the radius to the launcher
 * (`appWidgetBackground` owns it on Android 12+).
 */
enum class WidgetCorners(val label: String) {
    System("System"),
    Medium("Medium 12"),
    Large("Large 16"),
    ExtraLarge("XL 28"),
}

/**
 * Thickness of the remaining-budget bar.
 *
 * From M3: linear progress default track thickness is 4dp and configurable in
 * Expressive. My design decision (not in M3): the 6/8/12dp steps, tuned so the
 * time marker stays legible on a home screen.
 */
enum class WidgetBarHeight(val label: String) {
    Compact("Slim"),
    Standard("Standard"),
    Comfortable("Bold"),
}

/**
 * How the pacing (time-remaining) marker on the bar is drawn.
 *
 * Mirrors the desktop plasmoid's `paceIndicatorStyle` (`position`, `bar`,
 * `provider`, `theme`, `gaps`) so the phone and the desktop agree at a
 * glance, plus `Off` to render a single unsplit bar.
 */
enum class WidgetPaceStyle(val label: String) {
    Position("Position color"),
    Bar("Match bar"),
    Provider("Provider color"),
    Theme("Theme text"),
    Gaps("Gaps only"),
    Off("Off"),
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
    /** The reset countdown and pace verdict line under each bar. */
    val showCaptions: Boolean = true,
    val colorMode: WidgetColorMode = WidgetColorMode.Dynamic,
    /** Compact widget: pin one provider instead of following the tightest. */
    val heroProvider: String = "",
    /** Providers widget: 0 follows the granted height. */
    val maxRows: Int = 0,
    val background: WidgetBackground = WidgetBackground.Solid,
    val corners: WidgetCorners = WidgetCorners.Large,
    val barHeight: WidgetBarHeight = WidgetBarHeight.Standard,
    /** The CodexBar title and freshness line. */
    val showHeader: Boolean = true,
    /** Provider brand badges next to row titles. */
    val showBadges: Boolean = true,
    /** Pacing marker style for the split progress bar. */
    val paceStyle: WidgetPaceStyle = WidgetPaceStyle.Position,
) {
    companion object {
        val PROVIDER_KEY = stringSetPreferencesKey("providers")
        val SORT_KEY = stringPreferencesKey("sort")
        val CAPTIONS_KEY = booleanPreferencesKey("captions")
        val COLOR_KEY = stringPreferencesKey("colorMode")
        val HERO_KEY = stringPreferencesKey("heroProvider")
        val ROWS_KEY = intPreferencesKey("maxRows")
        val BACKGROUND_KEY = stringPreferencesKey("background")
        val CORNERS_KEY = stringPreferencesKey("corners")
        val BAR_HEIGHT_KEY = stringPreferencesKey("barHeight")
        val HEADER_KEY = booleanPreferencesKey("showHeader")
        val BADGES_KEY = booleanPreferencesKey("showBadges")
        val PACE_KEY = stringPreferencesKey("paceStyle")

        const val AUTO_ROWS = 0

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
            maxRows = preferences[ROWS_KEY] ?: AUTO_ROWS,
            background = preferences[BACKGROUND_KEY]
                ?.let { name -> WidgetBackground.entries.firstOrNull { it.name == name } }
                ?: WidgetBackground.Solid,
            corners = preferences[CORNERS_KEY]
                ?.let { name -> WidgetCorners.entries.firstOrNull { it.name == name } }
                ?: WidgetCorners.Large,
            barHeight = preferences[BAR_HEIGHT_KEY]
                ?.let { name -> WidgetBarHeight.entries.firstOrNull { it.name == name } }
                ?: WidgetBarHeight.Standard,
            showHeader = preferences[HEADER_KEY] ?: true,
            showBadges = preferences[BADGES_KEY] ?: true,
            paceStyle = preferences[PACE_KEY]
                ?.let { name -> WidgetPaceStyle.entries.firstOrNull { it.name == name } }
                ?: WidgetPaceStyle.Position,
        )

        /** Writes into a widget's preference set, replacing every managed key. */
        fun applyTo(preferences: MutablePreferences, settings: WidgetSettings) {
            preferences[PROVIDER_KEY] = settings.providers
            preferences[SORT_KEY] = settings.sort.name
            preferences[CAPTIONS_KEY] = settings.showCaptions
            preferences[COLOR_KEY] = settings.colorMode.name
            preferences[HERO_KEY] = settings.heroProvider
            preferences[ROWS_KEY] = settings.maxRows
            preferences[BACKGROUND_KEY] = settings.background.name
            preferences[CORNERS_KEY] = settings.corners.name
            preferences[BAR_HEIGHT_KEY] = settings.barHeight.name
            preferences[HEADER_KEY] = settings.showHeader
            preferences[BADGES_KEY] = settings.showBadges
            preferences[PACE_KEY] = settings.paceStyle.name
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
