package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.LocalSize
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.width
import androidx.glance.text.Text
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured
import org.slopfire.codexbar.ui.common.Fmt

/**
 * The spend widget: local token cost for today and the trailing 30 days, summed
 * across the providers the sync server merged, plus the model that cost the most.
 *
 * Cost is a background scan on the publishing device, so the caption names the
 * provenance instead of implying the number is live.
 */
class SpendWidget : GlanceAppWidget() {

    /** See [ProvidersWidget.sizeMode]: only Exact reports the real size. */
    override val sizeMode: SizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            val palette = WidgetTheme.palette(context, settings.colorMode)
            val providers = snapshot?.providers.orEmpty().configured(settings)
            val totals = SpendTotals.from(providers)

            CompositionLocalProvider(LocalWidgetPalette provides palette) {
                GlanceTheme(colors = palette.colors) {
                    WidgetShell(
                        background = settings.background,
                        corners = settings.corners,
                    ) {
                        when {
                            snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                            !totals.hasAny -> WidgetMessage(
                                title = "No cost data yet",
                                detail = snapshot.error
                                    ?: "No device has published a local cost scan yet.",
                            )

                            else -> {
                                if (settings.showHeader) {
                                    WidgetHeader(snapshot)
                                    Spacer(modifier = GlanceModifier.height(8.dp))
                                }
                                Row(modifier = GlanceModifier.fillMaxWidth()) {
                                    MetricBlock(
                                        label = "Today",
                                        value = Fmt.money(totals.sessionCost, totals.currency),
                                        caption = totals.sessionTokens?.let { "${Fmt.tokens(it)} tokens" },
                                        modifier = GlanceModifier.defaultWeight(),
                                    )
                                    Spacer(modifier = GlanceModifier.width(12.dp))
                                    MetricBlock(
                                        label = "30 days",
                                        value = Fmt.money(totals.cost30d, totals.currency),
                                        caption = totals.tokens30d?.let { "${Fmt.tokens(it)} tokens" },
                                        modifier = GlanceModifier.defaultWeight(),
                                    )
                                }
                                totals.topModel?.let { model ->
                                    Spacer(modifier = GlanceModifier.height(6.dp))
                                    WidgetCaption(
                                        "Top model: $model" +
                                            (totals.topModelCost?.let { " · ${Fmt.money(it, totals.currency)}" }
                                                ?: ""),
                                        GlanceTheme.colors.onSurfaceVariant,
                                    )
                                }
                                val breakdown = totals.byProvider(LocalSize.current.height)
                                if (breakdown.isNotEmpty()) {
                                    Spacer(modifier = GlanceModifier.height(10.dp))
                                    Text(
                                        text = "By provider",
                                        style = WidgetType.colored(
                                            WidgetType.labelMedium,
                                            GlanceTheme.colors.onSurfaceVariant,
                                        ),
                                    )
                                    Spacer(modifier = GlanceModifier.height(4.dp))
                                    breakdown.forEach { line ->
                                        Row(
                                            modifier = GlanceModifier.fillMaxWidth(),
                                            verticalAlignment = Alignment.CenterVertically,
                                        ) {
                                            Text(
                                                text = line.label,
                                                style = WidgetType.colored(
                                                    WidgetType.titleSmall,
                                                    GlanceTheme.colors.onSurface,
                                                ),
                                                maxLines = 1,
                                                modifier = GlanceModifier.defaultWeight(),
                                            )
                                            Text(
                                                text = line.value,
                                                style = WidgetType.colored(
                                                    WidgetType.labelMedium,
                                                    GlanceTheme.colors.onSurfaceVariant,
                                                ),
                                                maxLines = 1,
                                            )
                                        }
                                        Spacer(modifier = GlanceModifier.height(4.dp))
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/** Summed spend across the providers this widget is configured to show. */
internal data class SpendTotals(
    val sessionCost: Double?,
    val sessionTokens: Long?,
    val cost30d: Double?,
    val tokens30d: Long?,
    val topModel: String?,
    val topModelCost: Double?,
    val currency: String?,
    /** Kept so the per-provider breakdown can follow the granted widget height. */
    val providers: List<WidgetProvider>,
) {
    val hasAny: Boolean
        get() = sessionCost != null || cost30d != null || sessionTokens != null || tokens30d != null

    /** Per-provider 30-day cost, biggest first, limited to what fits. */
    fun byProvider(available: Dp): List<SpendLine> {
        val limit = ((available.value - 84f) / 20f).toInt().coerceIn(0, 4)
        if (limit == 0) return emptyList()
        return providers
            .filter { (it.cost30d ?: 0.0) > 0.0 }
            .sortedByDescending { it.cost30d ?: 0.0 }
            .take(limit)
            .map { SpendLine(label = it.label, value = Fmt.money(it.cost30d, it.currency ?: currency)) }
    }

    companion object {
        fun from(providers: List<WidgetProvider>): SpendTotals {
            val top = providers
                .filter { it.topModel != null }
                .maxByOrNull { it.topModelCost ?: 0.0 }
            return SpendTotals(
                sessionCost = providers.sumOfNullable { it.sessionCost },
                sessionTokens = providers.sumOfNullableLong { it.sessionTokens },
                cost30d = providers.sumOfNullable { it.cost30d },
                tokens30d = providers.sumOfNullableLong { it.tokens30d },
                topModel = top?.topModel,
                topModelCost = top?.topModelCost,
                currency = providers.firstNotNullOfOrNull { it.currency } ?: "USD",
                providers = providers,
            )
        }
    }
}

/** One provider's 30-day spend, as shown in the breakdown. */
internal data class SpendLine(val label: String, val value: String)

private inline fun List<WidgetProvider>.sumOfNullable(selector: (WidgetProvider) -> Double?): Double? {
    val values = mapNotNull(selector)
    return if (values.isEmpty()) null else values.sum()
}

private inline fun List<WidgetProvider>.sumOfNullableLong(selector: (WidgetProvider) -> Long?): Long? {
    val values = mapNotNull(selector)
    return if (values.isEmpty()) null else values.sum()
}

class SpendWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = SpendWidget()

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray,
    ) {
        super.onUpdate(context, appWidgetManager, appWidgetIds)
    }
}
