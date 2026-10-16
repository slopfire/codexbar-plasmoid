package org.slopfire.codexbar.data.model

import org.json.JSONArray
import org.json.JSONObject

/**
 * Tolerant JSON helpers. The sync server merges snapshots published by several
 * devices and CodexBar CLI versions, so every field is treated as optional and
 * a type mismatch never throws.
 */
internal fun JSONObject.stringOrNull(key: String): String? {
    if (!has(key) || isNull(key)) return null
    val value = optString(key, "")
    return value.ifBlank { null }
}

internal fun JSONObject.doubleOrNull(key: String): Double? {
    if (!has(key) || isNull(key)) return null
    val value = opt(key)
    if (value is Number) return value.toDouble()
    return value?.toString()?.toDoubleOrNull()
}

internal fun JSONObject.longOrNull(key: String): Long? {
    if (!has(key) || isNull(key)) return null
    val value = opt(key)
    if (value is Number) return value.toLong()
    return value?.toString()?.toDoubleOrNull()?.toLong()
}

internal fun JSONObject.intOrNull(key: String): Int? = longOrNull(key)?.toInt()

internal fun JSONObject.booleanOrNull(key: String): Boolean? {
    if (!has(key) || isNull(key)) return null
    val value = opt(key)
    return when (value) {
        is Boolean -> value
        is Number -> value.toInt() != 0
        else -> value?.toString()?.toBooleanStrictOrNull()
    }
}

internal fun JSONObject.objectOrNull(key: String): JSONObject? {
    if (!has(key) || isNull(key)) return null
    return optJSONObject(key)
}

internal fun JSONObject.arrayOrNull(key: String): JSONArray? {
    if (!has(key) || isNull(key)) return null
    return optJSONArray(key)
}

/** Iterates an array field, skipping anything that is not a JSON object. */
internal inline fun JSONObject.objects(key: String, transform: (JSONObject) -> Unit) {
    val array = arrayOrNull(key) ?: return
    for (index in 0 until array.length()) {
        array.optJSONObject(index)?.let(transform)
    }
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

/** Parses `GET /v1/usage`. Throws only on malformed JSON, never on missing fields. */
fun parseUsageSnapshot(json: JSONObject): UsageSnapshot = UsageSnapshot(
    generatedAt = json.stringOrNull("generatedAt"),
    requestedProvider = json.stringOrNull("requestedProvider"),
    costError = json.stringOrNull("costError"),
    entries = buildList {
        val entries = json.arrayOrNull("entries") ?: JSONArray()
        for (index in 0 until entries.length()) {
            entries.optJSONObject(index)?.let { add(ProviderEntry.from(it)) }
        }
    },
)

fun ProviderEntry.Companion.from(json: JSONObject): ProviderEntry = ProviderEntry(
    provider = json.stringOrNull("provider") ?: "unknown",
    account = json.stringOrNull("account"),
    organization = json.stringOrNull("organization"),
    plan = json.stringOrNull("plan"),
    source = json.stringOrNull("source"),
    siteUrl = json.stringOrNull("siteUrl"),
    version = json.stringOrNull("version"),
    updatedAt = json.stringOrNull("updatedAt"),
    status = json.objectOrNull("status")?.let { ProviderStatus.from(it) },
    error = json.errorMessage(),
    stale = json.booleanOrNull("stale") ?: false,
    staleSince = json.stringOrNull("staleSince"),
    lastAttemptError = json.stringOrNull("lastAttemptError"),
    usageUnavailable = json.usageUnavailableMessage(),
    rows = buildList {
        json.objects("rows") { row -> add(UsageRow.from(row)) }
    },
    creditsRemaining = json.doubleOrNull("creditsRemaining"),
    limitResetCredits = json.objectOrNull("limitResetCredits")?.let { LimitResetCredits.from(it) },
    codeReviewRemainingPercent = json.doubleOrNull("codeReviewRemainingPercent"),
    tokenUsage = json.objectOrNull("tokenUsage")?.let { TokenUsage.from(it) },
    dailyUsage = buildList {
        json.objects("dailyUsage") { day -> add(DailyUsagePoint.from(day)) }
    },
    historySource = json.stringOrNull("historySource"),
    remoteDevices = buildList {
        json.objects("remoteDevices") { device -> add(RemoteDevice.from(device)) }
    },
)

/** `error` is either a string or `{ message }` depending on the publisher. */
private fun JSONObject.errorMessage(): String? {
    if (!has("error") || isNull("error")) return null
    val raw = opt("error")
    return when (raw) {
        is JSONObject -> raw.stringOrNull("message") ?: "provider error"
        is String -> raw.ifBlank { null }
        else -> raw?.toString()?.ifBlank { null } ?: "provider error"
    }
}

private fun JSONObject.usageUnavailableMessage(): String? {
    if (!has("usageUnavailable") || isNull("usageUnavailable")) return null
    val raw = opt("usageUnavailable")
    return when (raw) {
        is JSONObject -> raw.stringOrNull("message") ?: "usage unavailable"
        is String -> raw.ifBlank { null }
        else -> raw?.toString()?.ifBlank { null } ?: "unavailable"
    }
}

fun UsageRow.Companion.from(json: JSONObject): UsageRow = UsageRow(
    id = json.stringOrNull("id") ?: json.stringOrNull("title") ?: "usage",
    title = json.stringOrNull("title") ?: "Usage",
    percentLeft = json.doubleOrNull("percentLeft") ?: 0.0,
    resetsAt = json.stringOrNull("resetsAt"),
    windowMinutes = json.doubleOrNull("windowMinutes"),
    pace = json.objectOrNull("pace")?.let { Pace.from(it) },
)

fun Pace.Companion.from(json: JSONObject): Pace = Pace(
    willLastToReset = json.booleanOrNull("willLastToReset"),
    deltaPercent = json.doubleOrNull("deltaPercent"),
    expectedUsedPercent = json.doubleOrNull("expectedUsedPercent"),
    etaSeconds = json.doubleOrNull("etaSeconds"),
    summary = json.stringOrNull("summary"),
)

fun TokenUsage.Companion.from(json: JSONObject): TokenUsage = TokenUsage(
    sessionCostUSD = json.doubleOrNull("sessionCostUSD"),
    sessionTokens = json.longOrNull("sessionTokens"),
    last30DaysCostUSD = json.doubleOrNull("last30DaysCostUSD"),
    last30DaysTokens = json.longOrNull("last30DaysTokens"),
    provenance = json.stringOrNull("provenance"),
    historyCoverageIsEstablished = json.booleanOrNull("historyCoverageIsEstablished"),
    currencyCode = json.stringOrNull("currencyCode"),
    sessionLabel = json.stringOrNull("sessionLabel"),
    last30DaysLabel = json.stringOrNull("last30DaysLabel"),
    historySource = json.stringOrNull("historySource"),
)

fun DailyUsagePoint.Companion.from(json: JSONObject): DailyUsagePoint = DailyUsagePoint(
    dayKey = json.stringOrNull("dayKey") ?: "",
    totalTokens = json.longOrNull("totalTokens") ?: 0L,
    costUSD = json.doubleOrNull("costUSD") ?: 0.0,
    models = buildList {
        json.objects("models") { model -> add(ModelUsage.from(model)) }
    },
    historySource = json.stringOrNull("historySource"),
    historyFingerprint = json.stringOrNull("historyFingerprint") ?: json.stringOrNull("fingerprint"),
    limitResets = buildList {
        json.objects("limitResets") { reset -> add(LimitReset.from(reset)) }
    },
).takeIf { it.dayKey.isNotBlank() } ?: DailyUsagePoint(dayKey = "unknown")

fun ModelUsage.Companion.from(json: JSONObject): ModelUsage = ModelUsage(
    name = json.stringOrNull("name") ?: "unknown",
    costUSD = json.doubleOrNull("costUSD") ?: 0.0,
    totalTokens = json.longOrNull("totalTokens") ?: 0L,
)

fun LimitReset.Companion.from(json: JSONObject): LimitReset = LimitReset(
    title = json.stringOrNull("title") ?: "Limit",
    percentLeft = json.doubleOrNull("percentLeft"),
    resetsAt = json.stringOrNull("resetsAt"),
)

fun LimitResetCredits.Companion.from(json: JSONObject): LimitResetCredits = LimitResetCredits(
    availableCount = json.intOrNull("availableCount") ?: 0,
    nextExpiresAt = json.stringOrNull("nextExpiresAt"),
    updatedAt = json.stringOrNull("updatedAt"),
    items = buildList {
        json.objects("items") { credit -> add(ResetCredit.from(credit)) }
    },
)

fun ResetCredit.Companion.from(json: JSONObject): ResetCredit = ResetCredit(
    id = json.stringOrNull("id"),
    title = json.stringOrNull("title") ?: "Reset",
    description = json.stringOrNull("description"),
    status = json.stringOrNull("status") ?: "unknown",
    resetType = json.stringOrNull("resetType"),
    grantedAt = json.stringOrNull("grantedAt"),
    expiresAt = json.stringOrNull("expiresAt"),
)

fun RemoteDevice.Companion.from(json: JSONObject): RemoteDevice = RemoteDevice(
    device = json.stringOrNull("device") ?: "unknown",
    updatedAt = json.stringOrNull("updatedAt"),
    receivedAt = json.stringOrNull("receivedAt"),
)

fun ProviderStatus.Companion.from(json: JSONObject): ProviderStatus = ProviderStatus(
    indicator = json.stringOrNull("indicator"),
    description = json.stringOrNull("description"),
    updatedAt = json.stringOrNull("updatedAt"),
)

/** Parses `GET /health`. */
fun parseHealth(json: JSONObject): HealthInfo = HealthInfo(
    status = json.stringOrNull("status") ?: "unknown",
    version = json.stringOrNull("version"),
)

/** Parses `GET /v1/devices`. */
fun parseDeviceList(json: JSONObject): DeviceList = DeviceList(
    corrupt = json.intOrNull("corrupt") ?: 0,
    devices = buildList {
        json.objects("devices") { device -> add(DeviceRecord.from(device)) }
    },
)

fun DeviceRecord.Companion.from(json: JSONObject): DeviceRecord = DeviceRecord(
    device = json.stringOrNull("device") ?: "unknown",
    receivedAt = json.stringOrNull("receivedAt"),
    generatedAt = json.stringOrNull("generatedAt"),
    entryCount = json.intOrNull("entryCount") ?: 0,
)
