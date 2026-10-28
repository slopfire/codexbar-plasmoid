package org.slopfire.codexbar.widget

import kotlin.math.roundToLong
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.ui.common.Fmt

/** How a verdict should be painted. Kept theme-free so each widget maps it. */
enum class VerdictTone { SUPPORTED, TERTIARY, ERROR, MUTED }

/** One short piece of prose plus the tone it reads in. */
data class Verdict(val text: String, val tone: VerdictTone)

/**
 * The widget's short verdicts: "on pace", "tight", "dry in 4d".
 *
 * The wording is deliberately shorter than [Fmt.paceText]'s prose — it has to
 * fit on one line beside a provider name — so the pace rules are restated here
 * rather than reused.
 */
object WidgetVerdict {

    private const val DAY_SECONDS = 86_400.0
    private const val HOUR_SECONDS = 3_600.0
    private const val MINUTE_SECONDS = 60.0

    /** A trailing " usage" word, the suffix most provider row titles carry. */
    private val TRAILING_USAGE = Regex("""\s+usage$""", RegexOption.IGNORE_CASE)

    /** Provider-level verdict: failures and staleness outrank the numbers. */
    fun forProvider(provider: WidgetProvider): Verdict {
        if (provider.hasError) return Verdict("refresh failed", VerdictTone.ERROR)
        if (provider.stale) return Verdict("last good data", VerdictTone.MUTED)
        // A provider can report no windows at all; that is information, not a crash.
        return forRow(provider.tightest, provider) ?: Verdict("no limits", VerdictTone.MUTED)
    }

    /**
     * Row-level verdict, or null when there is no row to judge.
     *
     * A provider is optional so callers can judge a single row without owning
     * the provider: when supplied, its error/stale state still wins.
     */
    fun forRow(row: UsageRow?, provider: WidgetProvider? = null): Verdict? {
        if (row == null) return null
        if (provider != null) {
            if (provider.hasError) return Verdict("refresh failed", VerdictTone.ERROR)
            if (provider.stale) return Verdict("last good data", VerdictTone.MUTED)
        }
        val pace = row.pace
        when (pace?.willLastToReset) {
            false -> {
                val eta = shortEta(pace.etaSeconds)
                return Verdict(if (eta != null) "dry in $eta" else "dry early", VerdictTone.ERROR)
            }
            true -> return Verdict("on pace", VerdictTone.SUPPORTED)
            null -> Unit // No pace report: judge the budget, never claim pacing.
        }
        return when {
            row.percentLeft < 20.0 -> Verdict("low", VerdictTone.ERROR)
            row.percentLeft < 30.0 -> Verdict("tight", VerdictTone.TERTIARY)
            // A missing pace must not read as a healthy green claim.
            else -> Verdict("steady", VerdictTone.MUTED)
        }
    }

    /**
     * Compact ETA: `4d`, `18h`, `25m`, `45s`.
     *
     * The unit is picked from the raw value, then rounded, so 90 minutes reads
     * as `2h` rather than `90m`. Zero and negative ETAs mean "already dry" and
     * have no duration to show.
     */
    fun shortEta(seconds: Double?): String? {
        if (seconds == null || seconds.isNaN() || seconds <= 0.0) return null
        return when {
            seconds >= DAY_SECONDS -> "${(seconds / DAY_SECONDS).roundToLong()}d"
            seconds >= HOUR_SECONDS -> "${(seconds / HOUR_SECONDS).roundToLong()}h"
            seconds >= MINUTE_SECONDS -> "${(seconds / MINUTE_SECONDS).roundToLong()}m"
            else -> "${seconds.roundToLong()}s"
        }
    }

    /** `Monthly Usage` -> `monthly`, `5h session` -> `5h session`. */
    fun windowLabel(row: UsageRow): String {
        val title = row.title.trim()
        val stripped = title.replace(TRAILING_USAGE, "").trim()
        val base = stripped.ifBlank { title }
        if (base.isBlank()) return "usage"
        return base.replaceFirstChar { it.lowercase() }
    }

    /** `monthly · resets in 11 d 14 h`, or just the label when unknown. */
    fun headlineMeta(row: UsageRow, nowMillis: Long = System.currentTimeMillis()): String {
        val label = windowLabel(row)
        val countdown = Fmt.resetCountdown(row.resetsAt, nowMillis) ?: return label
        if (countdown == "resets now") return "$label · resets now"
        return "$label · resets in ${countdown.removePrefix("resets in ")}"
    }
}
