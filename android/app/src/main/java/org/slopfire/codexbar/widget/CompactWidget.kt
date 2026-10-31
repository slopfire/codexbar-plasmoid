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
import androidx.glance.layout.fillMaxSize
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import androidx.glance.layout.width
import androidx.glance.text.Text
import org.slopfire.codexbar.data.widget.WidgetProvider
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.theme.providerLabel

/**
 * The one-provider widget: the window closest to running out.
 *
 * At one cell tall it is a single line — badge, name and window, the percentage —
 * over a bar. Given two cells or more it becomes the same tonal hero card the
 * providers widget leads with, listing the provider's other windows inside it.
 */
class CompactWidget : GlanceAppWidget() {

    /** See [ProvidersWidget.sizeMode]: only Exact reports the real size. */
    override val sizeMode: SizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            // The usage widget picks one provider itself, so it ignores the
            // providers widget's filter and order.
            val provider = snapshot?.providers.orEmpty().pickHero(settings)
            val style = UsageStyle.from(settings)

            WidgetFrame(settings) {
                when {
                    snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                    provider == null -> WidgetMessage(
                        title = "No providers yet",
                        detail = snapshot.error ?: "Publish a snapshot from a CodexBar device.",
                    )

                    else -> {
                        val inner = widgetInnerHeight()
                        val plan = WidgetLayout.plan(
                            innerHeight = inner,
                            barHeight = style.barHeight.value,
                            marker = style.showMarker,
                            siblingCount = siblingsOf(provider).size,
                            tileWindows = emptyList(),
                        )
                        if (!inner.isNaN() && inner >= HERO_MIN_HEIGHT) {
                            HeroCard(
                                provider = provider,
                                style = style,
                                plan = plan,
                                modifier = GlanceModifier.defaultWeight(),
                                fill = true,
                            )
                        } else {
                            UsageLine(provider, style, showBar = inner.isNaN() || inner >= LINE_WITH_BAR)
                        }
                    }
                }
            }
        }
    }

    private companion object {
        /** Below this the hero card's number and bar would not both fit. */
        const val HERO_MIN_HEIGHT = 120f

        /** The line (36dp) plus the bar and its gap. */
        const val LINE_WITH_BAR = 54f
    }
}

/** The one-line layout: badge, name over window, percentage, then the bar. */
@Composable
private fun UsageLine(provider: WidgetProvider, style: UsageStyle, showBar: Boolean) {
    val palette = LocalWidgetPalette.current
    val surface = LocalWidgetSurface.current
    val row = provider.tightest
    val tone = heroTone(provider)
    val verdict = WidgetVerdict.forProvider(provider)
    val verdictTone = toneForVerdict(verdict.tone)
    val warn = verdictTone == WidgetTone.Error || verdictTone == WidgetTone.Tertiary
    val window = row?.let {
        if (style.showReset) WidgetVerdict.headlineMeta(it) else WidgetVerdict.windowLabel(it)
    }
    Column(
        modifier = GlanceModifier.fillMaxSize(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Row(
            modifier = GlanceModifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (style.showBadge) {
                ProviderBadge(provider.id, size = 32.dp)
                Spacer(modifier = GlanceModifier.width(10.dp))
            }
            Column(modifier = GlanceModifier.defaultWeight()) {
                Text(
                    text = providerLabel(provider.id),
                    style = WidgetType.colored(WidgetType.titleSmall, surface.onShell),
                    maxLines = 1,
                )
                Text(
                    // The window always names what the bar measures; a warning
                    // leads so it is the part that survives truncation.
                    text = when {
                        window == null -> verdict.text
                        warn -> "${verdict.text} · $window"
                        else -> window
                    },
                    style = WidgetType.colored(
                        WidgetType.bodySmall,
                        if (warn) palette.accent(verdictTone) else surface.onShellVariant,
                    ),
                    maxLines = 1,
                )
            }
            Spacer(modifier = GlanceModifier.width(8.dp))
            Text(
                text = Fmt.percent(row?.percentLeft),
                style = WidgetType.colored(WidgetType.titleLarge, palette.accent(tone)),
                maxLines = 1,
            )
        }
        if (showBar) {
            Spacer(modifier = GlanceModifier.height(8.dp))
            ExpressiveBar(
                percentLeft = row?.percentLeft,
                markerFraction = if (style.showMarker && row != null) {
                    WidgetPace.timeLeftFraction(row.resetsAt, row.windowMinutes)
                } else {
                    null
                },
                width = widgetInnerWidth(),
                height = style.barHeight,
                active = palette.accent(tone),
                track = palette.track,
                marker = surface.onShell,
            )
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
