package org.slopfire.codexbar.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetBarHeight
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.data.widget.WidgetSettings

/**
 * Pacing-marker math and appearance defaults for the home screen widgets.
 *
 * The marker fraction mirrors the desktop plasmoid's `timeLeftFraction` in
 * `UsageBarRow.qml`: unknown whenever there is no usable reset or window, more
 * than a whole window ahead, or an unparseable timestamp.
 */
class WidgetPaceTest {

    private val now = 1_700_000_000_000L

    private fun resetIn(minutes: Long): String =
        java.time.Instant.ofEpochMilli(now + minutes * 60_000L).toString()

    @Test
    fun halfWindowLeft() {
        val fraction = WidgetPace.timeLeftFraction(resetIn(150), 300.0, now)
        assertEquals(0.5, fraction!!, 0.001)
    }

    @Test
    fun justResetReadsFull() {
        val fraction = WidgetPace.timeLeftFraction(resetIn(300), 300.0, now)
        assertEquals(1.0, fraction!!, 0.001)
    }

    @Test
    fun pastResetClampsToZero() {
        val fraction = WidgetPace.timeLeftFraction(resetIn(-5), 300.0, now)
        assertEquals(0.0, fraction!!, 0.001)
    }

    @Test
    fun resetBeyondFullWindowIsUnknown() {
        assertNull(WidgetPace.timeLeftFraction(resetIn(400), 300.0, now))
    }

    @Test
    fun missingInputsAreUnknown() {
        assertNull(WidgetPace.timeLeftFraction(null, 300.0, now))
        assertNull(WidgetPace.timeLeftFraction(resetIn(150), null, now))
        assertNull(WidgetPace.timeLeftFraction(resetIn(150), 0.0, now))
        assertNull(WidgetPace.timeLeftFraction("not-a-time", 300.0, now))
    }

    @Test
    fun markerSlotSplitsCells() {
        assertEquals(12, WidgetPace.markerSlot(0.5, 24))
        assertEquals(0, WidgetPace.markerSlot(0.0, 24))
        assertEquals(24, WidgetPace.markerSlot(1.0, 24))
        assertEquals(null, WidgetPace.markerSlot(null, 24))
    }

    @Test
    fun fillSlotsRoundToNearestCell() {
        assertEquals(10, WidgetPace.fillSlots(0.42, 24))
        assertEquals(0, WidgetPace.fillSlots(0.0, 24))
        assertEquals(24, WidgetPace.fillSlots(1.0, 24))
    }

    @Test
    fun appearanceDefaultsFollowTheLauncher() {
        val settings = WidgetSettings()
        assertEquals(WidgetBackground.Solid, settings.background)
        assertEquals(WidgetCorners.System, settings.corners)
        assertEquals(WidgetBarHeight.Standard, settings.barHeight)
        assertEquals(true, settings.showTimeMarker)
        assertEquals(true, settings.showBadges)
    }

    /**
     * Every setting round-trips through the widget's preference set. This is the
     * contract the header removal had to keep intact.
     */
    @Test
    fun settingsRoundTripThroughPreferences() {
        val original = WidgetSettings()
        val preferences = androidx.datastore.preferences.core.emptyPreferences().toMutablePreferences()
        WidgetSettings.applyTo(preferences, original)
        assertEquals(original, WidgetSettings.from(preferences))
    }

    /** Widgets saved with the old six-way marker style keep a marker unless it was Off. */
    @Test
    fun legacyMarkerStylesMapToTheToggle() {
        val preferences = androidx.datastore.preferences.core.emptyPreferences().toMutablePreferences()
        preferences[WidgetSettings.PACE_KEY] = "Gaps"
        assertEquals(true, WidgetSettings.from(preferences).showTimeMarker)
        preferences[WidgetSettings.PACE_KEY] = "Off"
        assertEquals(false, WidgetSettings.from(preferences).showTimeMarker)
    }

    @Test
    fun markerOffRoundTrips() {
        val original = WidgetSettings(showTimeMarker = false, showCaptions = false)
        val preferences = androidx.datastore.preferences.core.emptyPreferences().toMutablePreferences()
        WidgetSettings.applyTo(preferences, original)
        assertEquals(original, WidgetSettings.from(preferences))
    }
}
