package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.unit.dp
import androidx.glance.GlanceId
import androidx.glance.GlanceModifier
import androidx.glance.GlanceTheme
import androidx.glance.LocalSize
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetReceiver
import androidx.glance.appwidget.SizeMode
import androidx.glance.appwidget.provideContent
import androidx.glance.currentState
import androidx.glance.layout.Column
import androidx.glance.layout.Spacer
import androidx.glance.layout.fillMaxWidth
import androidx.glance.layout.height
import org.slopfire.codexbar.data.widget.WidgetRefresher
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.configured

/**
 * The providers widget: one provider expanded, the rest collapsed to a line.
 *
 * The list of equally-tall rows was honest but unreadable — every provider got the
 * same slice of attention, so nothing led. This version picks the first provider
 * after the user's sort (tightest remaining by default), gives it the whole
 * anatomy — verdict, the one big number, the reset line, a continuous bar with the
 * clock notch — and shows the provider's other windows underneath as the evidence
 * behind that number. Up to two further providers follow as one-line summaries.
 *
 * The layout is height-tiered rather than paginated: a 4×4 widget can afford the
 * window list, a half-height one only the headline and its summaries, and a
 * squashed one just the headline. Every tier keeps the same [Column] and the same
 * components, so nothing is a special case.
 */
class ProvidersWidget : GlanceAppWidget() {

    /**
     * `SizeMode.Exact` renders one UI per size the host may display, which is what
     * makes `LocalSize` the *real* size. The continuous bar measures its fill from
     * that width, so a `SizeMode.Single` widget would be laid out against the
     * provider info's minimum size instead.
     */
    override val sizeMode: SizeMode = SizeMode.Exact

    override suspend fun provideGlance(context: Context, id: GlanceId) {
        val snapshot = WidgetRefresher.readFresh(context)

        provideContent {
            val settings = WidgetSettings.from(currentState())
            val palette = WidgetTheme.palette(context, settings.colorMode)
            val all = snapshot?.providers.orEmpty().configured(settings)
            // A pinned row count wins; otherwise the height tier decides how much
            // of the list is worth showing.
            val visible = if (settings.maxRows > WidgetSettings.AUTO_ROWS) {
                all.take(settings.maxRows)
            } else {
                all
            }
            val headline = visible.firstOrNull()
            val height = LocalSize.current.height
            // The tiers are measured against the *inner* height: the host reports
            // the widget's outer box and [WidgetShell] keeps 12dp on every side.
            // A preview or a host that ignores SizeMode.Exact reports no height;
            // fall back to the smallest tier rather than guessing a layout.
            val inner = if (height.value.isNaN()) 0.dp else (height - SHELL_CHROME).coerceAtLeast(0.dp)
            // 118dp is the headline with both its supporting lines; below that the
            // meta and the scale would be clipped, so they are dropped together.
            val showSupport = inner >= MIN_SUPPORT
            val showWindowList = inner >= MIN_LIST
            // Summaries are what fills a taller slot: one from ~140dp, two with
            // the window list (4×4), and one more for every extra row the host
            // gives us. `maxRows` still caps the provider list itself.
            val collapsedMax = when {
                inner >= 285.dp -> 4
                inner >= 255.dp -> 3
                inner >= MIN_LIST -> 2
                inner >= MIN_SUMMARY -> 1
                else -> 0
            }
            val collapsed = visible.drop(1).take(collapsedMax)

            CompositionLocalProvider(LocalWidgetPalette provides palette) {
                GlanceTheme(colors = palette.colors) {
                    WidgetShell(
                        background = settings.background,
                        corners = settings.corners,
                    ) {
                        when {
                            snapshot == null -> WidgetMessage("CodexBar", "Loading merged usage…")

                            headline == null -> WidgetMessage(
                                title = if (settings.providers.isEmpty()) "No providers yet" else "No matching providers",
                                detail = snapshot.error
                                    ?: "Publish a snapshot from a CodexBar device.",
                            )

                            else -> Column(modifier = GlanceModifier.fillMaxWidth()) {
                                ProviderHeadline(
                                    provider = headline,
                                    barHeight = BAR_HEIGHT,
                                    paceStyle = settings.paceStyle,
                                    showBadge = settings.showBadges,
                                    // Below this tier the meta and the scale labels
                                    // would be clipped. The window rows only carry
                                    // label/bar/percent, so nothing else is lost here
                                    // besides the reset countdown.
                                    showMeta = settings.showCaptions && showSupport,
                                    showScale = showSupport,
                                )
                                // The headline's own window is already stated above
                                // (17% monthly); repeating it as a row would spend a
                                // line on the same fact and overflow a 4x4 slot.
                                // Sorted first, then drop(1): `tightest` is that same
                                // first element, so this removes exactly one row even
                                // if two windows share an id.
                                val siblings = headline.rows
                                    .sortedBy { it.percentLeft }
                                    .drop(1)
                                    .take(2)
                                if (showWindowList && siblings.isNotEmpty()) {
                                    GappedDivider()
                                    siblings.forEachIndexed { index, row ->
                                            if (index > 0) {
                                                Spacer(modifier = GlanceModifier.height(ROW_GAP))
                                            }
                                            WindowRow(
                                                row = row,
                                                providerId = headline.id,
                                                barHeight = BAR_HEIGHT,
                                                paceStyle = settings.paceStyle,
                                            )
                                        }
                                }
                                if (collapsed.isNotEmpty()) {
                                    GappedDivider()
                                    collapsed.forEachIndexed { index, provider ->
                                        if (index > 0) {
                                            Spacer(modifier = GlanceModifier.height(ROW_GAP))
                                        }
                                        CollapsedProviderRow(
                                            provider = provider,
                                            showBadge = settings.showBadges,
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

    private companion object {
        /** Space between two rows of the same list (never between sections). */
        val ROW_GAP = 4.dp

        /**
         * The redesigned bar is always 4dp: the continuous fill and the 2dp notch
         * need a hairline to read as one shape. The `barHeight` setting still
         * drives the compact widget's segmented bar.
         */
        val BAR_HEIGHT = 4.dp

        /** [WidgetShell]'s padding, top and bottom. */
        val SHELL_CHROME = 24.dp

        /** Headline + meta + scale labels: ~118dp, with a couple of dp of slack. */
        val MIN_SUPPORT = 120.dp

        /** Headline + one collapsed provider: ~148dp. */
        val MIN_SUMMARY = 140.dp

        /**
         * Headline + up to two window rows + two collapsed providers: ~222dp,
         * which is what a 4×4 slot leaves after the shell's padding.
         */
        val MIN_LIST = 220.dp
    }
}

/**
 * A section divider with the spacing both sides of it, so the two lists that use
 * it cannot drift apart.
 */
@Composable
private fun GappedDivider() {
    Spacer(modifier = GlanceModifier.height(6.dp))
    SectionDivider()
    Spacer(modifier = GlanceModifier.height(4.dp))
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
