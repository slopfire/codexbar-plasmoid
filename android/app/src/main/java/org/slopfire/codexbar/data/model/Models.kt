package org.slopfire.codexbar.data.model

/**
 * The merged sync-server payload, mirroring the JSON shape the CodexBar helper
 * and `GET /v1/usage` return. Everything is nullable-friendly: the server merges
 * snapshots published by different devices and CLI versions, so any field can be
 * absent.
 */
data class UsageSnapshot(
    val generatedAt: String?,
    val requestedProvider: String?,
    val entries: List<ProviderEntry>,
    val costError: String?,
)

/** One provider+account row after the server merged every device's snapshot. */
data class ProviderEntry(
    val provider: String,
    val account: String? = null,
    val organization: String? = null,
    val plan: String? = null,
    val source: String? = null,
    val siteUrl: String? = null,
    val version: String? = null,
    val updatedAt: String? = null,
    val status: ProviderStatus? = null,
    val error: String? = null,
    val stale: Boolean = false,
    val staleSince: String? = null,
    val lastAttemptError: String? = null,
    val usageUnavailable: String? = null,
    val rows: List<UsageRow> = emptyList(),
    val creditsRemaining: Double? = null,
    val limitResetCredits: LimitResetCredits? = null,
    val codeReviewRemainingPercent: Double? = null,
    val tokenUsage: TokenUsage? = null,
    val dailyUsage: List<DailyUsagePoint> = emptyList(),
    val historySource: String? = null,
    val remoteDevices: List<RemoteDevice> = emptyList(),
) {
    /** Stable identity used for selection and navigation. */
    val key: String get() = "$provider|${account.orEmpty()}"

    /** True when the entry carries no limits, no cost and no usable status. */
    val isEmpty: Boolean
        get() = rows.isEmpty() && creditsRemaining == null && tokenUsage == null && status?.indicator.isNullOrBlank()

    companion object

}

/** One rate-limit window, e.g. the 5-hour session or the weekly limit. */
data class UsageRow(
    val id: String,
    val title: String,
    val percentLeft: Double,
    val resetsAt: String? = null,
    val windowMinutes: Double? = null,
    val pace: Pace? = null,
) {
    companion object
}

/** CodexBar's pace report: will the remaining budget last until the reset? */
data class Pace(
    val willLastToReset: Boolean? = null,
    val deltaPercent: Double? = null,
    val expectedUsedPercent: Double? = null,
    val etaSeconds: Double? = null,
    val summary: String? = null,
) {
    companion object
}

/** Local token/cost totals for one provider+account. */
data class TokenUsage(
    val sessionCostUSD: Double? = null,
    val sessionTokens: Long? = null,
    val last30DaysCostUSD: Double? = null,
    val last30DaysTokens: Long? = null,
    val provenance: String? = null,
    val historyCoverageIsEstablished: Boolean? = null,
    val currencyCode: String? = null,
    val sessionLabel: String? = null,
    val last30DaysLabel: String? = null,
    val historySource: String? = null,
) {
    companion object
}

/** One day of the trailing history window. */
data class DailyUsagePoint(
    val dayKey: String,
    val totalTokens: Long = 0,
    val costUSD: Double = 0.0,
    val models: List<ModelUsage> = emptyList(),
    val historySource: String? = null,
    val historyFingerprint: String? = null,
    val limitResets: List<LimitReset> = emptyList(),
) {
    companion object
}

data class ModelUsage(
    val name: String,
    val costUSD: Double = 0.0,
    val totalTokens: Long = 0,
) {
    companion object
}

/** An unused rate-limit reset that falls on this day. */
data class LimitReset(
    val title: String,
    val percentLeft: Double? = null,
    val resetsAt: String? = null,
) {
    companion object
}

/** Codex-style free rate-limit reset credits. */
data class LimitResetCredits(
    val availableCount: Int = 0,
    val nextExpiresAt: String? = null,
    val items: List<ResetCredit> = emptyList(),
    val updatedAt: String? = null,
) {
    companion object
}

data class ResetCredit(
    val id: String? = null,
    val title: String = "Reset",
    val description: String? = null,
    val status: String = "unknown",
    val resetType: String? = null,
    val grantedAt: String? = null,
    val expiresAt: String? = null,
) {
    companion object
}

/** A device that contributed to a merged entry. */
data class RemoteDevice(
    val device: String,
    val updatedAt: String? = null,
    val receivedAt: String? = null,
) {
    companion object
}

/** Provider status page indicator. */
data class ProviderStatus(
    val indicator: String? = null,
    val description: String? = null,
    val updatedAt: String? = null,
) {
    companion object
}

/** `GET /health` — never needs a token. */
data class HealthInfo(
    val status: String,
    val version: String?,
)

/** One row of `GET /v1/devices`. */
data class DeviceRecord(
    val device: String,
    val receivedAt: String? = null,
    val generatedAt: String? = null,
    val entryCount: Int = 0,
) {
    companion object
}

data class DeviceList(
    val devices: List<DeviceRecord>,
    val corrupt: Int = 0,
)

/** A failure talking to the sync server, with the HTTP status when there was one. */
class SyncServerException(
    val statusCode: Int,
    override val message: String,
    cause: Throwable? = null,
) : Exception(message, cause)
