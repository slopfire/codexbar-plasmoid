package org.slopfire.codexbar.ui.common

import androidx.compose.ui.graphics.Color
import java.time.Instant
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToInt
import org.slopfire.codexbar.data.model.Pace

/**
 * Display formatting for sync-server values.
 *
 * Every helper accepts a nullable input and returns a placeholder instead of
 * throwing: the server merges snapshots from several devices, so a field that is
 * present on one device can legitimately be missing on another.
 */
object Fmt {

    const val PLACEHOLDER = "—"

    private val currencySymbols = mapOf(
        "USD" to "$",
        "EUR" to "€",
        "GBP" to "£",
        "AUD" to "A$",
        "CAD" to "C$",
        "JPY" to "¥",
    )

    /** `$1.23`, `€4.50`, `12.00 USD`. */
    fun money(value: Double?, currency: String? = null): String {
        if (value == null) return PLACEHOLDER
        val code = currency?.uppercase(Locale.US)?.takeIf { it.isNotBlank() } ?: "USD"
        val symbol = currencySymbols[code]
        val magnitude = abs(value)
        val decimals = if (magnitude >= 1000) 0 else if (magnitude >= 10) 2 else if (magnitude >= 0.01) 2 else 4
        val number = String.format(Locale.US, "%,.${decimals}f", value)
        return if (symbol != null) "$symbol$number" else "$number $code"
    }

    /** `12.3k`, `4.1M`, `812`. */
    fun tokens(value: Long?): String {
        if (value == null) return PLACEHOLDER
        val magnitude = abs(value)
        return when {
            magnitude >= 1_000_000_000 -> String.format(Locale.US, "%.1fB", value / 1_000_000_000.0)
            magnitude >= 1_000_000 -> String.format(Locale.US, "%.1fM", value / 1_000_000.0)
            magnitude >= 1_000 -> String.format(Locale.US, "%.1fk", value / 1_000.0)
            else -> value.toString()
        }
    }

    /** `42%`. */
    fun percent(value: Double?, digits: Int = 0): String {
        if (value == null) return PLACEHOLDER
        return String.format(Locale.US, "%.${digits}f%%", value)
    }

    /** `1,234`. */
    fun count(value: Number?): String {
        if (value == null) return PLACEHOLDER
        return String.format(Locale.US, "%,d", value.toLong())
    }

    /** `3 min ago`, `in 2 h`, `just now`. */
    fun relativeTime(iso: String?, nowMillis: Long = System.currentTimeMillis()): String? {
        val millis = parseMillis(iso) ?: return null
        val deltaSeconds = ((millis - nowMillis) / 1000.0).roundToInt()
        val past = deltaSeconds <= 0
        val seconds = abs(deltaSeconds)
        val text = when {
            seconds < 45 -> "just now"
            seconds < 3600 -> "${(seconds / 60.0).roundToInt()} min"
            seconds < 86_400 -> "${(seconds / 3600.0).roundToInt()} h"
            seconds < 2_592_000 -> "${(seconds / 86_400.0).roundToInt()} d"
            else -> "${(seconds / 2_592_000.0).roundToInt()} mo"
        }
        if (text == "just now") return text
        return if (past) "$text ago" else "in $text"
    }

    /** `resets in 2 h 5 m`, or `resets now`. */
    fun resetCountdown(iso: String?, nowMillis: Long = System.currentTimeMillis()): String? {
        val millis = parseMillis(iso) ?: return null
        val remainingSeconds = (millis - nowMillis) / 1000.0
        if (remainingSeconds <= 0) return "resets now"
        return "resets in ${duration(remainingSeconds)}"
    }

    /** `2 h 5 m`, `45 s`, `3 d 4 h`. */
    fun duration(seconds: Double?): String? {
        if (seconds == null || seconds.isNaN()) return null
        val total = seconds.roundToInt().coerceAtLeast(0)
        val days = total / 86_400
        val hours = (total % 86_400) / 3600
        val minutes = (total % 3600) / 60
        val secs = total % 60
        return when {
            days > 0 -> "$days d $hours h"
            hours > 0 -> "$hours h $minutes m"
            minutes > 0 -> "$minutes m"
            else -> "$secs s"
        }
    }

    /** `Oct 15, 14:30` in the device time zone. */
    fun dateTime(iso: String?, zone: ZoneId = ZoneId.systemDefault()): String? {
        val millis = parseMillis(iso) ?: return null
        val formatter = DateTimeFormatter.ofPattern("MMM d, HH:mm", Locale.getDefault())
        return Instant.ofEpochMilli(millis).atZone(zone).format(formatter)
    }

    /** `Oct 15`. */
    fun date(iso: String?, zone: ZoneId = ZoneId.systemDefault()): String? {
        val millis = parseMillis(iso) ?: return null
        val formatter = DateTimeFormatter.ofPattern("MMM d", Locale.getDefault())
        return Instant.ofEpochMilli(millis).atZone(zone).format(formatter)
    }

    /** `Oct 15` from a `YYYY-MM-DD` history day key. */
    fun dayKey(dayKey: String?): String {
        if (dayKey.isNullOrBlank()) return PLACEHOLDER
        return runCatching {
            LocalDate.parse(dayKey).format(DateTimeFormatter.ofPattern("MMM d", Locale.getDefault()))
        }.getOrElse { dayKey }
    }

    /** `Wed` from a `YYYY-MM-DD` history day key. */
    fun dayKeyWeekday(dayKey: String?): String {
        if (dayKey.isNullOrBlank()) return ""
        return runCatching {
            LocalDate.parse(dayKey).format(DateTimeFormatter.ofPattern("EEE", Locale.getDefault()))
        }.getOrElse { "" }
    }

    /** A one-line pace verdict, preferring CodexBar's own prose when present. */
    fun paceText(pace: Pace?, nowMillis: Long = System.currentTimeMillis()): String? {
        if (pace == null) return null
        pace.summary?.takeIf { it.isNotBlank() }?.let { return it }
        val willLast = pace.willLastToReset
        val delta = pace.deltaPercent
        return when {
            willLast == true && delta != null && delta <= -3 ->
                "on pace, ${abs(delta).roundToInt()}% in reserve"

            willLast == true -> "on pace to last until reset"
            willLast == false && pace.etaSeconds != null ->
                "runs dry in ${duration(pace.etaSeconds)}"

            willLast == false && delta != null ->
                "${delta.roundToInt()}% behind pace"

            willLast == false -> "projected to run out before reset"
            delta != null && delta <= -3 -> "${abs(delta).roundToInt()}% in reserve"
            delta != null && delta >= 3 -> "${delta.roundToInt()}% behind pace"
            else -> null
        }
    }

    /** Parses RFC 3339 (`2026-10-15T04:00:00Z`) and plain instants. */
    fun parseMillis(iso: String?): Long? {
        if (iso.isNullOrBlank()) return null
        runCatching { return OffsetDateTime.parse(iso).toInstant().toEpochMilli() }
        runCatching { return Instant.parse(iso).toEpochMilli() }
        runCatching { return LocalDate.parse(iso).atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli() }
        return null
    }
}

/**
 * Semantic colors for limit health: the same white → amber → red scale the
 * Plasma widget uses for its remaining-limit gradient. Kept independent of the
 * color scheme because "you are about to run out" must read the same in any
 * theme, including one derived from a red wallpaper.
 */
object Health {
    val Good = Color(0xFF2E7D5B)
    val Tight = Color(0xFFB98900)
    val Low = Color(0xFFC4552B)
    val Critical = Color(0xFFB3261E)
    val Neutral = Color(0xFF6F7979)

    /** Remaining-limit gradient color for a 0..100 percentage. */
    fun remainingColor(percentLeft: Double?): Color {
        val fraction = (percentLeft ?: 0.0) / 100.0
        return when {
            percentLeft == null -> Neutral
            fraction >= 0.5 -> Good
            fraction >= 0.25 -> Tight
            fraction >= 0.1 -> Low
            else -> Critical
        }
    }

    /** Pace verdict color: green when the budget lasts, red when it will not. */
    fun paceColor(willLastToReset: Boolean?): Color = when (willLastToReset) {
        true -> Good
        false -> Critical
        null -> Neutral
    }
}
