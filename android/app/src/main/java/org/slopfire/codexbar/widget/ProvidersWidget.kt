package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured

/**
 * The providers widget: the provider closest to running out as a tonal hero card,
 * the rest as one tile each.
 *
 * The hero card takes whatever height the tiles do not need, so a tall slot gets
 * a bigger number instead of an empty band at the bottom. What fits is planned by
 * [WidgetLayout.plan] from the real size the host reports.
 */
class ProvidersWidget : GlanceAppWidget() {

    /**
     * `SizeMode.Exact` renders one UI per size the host may display, which is what
     * makes `LocalSize` the *real* size the bars and the plan are measured from.
     */
    override val sizeMode: SizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            val providers = snapshot?.providers.orEmpty().configured(settings)
            val hero = providers.firstOrNull()
            val style = UsageStyle.from(settings)

            WidgetFrame(settings) {
                when {
                    snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                    hero == null -> WidgetMessage(
                        title = if (settings.providers.isEmpty()) "No providers yet" else "No matching providers",
                        detail = snapshot.error ?: "Publish a snapshot from a CodexBar device.",
                    )

                    else -> {
                        val others = providers.drop(1)
                        val plan = WidgetLayout.plan(
                            innerHeight = widgetInnerHeight(),
                            barHeight = style.barHeight.value,
                            marker = style.showMarker,
                            siblingCount = siblingsOf(hero).size,
                            tileWindows = others.map { tileWindowsOf(it).size },
                        )
                        HeroCard(
                            provider = hero,
                            style = style,
                            plan = plan,
                            modifier = GlanceModifier.defaultWeight(),
                            fill = true,
                        )
                        others.take(plan.tiles).forEach { ProviderTile(it, style, expanded = plan.expandTiles) }
                    }
                }
            }
        }
    }
}

class ProvidersWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = ProvidersWidget()

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray,
    ) {
        super.onUpdate(context, appWidgetManager, appWidgetIds)
    }
}
