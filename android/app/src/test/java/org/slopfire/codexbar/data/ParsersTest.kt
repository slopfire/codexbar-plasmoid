package org.slopfire.codexbar.data

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.slopfire.codexbar.data.model.DailyUsagePoint
import org.slopfire.codexbar.data.model.LimitResetCredits
import org.slopfire.codexbar.data.model.Pace
import org.slopfire.codexbar.data.model.ProviderEntry
import org.slopfire.codexbar.data.model.ProviderStatus
import org.slopfire.codexbar.data.model.RemoteDevice
import org.slopfire.codexbar.data.model.TokenUsage
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.model.from
import org.slopfire.codexbar.data.model.parseDeviceList
import org.slopfire.codexbar.data.model.parseHealth
import org.slopfire.codexbar.data.model.parseUsageSnapshot
import org.slopfire.codexbar.ui.common.Fmt

/**
 * Exercises the tolerant JSON layer against the exact merged payload shape the
 * CodexBar sync server returns, plus the type-mismatch and error shapes that
 * arise when several devices publish snapshots.
 */
class ParsersTest {

    /** The full realistic `GET /v1/usage` payload. */
    private fun fullPayload(): JSONObject = JSONObject(FULL_PAYLOAD)

    @Test
    fun fullPayloadMapsEveryField() {
        val snapshot = parseUsageSnapshot(fullPayload())
        assertEquals("2026-10-15T04:00:00Z", snapshot.generatedAt)
        assertEquals("all", snapshot.requestedProvider)
        assertNull(snapshot.costError)
        assertEquals(1, snapshot.entries.size)

        val entry = snapshot.entries[0]
        assertEquals("codex", entry.provider)
        assertEquals("user@example.com", entry.account)
        assertEquals("Slopfire", entry.organization)
        assertEquals("Pro", entry.plan)
        assertEquals("cli", entry.source)
        assertNull(entry.siteUrl)
        assertEquals("0.1.11", entry.version)
        assertEquals("2026-10-15T03:59:00Z", entry.updatedAt)
        assertEquals("codex|user@example.com", entry.key)
        assertFalse(entry.isEmpty)
        assertNull(entry.error)
        assertFalse(entry.stale)

        assertNotNull(entry.status)
        assertEquals("none", entry.status!!.indicator)
        assertEquals("Operational", entry.status!!.description)
        assertEquals("2026-10-15T03:50:00Z", entry.status!!.updatedAt)

        assertEquals(2, entry.rows.size)
        val primary = entry.rows[0]
        assertEquals("primary", primary.id)
        assertEquals("Session", primary.title)
        assertEquals(42.5, primary.percentLeft, 0.0)
        assertEquals("2026-10-15T09:00:00Z", primary.resetsAt)
        assertEquals(300.0, primary.windowMinutes!!, 0.0)
        assertNotNull(primary.pace)
        assertEquals(true, primary.pace!!.willLastToReset)
        assertEquals(-6.0, primary.pace!!.deltaPercent!!, 0.0)
        assertEquals(48.0, primary.pace!!.expectedUsedPercent!!, 0.0)
        assertNull(primary.pace!!.etaSeconds)
        assertNull(primary.pace!!.summary)

        val secondary = entry.rows[1]
        assertEquals("secondary", secondary.id)
        assertEquals("Weekly", secondary.title)
        assertEquals(88.0, secondary.percentLeft, 0.0)
        assertEquals(10080.0, secondary.windowMinutes!!, 0.0)
        assertNull(secondary.pace)

        assertEquals(8.56, entry.creditsRemaining!!, 0.0)

        assertNotNull(entry.limitResetCredits)
        assertEquals(2, entry.limitResetCredits!!.availableCount)
        assertEquals("2026-10-18T00:00:00Z", entry.limitResetCredits!!.nextExpiresAt)
        assertEquals(1, entry.limitResetCredits!!.items.size)
        val credit = entry.limitResetCredits!!.items[0]
        assertEquals("r1", credit.id)
        assertEquals("Free reset", credit.title)
        assertEquals("granted", credit.description)
        assertEquals("available", credit.status)
        assertEquals("weekly", credit.resetType)
        assertEquals("2026-10-10T00:00:00Z", credit.grantedAt)
        assertEquals("2026-10-18T00:00:00Z", credit.expiresAt)

        assertEquals(100.0, entry.codeReviewRemainingPercent!!, 0.0)

        assertNotNull(entry.tokenUsage)
        val tokens = entry.tokenUsage!!
        assertEquals(1.23, tokens.sessionCostUSD!!, 0.0)
        assertEquals(45678L, tokens.sessionTokens)
        assertEquals(12.3, tokens.last30DaysCostUSD!!, 0.0)
        assertEquals(456789L, tokens.last30DaysTokens)
        assertEquals("listPriceEstimate", tokens.provenance)
        assertEquals(true, tokens.historyCoverageIsEstablished)
        assertEquals("USD", tokens.currencyCode)
        assertEquals("Today", tokens.sessionLabel)
        assertEquals("30d", tokens.last30DaysLabel)
        assertEquals("local", tokens.historySource)

        assertEquals(1, entry.dailyUsage.size)
        val day = entry.dailyUsage[0]
        assertEquals("2026-10-14", day.dayKey)
        assertEquals(1234L, day.totalTokens)
        assertEquals(0.5, day.costUSD, 0.0)
        assertEquals("local", day.historySource)
        assertEquals("abc", day.historyFingerprint)
        assertEquals(1, day.models.size)
        assertEquals("gpt-5", day.models[0].name)
        assertEquals(0.5, day.models[0].costUSD, 0.0)
        assertEquals(1234L, day.models[0].totalTokens)
        assertEquals(1, day.limitResets.size)
        assertEquals("Weekly", day.limitResets[0].title)
        assertEquals(88.0, day.limitResets[0].percentLeft!!, 0.0)
        assertEquals("2026-10-20T00:00:00Z", day.limitResets[0].resetsAt)

        assertEquals("local", entry.historySource)
        assertEquals(1, entry.remoteDevices.size)
        assertEquals("alpha", entry.remoteDevices[0].device)
        assertEquals("2026-10-15T03:59:00Z", entry.remoteDevices[0].updatedAt)
        assertEquals("2026-10-15T04:00:00Z", entry.remoteDevices[0].receivedAt)
    }

    // ------------------------------------------------------------------
    // Tolerance
    // ------------------------------------------------------------------

    @Test
    fun missingProviderDefaultsToUnknown() {
        val entry = ProviderEntry.from(JSONObject("{}"))
        assertEquals("unknown", entry.provider)
        assertEquals("unknown|", entry.key)
        assertNull(entry.account)
        assertTrue(entry.rows.isEmpty())
        assertTrue(entry.isEmpty)
    }

    @Test
    fun missingRowsIsEmptyList() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex"}"""))
        assertTrue(entry.rows.isEmpty())
    }

    @Test
    fun nonObjectRowsAreSkipped() {
        val json = JSONObject(
            """{"provider":"codex","rows":[{"id":"a","percentLeft":1},7,{"id":"b","percentLeft":2}]}""",
        )
        val entry = ProviderEntry.from(json)
        assertEquals(2, entry.rows.size)
        assertEquals("a", entry.rows[0].id)
        assertEquals("b", entry.rows[1].id)
    }

    @Test
    fun stringPercentLeftParsedAsDouble() {
        val row = UsageRow.from(JSONObject("""{"id":"x","percentLeft":"42.5"}"""))
        assertEquals(42.5, row.percentLeft, 0.0)
    }

    @Test
    fun numericTotalTokensParsedAsLong() {
        val day = DailyUsagePoint.from(JSONObject("""{"dayKey":"2026-10-14","totalTokens":1000.0}"""))
        assertEquals(1000L, day.totalTokens)
    }

    @Test
    fun explicitNullAccountIsNull() {
        val json = JSONObject().put("provider", "codex").put("account", JSONObject.NULL)
        val entry = ProviderEntry.from(json)
        assertNull(entry.account)
    }

    @Test
    fun unknownExtraKeyIsIgnored() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex","totallyUnknown":"zzz"}"""))
        assertEquals("codex", entry.provider)
    }

    // ------------------------------------------------------------------
    // Error shapes
    // ------------------------------------------------------------------

    @Test
    fun errorObjectBecomesMessage() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex","error":{"message":"boom"}}"""))
        assertEquals("boom", entry.error)
    }

    @Test
    fun errorPlainStringIsKept() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex","error":"plain text"}"""))
        assertEquals("plain text", entry.error)
    }

    @Test
    fun nullErrorIsNull() {
        val json = JSONObject().put("provider", "codex").put("error", JSONObject.NULL)
        assertNull(ProviderEntry.from(json).error)
    }

    @Test
    fun usageUnavailableObjectBecomesMessage() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex","usageUnavailable":{"message":"no data"}}"""))
        assertEquals("no data", entry.usageUnavailable)
    }

    @Test
    fun usageUnavailablePlainStringIsKept() {
        val entry = ProviderEntry.from(JSONObject("""{"provider":"codex","usageUnavailable":"usage text"}"""))
        assertEquals("usage text", entry.usageUnavailable)
    }

    @Test
    fun nullUsageUnavailableIsNull() {
        val json = JSONObject().put("provider", "codex").put("usageUnavailable", JSONObject.NULL)
        assertNull(ProviderEntry.from(json).usageUnavailable)
    }

    @Test
    fun staleEntryMapsSinceAndLastAttempt() {
        val entry = ProviderEntry.from(
            JSONObject(
                """{"provider":"codex","stale":true,"staleSince":"2026-10-15T01:00:00Z",
                   "lastAttemptError":"timeout"}""",
            ),
        )
        assertTrue(entry.stale)
        assertEquals("2026-10-15T01:00:00Z", entry.staleSince)
        assertEquals("timeout", entry.lastAttemptError)
    }

    // ------------------------------------------------------------------
    // Individual member extensions
    // ------------------------------------------------------------------

    @Test
    fun usageRowFromFallsBackForMissingFields() {
        val row = UsageRow.from(JSONObject("""{"title":"Session"}"""))
        assertEquals("Session", row.id)
        assertEquals("Session", row.title)
        assertEquals(0.0, row.percentLeft, 0.0)
        assertNull(row.resetsAt)
        assertNull(row.windowMinutes)
        assertNull(row.pace)
    }

    @Test
    fun paceFromMapsAllFields() {
        val pace = Pace.from(
            JSONObject(
                """{"willLastToReset":false,"deltaPercent":7.5,"expectedUsedPercent":60,
                   "etaSeconds":3600,"summary":"runs dry"}""",
            ),
        )
        assertEquals(false, pace.willLastToReset)
        assertEquals(7.5, pace.deltaPercent!!, 0.0)
        assertEquals(60.0, pace.expectedUsedPercent!!, 0.0)
        assertEquals(3600.0, pace.etaSeconds!!, 0.0)
        assertEquals("runs dry", pace.summary)
    }

    @Test
    fun tokenUsageFromMapsAllFields() {
        val usage = TokenUsage.from(
            JSONObject(
                """{"sessionCostUSD":1.5,"sessionTokens":10,"last30DaysCostUSD":2.5,"last30DaysTokens":20,
                   "provenance":"p","historyCoverageIsEstablished":true,"currencyCode":"USD",
                   "sessionLabel":"Today","last30DaysLabel":"30d","historySource":"local"}""",
            ),
        )
        assertEquals(1.5, usage.sessionCostUSD!!, 0.0)
        assertEquals(10L, usage.sessionTokens)
        assertEquals(2.5, usage.last30DaysCostUSD!!, 0.0)
        assertEquals(20L, usage.last30DaysTokens)
        assertEquals("p", usage.provenance)
        assertEquals(true, usage.historyCoverageIsEstablished)
        assertEquals("USD", usage.currencyCode)
        assertEquals("Today", usage.sessionLabel)
        assertEquals("30d", usage.last30DaysLabel)
        assertEquals("local", usage.historySource)
    }

    @Test
    fun dailyUsagePointFromMapsModelsAndResets() {
        val day = DailyUsagePoint.from(
            JSONObject(
                """{"dayKey":"2026-10-14","totalTokens":5,"costUSD":0.25,
                   "models":[{"name":"gpt-5","costUSD":0.25,"totalTokens":5}],
                   "limitResets":[{"title":"Weekly","percentLeft":80,"resetsAt":"2026-10-20T00:00:00Z"}]}""",
            ),
        )
        assertEquals("2026-10-14", day.dayKey)
        assertEquals(5L, day.totalTokens)
        assertEquals(0.25, day.costUSD, 0.0)
        assertEquals(1, day.models.size)
        assertEquals("gpt-5", day.models[0].name)
        assertEquals(1, day.limitResets.size)
        assertEquals("Weekly", day.limitResets[0].title)
        assertEquals(80.0, day.limitResets[0].percentLeft!!, 0.0)
    }

    @Test
    fun dailyUsagePointWithoutDayKeyBecomesUnknown() {
        val day = DailyUsagePoint.from(JSONObject("""{"totalTokens":5}"""))
        assertEquals("unknown", day.dayKey)
    }

    @Test
    fun limitResetCreditsFromMapsItems() {
        val credits = LimitResetCredits.from(
            JSONObject(
                """{"availableCount":3,"nextExpiresAt":"2026-10-18T00:00:00Z",
                   "updatedAt":"2026-10-15T00:00:00Z",
                   "items":[{"id":"r1","status":"available","resetType":"weekly"}]}""",
            ),
        )
        assertEquals(3, credits.availableCount)
        assertEquals("2026-10-18T00:00:00Z", credits.nextExpiresAt)
        assertEquals("2026-10-15T00:00:00Z", credits.updatedAt)
        assertEquals(1, credits.items.size)
        assertEquals("r1", credits.items[0].id)
        assertEquals("available", credits.items[0].status)
        assertEquals("weekly", credits.items[0].resetType)
        assertEquals("Reset", credits.items[0].title)
    }

    @Test
    fun remoteDeviceFromDefaultsToUnknown() {
        val device = RemoteDevice.from(JSONObject("{}"))
        assertEquals("unknown", device.device)
        assertNull(device.updatedAt)
        assertNull(device.receivedAt)
    }

    @Test
    fun providerStatusFromMapsFields() {
        val status = ProviderStatus.from(
            JSONObject(
                """{"indicator":"minor","description":"Degraded","updatedAt":"2026-10-15T03:50:00Z"}""",
            ),
        )
        assertEquals("minor", status.indicator)
        assertEquals("Degraded", status.description)
        assertEquals("2026-10-15T03:50:00Z", status.updatedAt)
    }

    // ------------------------------------------------------------------
    // Top-level parsers
    // ------------------------------------------------------------------

    @Test
    fun parseHealthMapsStatusAndVersion() {
        val health = parseHealth(JSONObject("""{"status":"ok","version":"0.1.11"}"""))
        assertEquals("ok", health.status)
        assertEquals("0.1.11", health.version)
    }

    @Test
    fun parseHealthEmptyObject() {
        val health = parseHealth(JSONObject("{}"))
        assertEquals("unknown", health.status)
        assertNull(health.version)
    }

    @Test
    fun parseDeviceListMapsDeviceAndCorrupt() {
        val list = parseDeviceList(
            JSONObject(
                """{"ok":true,"devices":[{"device":"alpha","receivedAt":"2026-10-15T04:00:00Z",
                   "generatedAt":"2026-10-15T03:59:00Z","entryCount":2}],"corrupt":1}""",
            ),
        )
        assertEquals(1, list.devices.size)
        assertEquals("alpha", list.devices[0].device)
        assertEquals("2026-10-15T04:00:00Z", list.devices[0].receivedAt)
        assertEquals("2026-10-15T03:59:00Z", list.devices[0].generatedAt)
        assertEquals(2, list.devices[0].entryCount)
        assertEquals(1, list.corrupt)
    }

    @Test
    fun parseDeviceListEmptyObject() {
        val list = parseDeviceList(JSONObject("{}"))
        assertTrue(list.devices.isEmpty())
        assertEquals(0, list.corrupt)
    }

    @Test
    fun parseUsageSnapshotEmptyObject() {
        val snapshot = parseUsageSnapshot(JSONObject("{}"))
        assertTrue(snapshot.entries.isEmpty())
        assertNull(snapshot.generatedAt)
        assertNull(snapshot.requestedProvider)
        assertNull(snapshot.costError)
    }

    @Test
    fun parseUsageSnapshotCostError() {
        val snapshot = parseUsageSnapshot(JSONObject("""{"costError":"scan failed"}"""))
        assertEquals("scan failed", snapshot.costError)
    }

    private companion object {
        /** The merged payload the real server returns, copied verbatim. */
        const val FULL_PAYLOAD = """
        {
          "ok": true,
          "generatedAt": "2026-10-15T04:00:00Z",
          "requestedProvider": "all",
          "costError": null,
          "entries": [
            {
              "provider": "codex",
              "account": "user@example.com",
              "organization": "Slopfire",
              "plan": "Pro",
              "source": "cli",
              "siteUrl": null,
              "version": "0.1.11",
              "updatedAt": "2026-10-15T03:59:00Z",
              "status": {"indicator": "none", "description": "Operational",
                         "updatedAt": "2026-10-15T03:50:00Z"},
              "error": null,
              "stale": false,
              "rows": [
                {"id": "primary", "title": "Session", "percentLeft": 42.5,
                 "resetsAt": "2026-10-15T09:00:00Z", "windowMinutes": 300,
                 "pace": {"willLastToReset": true, "deltaPercent": -6, "expectedUsedPercent": 48,
                          "etaSeconds": null, "summary": null}},
                {"id": "secondary", "title": "Weekly", "percentLeft": 88,
                 "resetsAt": "2026-10-20T00:00:00Z", "windowMinutes": 10080, "pace": null}
              ],
              "creditsRemaining": 8.56,
              "limitResetCredits": {"availableCount": 2, "nextExpiresAt": "2026-10-18T00:00:00Z",
                "items": [{"id": "r1", "title": "Free reset", "description": "granted",
                           "status": "available", "resetType": "weekly",
                           "grantedAt": "2026-10-10T00:00:00Z", "expiresAt": "2026-10-18T00:00:00Z"}]},
              "codeReviewRemainingPercent": 100,
              "tokenUsage": {"sessionCostUSD": 1.23, "sessionTokens": 45678,
                             "last30DaysCostUSD": 12.3, "last30DaysTokens": 456789,
                             "provenance": "listPriceEstimate", "historyCoverageIsEstablished": true,
                             "currencyCode": "USD", "sessionLabel": "Today",
                             "last30DaysLabel": "30d", "historySource": "local"},
              "dailyUsage": [
                {"dayKey": "2026-10-14", "totalTokens": 1234, "costUSD": 0.5,
                 "historySource": "local", "historyFingerprint": "abc",
                 "models": [{"name": "gpt-5", "costUSD": 0.5, "totalTokens": 1234}],
                 "limitResets": [{"title": "Weekly", "percentLeft": 88,
                                  "resetsAt": "2026-10-20T00:00:00Z"}]}
              ],
              "historySource": "local",
              "remoteDevices": [{"device": "alpha", "updatedAt": "2026-10-15T03:59:00Z",
                                 "receivedAt": "2026-10-15T04:00:00Z"}]
            }
          ]
        }
        """
    }
}

/** Display formatting lives in the UI layer but is pure JVM code. */
class FmtFormattingTest {

    @Test
    fun moneyFormatsTwoDecimals() {
        assertEquals("\$1.23", Fmt.money(1.2345))
    }

    @Test
    fun moneyNullIsPlaceholder() {
        assertEquals("—", Fmt.money(null))
    }

    @Test
    fun moneyUsesCurrencySymbol() {
        assertEquals("€12.30", Fmt.money(12.3, "EUR"))
    }

    @Test
    fun tokensFormatsThousands() {
        assertEquals("1.2k", Fmt.tokens(1234))
    }

    @Test
    fun tokensFormatsMillions() {
        assertEquals("2.5M", Fmt.tokens(2_500_000))
    }

    @Test
    fun percentRoundsHalfUp() {
        assertEquals("43%", Fmt.percent(42.5))
    }

    @Test
    fun percentNullIsPlaceholder() {
        assertEquals("—", Fmt.percent(null))
    }

    @Test
    fun durationFormatsHoursAndMinutes() {
        assertEquals("2 h 3 m", Fmt.duration(7380.0))
    }

    @Test
    fun dayKeyIsNonBlankMonthDay() {
        val text = Fmt.dayKey("2026-10-14")
        assertTrue(text.isNotBlank())
        assertTrue(text.contains("14"))
    }

    @Test
    fun paceTextNullIsNull() {
        assertNull(Fmt.paceText(null))
    }
}
