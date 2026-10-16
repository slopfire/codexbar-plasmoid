package org.slopfire.codexbar.widget

import kotlin.math.roundToInt
import org.slopfire.codexbar.ui.common.Fmt

/**
 * Pacing-marker math shared by the widget bars, ported from the desktop
 * plasmoid's `UsageBarRow.qml` (`timeLeftFraction`).
 *
 * The marker is drawn at the same scale as `percentLeft`: the fill reaching
 * past it means the budget outlasts the clock.
 */
object WidgetPace {

    /**
     * Fraction of the window still ahead (1 = just reset, 0 = about to reset),
     * or null when unknown.
     *
     * Unknown covers: no reset time, no usable window length, an unparseable
     * reset, or a reset scheduled at or beyond a full window length (which reads
     * as more than a whole window and must not pin the marker to an edge).
     */
    fun timeLeftFraction(
        resetsAt: String?,
        windowMinutes: Double?,
        nowMillis: Long = System.currentTimeMillis(),
    ): Double? {
        if (resetsAt.isNullOrBlank()) return null
        if (windowMinutes == null || windowMinutes.isNaN() || windowMinutes <= 0) return null
        val resetMillis = Fmt.parseMillis(resetsAt) ?: return null
        val left = (resetMillis - nowMillis) / (windowMinutes * 60_000.0)
        if (left.isNaN() || left > 1.0) return null
        return maxOf(0.0, left)
    }

    /**
     * Marker slot for a quantized bar: cells before it go left, the rest go
     * right. Null when there is no marker to split at.
     */
    fun markerSlot(markerFraction: Double?, cells: Int): Int? =
        markerFraction?.let { (it * cells).toInt().coerceIn(0, cells) }

    /** Filled cells for a quantized bar, counted left to right past the marker. */
    fun fillSlots(fillFraction: Double, cells: Int): Int =
        (fillFraction.coerceIn(0.0, 1.0) * cells).roundToInt().coerceIn(0, cells)
}
