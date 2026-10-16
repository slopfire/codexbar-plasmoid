package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.dp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.LocalSize
import androidx.glance.action.clickable
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.action.ActionCallback
import androidx.glance.appwidget.action.actionRunCallback
import androidx.glance.appwidget.provideContent
import androidx.glance.appwidget.updateAll
import androidx.glance.currentState
import androidx.glance.layout.Alignment
import androidx.glance.layout.Column
import androidx.glance.layout.Row
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.padding
import androidx.glance.text.Text
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured

/**
 * The multi-provider widget: the merged list, tightest first.
 *
 * A contained list uses **gaps** between rows rather than dividers (M3 Expressive
 * containment), and each row keeps the same anatomy as the app's provider card so
 * the phone and the desktop agree at a glance.
 */
class ProvidersWidget : GlanceAppWidget() {

    /**
     * `SizeMode.Exact` renders one UI per size the host may display, which is what
     * makes `LocalSize` the *real* size. Under the default `SizeMode.Single` it
     * reports the minimum size from the provider info, and the row count would
     * collapse to one.
     */
    override val sizeMode: SizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            val palette = WidgetTheme.palette(context, settings.colorMode)
            val all = snapshot?.providers.orEmpty().configured(settings)
            // A pinned row count wins; otherwise fit the height the host granted.
            val providers = all.take(
                if (settings.maxRows > WidgetSettings.AUTO_ROWS) {
                    settings.maxRows
                } else {
                    rowsThatFit(LocalSize.current.height)
                },
            )

            CompositionLocalProvider(LocalWidgetPalette provides palette) {
                GlanceTheme(colors = palette.colors) {
                    WidgetShell(
                        background = settings.background,
                        corners = settings.corners,
                    ) {
                        when {
                            snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                            providers.isEmpty() -> WidgetMessage(
                                title = if (settings.providers.isEmpty()) "No providers yet" else "No matching providers",
                                detail = snapshot.error
                                    ?: "Publish a snapshot from a CodexBar device.",
                            )

                            else -> {
                                if (settings.showHeader) {
                                    WidgetHeader(
                                        snapshot = snapshot,
                                        trailing = {
                                            Text(
                                                text = "Refresh",
                                                style = WidgetType.colored(
                                                    WidgetType.labelMedium,
                                                    GlanceTheme.colors.primary,
                                                ),
                                                modifier = GlanceModifier
                                                    .padding(start = 8.dp)
                                                    .clickable(actionRunCallback<RefreshCallback>()),
                                            )
                                        },
                                    )
                                    Spacer(modifier = GlanceModifier.height(8.dp))
                                }
                                providers.forEachIndexed { index, provider ->
                                    if (index > 0) {
                                        Spacer(modifier = GlanceModifier.height(ROW_GAP))
                                    }
                                    Row(
                                        modifier = GlanceModifier.fillMaxWidth(),
                                        verticalAlignment = Alignment.CenterVertically,
                                    ) {
                                        Column(modifier = GlanceModifier.defaultWeight()) {
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
        }
    }

    private companion object {
        val ROW_GAP = 10.dp

        /** Header line plus the shell's own vertical padding. */
        val CHROME_HEIGHT = 34.dp

        /** Provider title, bar and caption, measured from a rendered widget. */
        val ROW_HEIGHT = 46.dp

        /**
         * How many provider rows fit in [available] dp of height. Clamped so a
         * squashed widget still shows one row rather than an empty container.
         */
        fun rowsThatFit(available: androidx.compose.ui.unit.Dp): Int {
            val usable = (available - CHROME_HEIGHT).value
            if (usable <= 0f) return 1
            return (usable / ROW_HEIGHT.value).toInt().coerceIn(1, 6)
        }
    }
}

/** Re-fetches the merged view from the widget itself. */
class RefreshCallback : ActionCallback {
    override suspend fun onAction(
        context: Context,
        glanceId: GlanceId,
        parameters: androidx.glance.action.ActionParameters,
    ) {
        WidgetRefresher.refresh(context)
        WidgetRefresher.updateAllWidgets(context)
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
