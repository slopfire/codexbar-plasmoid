package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.glance.GlanceId
import androidx.glance.GlanceTheme
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured

/**
 * The one-provider widget: the window closest to running out, with its remaining
 * percent as the emphasized headline.
 *
 * This is the hero surface — one number, one bar, one verdict. The provider is
 * configurable; by default it follows whichever limit is tightest.
 */
class CompactWidget : GlanceAppWidget() {

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            val palette = WidgetTheme.palette(context, settings.colorMode)
            val providers = snapshot?.providers.orEmpty().configured(settings)
            val provider = providers.pickHero(settings)

            CompositionLocalProvider(LocalWidgetPalette provides palette) {
                GlanceTheme(colors = palette.colors) {
                    WidgetShell(
                        background = settings.background,
                        corners = settings.corners,
                    ) {
                        when {
                            snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                            provider == null -> WidgetMessage(
                                title = "No providers yet",
                                detail = snapshot.error
                                    ?: "Publish a snapshot from a CodexBar device.",
                            )

                            else -> {
                                LimitLine(
                                    provider = provider,
                                    compact = true,
                                    showCaption = settings.showCaptions,
                                    showBadge = settings.showBadges,
                                    barHeight = WidgetTheme.barHeight(settings.barHeight),
                                    paceStyle = settings.paceStyle,
                                )
                            }
                        }
                    }
                }
            }
        }
    }
}

/**
 * Picks the provider a glance should land on: the configured one when the user
 * pinned a provider, otherwise the tightest window that actually reported limits.
 */
internal fun List<WidgetProvider>.pickHero(settings: WidgetSettings): WidgetProvider? =
    firstOrNull { it.id == settings.heroProvider }
        ?: filter { it.rows.isNotEmpty() }.minByOrNull { it.tightest?.percentLeft ?: 100.0 }
        ?: firstOrNull()

class CompactWidgetReceiver : GlanceAppWidgetReceiver() {
    override val glanceAppWidget: GlanceAppWidget = CompactWidget()

    override fun onUpdate(
        context: Context,
        appWidgetManager: AppWidgetManager,
        appWidgetIds: IntArray,
    ) {
        // Rendering fetches when the cache is stale (see WidgetRefresher.readFresh),
        // so the receiver only needs to trigger the re-render.
        super.onUpdate(context, appWidgetManager, appWidgetIds)
    }
}
