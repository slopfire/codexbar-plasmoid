package org.slopfire.codexbar.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import org.slopfire.codexbar.data.model.Pace
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.widget.WidgetProvider

/**
 * The widget's short verdicts: what the pace report shortens to, how a bare
 * percentage is judged when there is no pace at all, and how a row title becomes
 * a window label.
 *
 * The 20/30 budget thresholds are duplicated from the rule on purpose, so a
 * retune has to touch the test as well.
 */
class WidgetVerdictTest {

    private fun provider(
        rows: List<UsageRow> = emptyList(),
        stale: Boolean = false,
        hasError: Boolean = false,
    ): WidgetProvider = WidgetProvider(
        id = "codex",
        label = "Codex",
        account = null,
        rows = rows,
        creditsRemaining = null,
        currency = null,
        sessionCost = null,
        sessionTokens = null,
        cost30d = null,
        tokens30d = null,
        topModel = null,
        topModelCost = null,
        stale = stale,
        hasError = hasError,
    )

    private fun row(
        percentLeft: Double,
        title: String = "Weekly",
        resetsAt: String? = null,
        pace: Pace? = null,
    ): UsageRow = UsageRow(
        id = "weekly",
        title = title,
        percentLeft = percentLeft,
        resetsAt = resetsAt,
        windowMinutes = 10_080.0,
        pace = pace,
    )

    private fun dry(etaSeconds: Double?): Pace =
        Pace(willLastToReset = false, etaSeconds = etaSeconds)

    @Test
    fun dryVerdictCarriesTheEta() {
        val verdict = WidgetVerdict.forRow(row(60.0, pace = dry(4 * 86_400.0)))!!
        assertEquals("dry in 4d", verdict.text)
        assertEquals(VerdictTone.ERROR, verdict.tone)
    }

    @Test
    fun dryVerdictWithoutEtaSaysEarly() {
        val verdict = WidgetVerdict.forRow(row(60.0, pace = dry(null)))!!
        assertEquals("dry early", verdict.text)
        assertEquals(VerdictTone.ERROR, verdict.tone)
    }

    @Test
    fun percentVerdictsUseTheirOwnThresholds() {
        assertEquals(Verdict("low", VerdictTone.ERROR), WidgetVerdict.forRow(row(17.0)))
        assertEquals(Verdict("low", VerdictTone.ERROR), WidgetVerdict.forRow(row(19.9)))
        assertEquals(Verdict("tight", VerdictTone.TERTIARY), WidgetVerdict.forRow(row(20.0)))
        assertEquals(Verdict("tight", VerdictTone.TERTIARY), WidgetVerdict.forRow(row(29.9)))
        assertEquals(Verdict("steady", VerdictTone.MUTED), WidgetVerdict.forRow(row(30.0)))
    }

    @Test
    fun aMissingPaceReportIsNeverCalledOnPace() {
        // The most common shape in practice: a percentage with no pace object.
        val verdict = WidgetVerdict.forRow(row(90.0, pace = null))!!
        assertEquals("steady", verdict.text)
        assertEquals(VerdictTone.MUTED, verdict.tone)
    }

    @Test
    fun onPacePaceObjectWinsOverHighPercent() {
        val verdict = WidgetVerdict.forRow(row(90.0, pace = Pace(willLastToReset = true)))!!
        assertEquals("on pace", verdict.text)
    }

    @Test
    fun staleBeatsTheNumbers() {
        val stale = provider(rows = listOf(row(5.0)), stale = true)
        val verdict = WidgetVerdict.forProvider(stale)
        assertEquals("last good data", verdict.text)
        assertEquals(VerdictTone.MUTED, verdict.tone)
    }

    @Test
    fun hasErrorBeatsStale() {
        val broken = provider(rows = listOf(row(5.0)), stale = true, hasError = true)
        val verdict = WidgetVerdict.forProvider(broken)
        assertEquals("refresh failed", verdict.text)
        assertEquals(VerdictTone.ERROR, verdict.tone)
    }

    @Test
    fun forRowAppliesProviderStateAndNullsOut() {
        assertNull(WidgetVerdict.forRow(null))
        assertNull(WidgetVerdict.forRow(null, provider(rows = listOf(row(5.0)))))

        val staleRow = WidgetVerdict.forRow(row(5.0), provider(rows = listOf(row(5.0)), stale = true))!!
        assertEquals("last good data", staleRow.text)

        val brokenRow = WidgetVerdict.forRow(
            row(5.0),
            provider(rows = listOf(row(5.0)), hasError = true),
        )!!
        assertEquals("refresh failed", brokenRow.text)
    }

    @Test
    fun forProviderUsesTheTightestRow() {
        // 40% with no pace object: steady, not a green "on pace" claim.
        val healthy = provider(rows = listOf(row(90.0), row(80.0), row(40.0)))
        assertEquals("steady", WidgetVerdict.forProvider(healthy).text)
        assertEquals(VerdictTone.MUTED, WidgetVerdict.forProvider(healthy).tone)

        val paced = provider(
            rows = listOf(row(60.0, pace = Pace(willLastToReset = true))),
        )
        assertEquals("on pace", WidgetVerdict.forProvider(paced).text)
        assertEquals(VerdictTone.SUPPORTED, WidgetVerdict.forProvider(paced).tone)

        val tight = provider(rows = listOf(row(90.0), row(17.0), row(60.0)))
        assertEquals("low", WidgetVerdict.forProvider(tight).text)
        assertEquals(VerdictTone.ERROR, WidgetVerdict.forProvider(tight).tone)
    }

    @Test
    fun forProviderWithoutRowsDoesNotCrash() {
        val empty = provider(rows = emptyList())
        assertEquals("no limits", WidgetVerdict.forProvider(empty).text)
        assertEquals(VerdictTone.MUTED, WidgetVerdict.forProvider(empty).tone)
    }

    @Test
    fun shortEtaPicksTheLargestUsefulUnit() {
        assertEquals("4d", WidgetVerdict.shortEta(4 * 86_400.0))
        assertEquals("18h", WidgetVerdict.shortEta(18 * 3_600.0))
        assertEquals("25m", WidgetVerdict.shortEta(25 * 60.0))
        assertEquals("45s", WidgetVerdict.shortEta(45.0))
    }

    @Test
    fun shortEtaRoundsInsteadOfTruncating() {
        assertEquals("2h", WidgetVerdict.shortEta(90 * 60.0))
        assertEquals("3d", WidgetVerdict.shortEta(2.6 * 86_400.0))
    }

    @Test
    fun shortEtaRejectsNonPositiveAndMissingValues() {
        assertNull(WidgetVerdict.shortEta(null))
        assertNull(WidgetVerdict.shortEta(0.0))
        assertNull(WidgetVerdict.shortEta(-30.0))
        assertNull(WidgetVerdict.shortEta(Double.NaN))
    }

    @Test
    fun windowLabelStripsTheUsageSuffix() {
        assertEquals("monthly", WidgetVerdict.windowLabel(row(50.0, title = "Monthly Usage")))
        assertEquals("weekly", WidgetVerdict.windowLabel(row(50.0, title = "Weekly")))
        assertEquals("5h session", WidgetVerdict.windowLabel(row(50.0, title = "5h session")))
        assertEquals("usage", WidgetVerdict.windowLabel(row(50.0, title = "")))
        assertEquals("usage", WidgetVerdict.windowLabel(row(50.0, title = "   ")))
    }

    @Test
    fun headlineMetaAppendsAnExactCountdown() {
        val monthly = row(50.0, title = "Monthly Usage", resetsAt = "1970-01-02T00:00:00Z")
        assertEquals("monthly · resets in 1 d 0 h", WidgetVerdict.headlineMeta(monthly, nowMillis = 0L))
    }

    @Test
    fun headlineMetaSaysResetsNowWhenPast() {
        val weekly = row(50.0, title = "Weekly", resetsAt = "1970-01-01T00:00:00Z")
        assertEquals("weekly · resets now", WidgetVerdict.headlineMeta(weekly, nowMillis = 0L))
    }

    @Test
    fun shortWindowLabelsNameRepeatedLengthsByWhatSetsThemApart() {
        val weekly = row(45.0, title = "Weekly").copy(windowMinutes = 10080.0)
        val opus = row(70.0, title = "Weekly Opus").copy(windowMinutes = 10080.0)
        val session = row(82.0, title = "Session").copy(windowMinutes = 300.0)
        assertEquals(listOf("week", "opus", "5h"), WidgetVerdict.shortWindowLabels(listOf(weekly, opus, session)))
        // Nothing tells them apart: keep the shared name rather than invent one.
        assertEquals(listOf("week", "week"), WidgetVerdict.shortWindowLabels(listOf(weekly, weekly)))
    }

    @Test
    fun headlineMetaShortensTheNameBeforeTheCountdown() {
        val plan = row(0.0, title = "Pro Plan Monthly Subscription", resetsAt = "1970-01-03T00:00:00Z")
        // " · resets in 2 d 0 h" is 20 characters; the name gets the other 16.
        assertEquals(
            "pro Plan Monthl… · resets in 2 d 0 h",
            WidgetVerdict.headlineMeta(plan, nowMillis = 0L, maxChars = 36),
        )
        assertEquals("resets in 2 d 0 h", WidgetVerdict.headlineMeta(plan, nowMillis = 0L, maxChars = 22))
    }

    @Test
    fun headlineMetaWithoutResetIsJustTheLabel() {
        assertEquals("monthly", WidgetVerdict.headlineMeta(row(50.0, title = "Monthly Usage")))
        assertEquals(
            "monthly",
            WidgetVerdict.headlineMeta(row(50.0, title = "Monthly Usage", resetsAt = "not-a-time")),
        )
    }

    @Test
    fun shortWindowLabelsComeFromTheWindowLength() {
        fun window(minutes: Double?, title: String = "Whatever") =
            UsageRow(id = "w", title = title, percentLeft = 50.0, resetsAt = null, windowMinutes = minutes, pace = null)
        assertEquals("5h", WidgetVerdict.shortWindowLabel(window(300.0)))
        assertEquals("30m", WidgetVerdict.shortWindowLabel(window(30.0)))
        assertEquals("day", WidgetVerdict.shortWindowLabel(window(1_440.0)))
        assertEquals("week", WidgetVerdict.shortWindowLabel(window(10_080.0)))
        assertEquals("month", WidgetVerdict.shortWindowLabel(window(43_200.0)))
        assertEquals("3d", WidgetVerdict.shortWindowLabel(window(4_320.0)))
    }

    @Test
    fun shortWindowLabelFallsBackToTheTitlesFirstWord() {
        fun titled(title: String) =
            UsageRow(id = "w", title = title, percentLeft = 50.0, resetsAt = null, windowMinutes = null, pace = null)
        assertEquals("session", WidgetVerdict.shortWindowLabel(titled("Session")))
        assertEquals("subscri…", WidgetVerdict.shortWindowLabel(titled("Subscription credits")))
    }

    @Test
    fun anEmptyWindowIsUsedUpWhateverThePace() {
        val verdict = WidgetVerdict.forRow(row(0.0, pace = Pace(willLastToReset = true)))
        assertEquals(Verdict("used up", VerdictTone.ERROR), verdict)
    }
}
