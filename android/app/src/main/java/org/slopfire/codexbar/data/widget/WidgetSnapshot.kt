package org.slopfire.codexbar.data.widget

import org.json.JSONArray
import org.json.JSONObject
import org.slopfire.codexbar.data.model.Pace
import org.slopfire.codexbar.data.model.ProviderEntry
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.data.model.from

/**
 * The trimmed payload a home screen widget renders.
 *
 * Widgets cannot afford the whole `ProviderEntry` graph, and they must render
 * from a cached value because `GlanceAppWidget.provideGlance` runs on every
 * update without a network round trip. This is that cache: a few numbers per
 * provider, serialized to JSON in app-private storage.
 *
 * Rate-limit rows reuse [UsageRow] and [Pace] so the widget and the app format
 * remaining percent, reset countdowns and pace verdicts through the same code.
 */
data class WidgetProvider(
    val id: String,
    val label: String,
    val account: String?,
    val rows: List<UsageRow>,
    val creditsRemaining: Double?,
    val currency: String?,
    val sessionCost: Double?,
    val sessionTokens: Long?,
    val cost30d: Double?,
    val tokens30d: Long?,
    val topModel: String?,
    val topModelCost: Double?,
    val stale: Boolean,
    val hasError: Boolean,
) {
    /** The window closest to running out — the number a glance should find first. */
    val tightest: UsageRow? get() = rows.minByOrNull { it.percentLeft }

    companion object {
        /** Trims a server entry down to what a widget renders. */
        fun from(entry: ProviderEntry): WidgetProvider {
            val usage = entry.tokenUsage
            val topModel = entry.dailyUsage
                .flatMap { it.models }
                .groupBy { it.name }
                .mapValues { (_, models) -> models.sumOf { it.costUSD } }
                .maxByOrNull { it.value }
            return WidgetProvider(
                id = entry.provider,
                label = entry.provider,
                account = entry.account ?: entry.organization,
                rows = entry.rows,
                creditsRemaining = entry.creditsRemaining,
                currency = usage?.currencyCode,
                sessionCost = usage?.sessionCostUSD,
                sessionTokens = usage?.sessionTokens,
                cost30d = usage?.last30DaysCostUSD,
                tokens30d = usage?.last30DaysTokens,
                topModel = topModel?.key,
                topModelCost = topModel?.value,
                stale = entry.stale,
                hasError = entry.error != null || entry.usageUnavailable != null,
            )
        }

        /** Rehydrates the cached payload. */
        fun from(json: JSONObject): WidgetProvider = WidgetProvider(
            id = json.optString("id").ifBlank { "unknown" },
            label = json.optString("label").ifBlank { "unknown" },
            account = json.optNullableString("account"),
            rows = buildList {
                val rows = json.optJSONArray("rows") ?: JSONArray()
                for (index in 0 until rows.length()) {
                    rows.optJSONObject(index)?.let { add(UsageRow.from(it)) }
                }
            },
            creditsRemaining = json.optNullableDouble("creditsRemaining"),
            currency = json.optNullableString("currency"),
            sessionCost = json.optNullableDouble("sessionCost"),
            sessionTokens = json.optNullableLong("sessionTokens"),
            cost30d = json.optNullableDouble("cost30d"),
            tokens30d = json.optNullableLong("tokens30d"),
            topModel = json.optNullableString("topModel"),
            topModelCost = json.optNullableDouble("topModelCost"),
            stale = json.optBoolean("stale", false),
            hasError = json.optBoolean("hasError", false),
        )
    }

    fun toJson(): JSONObject = JSONObject().apply {
        put("id", id)
        put("label", label)
        put("account", account ?: JSONObject.NULL)
        put("creditsRemaining", creditsRemaining ?: JSONObject.NULL)
        put("currency", currency ?: JSONObject.NULL)
        put("sessionCost", sessionCost ?: JSONObject.NULL)
        put("sessionTokens", sessionTokens ?: JSONObject.NULL)
        put("cost30d", cost30d ?: JSONObject.NULL)
        put("tokens30d", tokens30d ?: JSONObject.NULL)
        put("topModel", topModel ?: JSONObject.NULL)
        put("topModelCost", topModelCost ?: JSONObject.NULL)
        put("stale", stale)
        put("hasError", hasError)
        put(
            "rows",
            JSONArray().apply {
                rows.forEach { row -> put(row.toJson()) }
            },
        )
    }
}

private fun UsageRow.toJson(): JSONObject = JSONObject().apply {
    put("id", id)
    put("title", title)
    put("percentLeft", percentLeft)
    put("resetsAt", resetsAt ?: JSONObject.NULL)
    put("windowMinutes", windowMinutes ?: JSONObject.NULL)
    put(
        "pace",
        pace?.let { pace ->
            JSONObject().apply {
                put("willLastToReset", pace.willLastToReset ?: JSONObject.NULL)
                put("deltaPercent", pace.deltaPercent ?: JSONObject.NULL)
                put("expectedUsedPercent", pace.expectedUsedPercent ?: JSONObject.NULL)
                put("etaSeconds", pace.etaSeconds ?: JSONObject.NULL)
                put("summary", pace.summary ?: JSONObject.NULL)
            }
        } ?: JSONObject.NULL,
    )
}

/** The cached widget payload: every provider plus how fresh it is. */
data class WidgetSnapshot(
    val host: String,
    val generatedAtMillis: Long,
    val providers: List<WidgetProvider>,
    val error: String?,
) {
    val isUsable: Boolean get() = providers.isNotEmpty()

    fun toJson(): String = JSONObject().apply {
        put("host", host)
        put("generatedAt", generatedAtMillis)
        put("error", error ?: JSONObject.NULL)
        put(
            "providers",
            JSONArray().apply {
                providers.forEach { provider -> put(provider.toJson()) }
            },
        )
    }.toString()

    companion object {
        fun from(host: String, providers: List<WidgetProvider>): WidgetSnapshot = WidgetSnapshot(
            host = host,
            generatedAtMillis = System.currentTimeMillis(),
            providers = providers,
            error = null,
        )

        fun failure(host: String, message: String, previous: WidgetSnapshot?): WidgetSnapshot =
            WidgetSnapshot(
                host = host,
                generatedAtMillis = previous?.generatedAtMillis ?: System.currentTimeMillis(),
                providers = previous?.providers.orEmpty(),
                error = message,
            )

        fun parse(raw: String?): WidgetSnapshot? {
            if (raw.isNullOrBlank()) return null
            return runCatching {
                val json = JSONObject(raw)
                WidgetSnapshot(
                    host = json.optString("host"),
                    generatedAtMillis = json.optLong("generatedAt", 0L),
                    error = json.optNullableString("error"),
                    providers = buildList {
                        val providers = json.optJSONArray("providers") ?: JSONArray()
                        for (index in 0 until providers.length()) {
                            providers.optJSONObject(index)?.let { add(WidgetProvider.from(it)) }
                        }
                    },
                )
            }.getOrNull()
        }
    }
}

private fun JSONObject.optNullableString(key: String): String? {
    if (!has(key) || isNull(key)) return null
    return optString(key, "").ifBlank { null }
}

private fun JSONObject.optNullableDouble(key: String): Double? {
    if (!has(key) || isNull(key)) return null
    val value = opt(key)
    return if (value is Number) value.toDouble() else value?.toString()?.toDoubleOrNull()
}

private fun JSONObject.optNullableLong(key: String): Long? {
    if (!has(key) || isNull(key)) return null
    val value = opt(key)
    return if (value is Number) value.toLong() else value?.toString()?.toDoubleOrNull()?.toLong()
}
