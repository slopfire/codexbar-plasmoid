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

    /** Title words that name a window's length or kind rather than what sets it apart. */
    private val GENERIC_WINDOW_WORDS = setOf(
        "session", "hourly", "daily", "day", "weekly", "week", "monthly", "month",
        "usage", "limit", "limits", "window", "rolling", "all", "models", "plan",
    )

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
        // Nothing left is a fact, not a forecast: it outranks any pace report.
        if (row.percentLeft <= 0.0) return Verdict("used up", VerdictTone.ERROR)
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

    /**
     * A window's name short enough to sit beside its percentage: `5h`, `day`,
     * `week`, `month`, `3d` from the window length, or the first word of the
     * title (cut to 8 characters) when the length is unknown.
     */
    fun shortWindowLabel(row: UsageRow): String {
        val minutes = row.windowMinutes
        if (minutes != null && !minutes.isNaN() && minutes > 0.0) {
            val hours = minutes / 60.0
            val days = hours / 24.0
            return when {
                minutes < 60.0 -> "${minutes.roundToLong()}m"
                hours < 24.0 -> "${hours.roundToLong()}h"
                days in 0.95..1.05 -> "day"
                days in 6.5..7.5 -> "week"
                days in 27.0..32.0 -> "month"
                else -> "${days.roundToLong()}d"
            }
        }
        val word = windowLabel(row).split(' ').firstOrNull { it.isNotBlank() } ?: "usage"
        return if (word.length > 8) word.take(7) + "…" else word
    }

    /**
     * [shortWindowLabel] for each of a provider's windows, made distinct: when two
     * windows share a length (Claude's weekly and weekly Opus), the later ones are
     * named by the title word that sets them apart, e.g. `week` and `opus`.
     */
    fun shortWindowLabels(rows: List<UsageRow>): List<String> {
        val labels = rows.map(::shortWindowLabel)
        return labels.mapIndexed { index, label ->
            if (labels.indexOf(label) == index) return@mapIndexed label
            val word = windowLabel(rows[index])
                .split(' ', '(', ')', '-', '/', ',')
                .map { it.trim().lowercase() }
                .firstOrNull { it.isNotEmpty() && it !in GENERIC_WINDOW_WORDS && it != label }
                ?: return@mapIndexed label
            if (word.length > 8) word.take(7) + "…" else word
        }
    }

    /**
     * `monthly · resets in 11 d 14 h`, or just the label when unknown.
     *
     * When the line has to fit in [maxChars], the window name is shortened and
     * the countdown kept whole: the name is context, the countdown is the fact.
     */
    fun headlineMeta(
        row: UsageRow,
        nowMillis: Long = System.currentTimeMillis(),
        maxChars: Int = Int.MAX_VALUE,
    ): String {
        val label = windowLabel(row)
        val countdown = Fmt.resetCountdown(row.resetsAt, nowMillis) ?: return label
        val reset = if (countdown == "resets now") {
            " · resets now"
        } else {
            " · resets in ${countdown.removePrefix("resets in ")}"
        }
        val room = maxChars - reset.length
        val shown = when {
            label.length <= room -> label
            room >= 4 -> label.take(room - 1).trimEnd() + "…"
            else -> return reset.removePrefix(" · ")
        }
        return shown + reset
    }
}
