package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.ui.unit.dp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.layout.width
import androidx.glance.text.Text
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.theme.providerLabel

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
            val providers = snapshot?.providers.orEmpty().configured(settings)
            val totals = SpendTotals.from(providers)

            WidgetFrame(settings) {
                when {
                    snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                    !totals.hasAny -> WidgetMessage(
                        title = "No cost data yet",
                        detail = snapshot.error ?: "No device has published a local cost scan yet.",
                    )

                    else -> SpendContent(totals, showBadges = settings.showBadges)
                }
            }
        }
    }
}

/** Two tonal figures, the top model, and the per-provider breakdown that fits. */
@Composable
private fun SpendContent(totals: SpendTotals, showBadges: Boolean) {
    val surface = LocalWidgetSurface.current
    Row(modifier = GlanceModifier.fillMaxWidth()) {
        MetricTile(
            label = "Today",
            value = Fmt.money(totals.sessionCost, totals.currency),
            caption = totals.sessionTokens?.let { "${Fmt.tokens(it)} tokens" },
            tone = WidgetTone.Primary,
            modifier = GlanceModifier.defaultWeight(),
        )
        Spacer(modifier = GlanceModifier.width(6.dp))
        MetricTile(
            label = "30 days",
            value = Fmt.money(totals.cost30d, totals.currency),
            caption = totals.tokens30d?.let { "${Fmt.tokens(it)} tokens" },
            tone = WidgetTone.Neutral,
            modifier = GlanceModifier.defaultWeight(),
        )
    }
    totals.topModel?.let { model ->
        Spacer(modifier = GlanceModifier.height(8.dp))
        Text(
            text = "Top model  $model" +
                (totals.topModelCost?.let { " · ${Fmt.money(it, totals.currency)}" } ?: ""),
            style = WidgetType.colored(WidgetType.bodySmall, surface.onShellVariant),
            maxLines = 1,
            modifier = GlanceModifier.padding(horizontal = 4.dp),
        )
    }
    // One child for the breakdown: Glance keeps at most ten per column.
    Column(modifier = GlanceModifier.fillMaxWidth()) {
        totals.byProvider(widgetInnerHeight()).forEach { line ->
            Row(
                modifier = GlanceModifier.fillMaxWidth().padding(start = 4.dp, end = 4.dp, top = 6.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                if (showBadges) {
                    ProviderBadge(line.id, size = 20.dp)
                    Spacer(modifier = GlanceModifier.width(8.dp))
                }
                Text(
                    text = line.label,
                    style = WidgetType.colored(WidgetType.titleSmall, surface.onShell),
                    maxLines = 1,
                    modifier = GlanceModifier.defaultWeight(),
                )
                Text(
                    text = line.value,
                    style = WidgetType.colored(WidgetType.labelLarge, surface.onShellVariant),
                    maxLines = 1,
                )
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

    /**
     * Per-provider 30-day cost, biggest first, limited to what fits in
     * [innerHeight] dp. ~116dp goes to the metric tiles and the top-model line;
     * each breakdown row costs ~28dp.
     */
    fun byProvider(innerHeight: Float): List<SpendLine> {
        if (innerHeight.isNaN()) return emptyList()
        val limit = ((innerHeight - 116f) / 28f).toInt().coerceIn(0, 4)
        if (limit == 0) return emptyList()
        return providers
            .filter { (it.cost30d ?: 0.0) > 0.0 }
            .sortedByDescending { it.cost30d ?: 0.0 }
            .take(limit)
            .map { SpendLine(id = it.id, label = providerLabel(it.id), value = Fmt.money(it.cost30d, it.currency ?: currency)) }
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
internal data class SpendLine(val id: String, val label: String, val value: String)

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
