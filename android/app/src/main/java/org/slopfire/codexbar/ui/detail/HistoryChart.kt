package org.slopfire.codexbar.ui.detail

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.slopfire.codexbar.data.model.DailyUsagePoint
import org.slopfire.codexbar.ui.common.Fmt

/** Height of the plot area, before the metric caption below it. */
private val ChartHeight = 160.dp

/** Trailing window kept on screen; the server already trims to ~31 days. */
private const val MaxDays = 31

/** Draw a date label every Nth bar so the axis stays readable. */
private const val LabelEvery = 5

/** Alpha applied to every bar except the selected one. */
private const val UnselectedAlpha = 0.45f

/**
 * A self-contained interactive bar chart for the trailing daily history.
 *
 * Bars use `costUSD` when any day reported a cost and fall back to `totalTokens`
 * otherwise, so a spend-free history still renders something meaningful. Tapping
 * a bar selects it (tapping it again clears the selection) and reports the day
 * through [onSelect] for the detail card. This composable owns no navigation and
 * reads only from [points].
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun HistoryChart(
    points: List<DailyUsagePoint>,
    modifier: Modifier = Modifier,
    accent: Color,
    onSelect: (DailyUsagePoint?) -> Unit = {},
) {
    val series = remember(points) { sanitize(points) }
    if (series.isEmpty()) {
        Box(modifier = modifier.fillMaxWidth().height(ChartHeight))
        return
    }

    val useCost = series.any { it.costUSD > 0.0 }
    val values = series.map { if (useCost) it.costUSD else it.totalTokens.toDouble() }
    val maxValue = values.maxOrNull() ?: 0.0
    val seriesKey = remember(series) {
        series.joinToString(separator = "|") { "${it.dayKey}:${it.costUSD}:${it.totalTokens}" }
    }

    var selectedIndex by remember { mutableStateOf<Int?>(null) }
    var revealed by remember { mutableStateOf(false) }
    val growth by animateFloatAsState(
        targetValue = if (revealed) 1f else 0f,
        animationSpec = MaterialTheme.motionScheme.slowSpatialSpec<Float>(),
        label = "historyGrowth",
    )

    LaunchedEffect(seriesKey) {
        selectedIndex = null
        onSelect(null)
        revealed = false
        withFrameNanos { }
        revealed = true
    }

    // Read in composition so a selection change recomposes the canvas; the draw
    // lambda then sees the new value without relying on draw-phase observation.
    val selected = selectedIndex
    val textMeasurer = rememberTextMeasurer()
    val labelStyle = TextStyle(
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        fontSize = 10.sp,
    )
    val baselineColor = MaterialTheme.colorScheme.outlineVariant

    Column(modifier = modifier.fillMaxWidth()) {
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(ChartHeight)
                .pointerInput(series) {
                    detectTapGestures { offset ->
                        if (series.isEmpty()) return@detectTapGestures
                        val slot = size.width.toFloat() / series.size
                        if (slot <= 0f) return@detectTapGestures
                        val tapped = (offset.x / slot).toInt().coerceIn(0, series.lastIndex)
                        val next = if (selectedIndex == tapped) null else tapped
                        selectedIndex = next
                        onSelect(next?.let { series[it] })
                    }
                },
        ) {
            Canvas(modifier = Modifier.fillMaxSize()) {
                val labelBand = 16.dp.toPx()
                val topPadding = 6.dp.toPx()
                val baseline = (size.height - labelBand).coerceAtLeast(0f)
                val plotHeight = (baseline - topPadding).coerceAtLeast(0f)
                val slot = size.width / series.size
                if (slot <= 0f) return@Canvas
                val barWidth = slot * 0.62f

                drawLine(
                    color = baselineColor,
                    start = Offset(0f, baseline),
                    end = Offset(size.width, baseline),
                    strokeWidth = 1.dp.toPx(),
                )

                for (index in series.indices) {
                    val value = values.getOrElse(index) { 0.0 }
                    if (value <= 0.0 || maxValue <= 0.0) continue
                    val fraction = (value / maxValue).toFloat().coerceIn(0f, 1f)
                    val barHeight = plotHeight * fraction * growth
                    if (barHeight <= 0f) continue
                    val left = index * slot + (slot - barWidth) / 2f
                    val right = left + barWidth
                    val top = baseline - barHeight
                    val radius = minOf(barWidth / 2f, barHeight)
                    val path = Path().apply {
                        moveTo(left, baseline)
                        lineTo(left, top + radius)
                        quadraticTo(left, top, left + radius, top)
                        lineTo(right - radius, top)
                        quadraticTo(right, top, right, top + radius)
                        lineTo(right, baseline)
                        close()
                    }
                    val alpha = if (index == selected) 1f else UnselectedAlpha
                    drawPath(path = path, color = accent.copy(alpha = alpha))
                }

                for (index in series.indices) {
                    // Label every Nth day plus the newest one, and skip a
                    // boundary label that would sit right next to the newest
                    // one — two adjacent dates read as one smudge.
                    val onBoundary = index % LabelEvery == 0 && index < series.lastIndex - 1
                    if (!onBoundary && index != series.lastIndex) continue
                    val label = Fmt.dayKey(series[index].dayKey)
                    if (label.isBlank()) continue
                    val layout = textMeasurer.measure(text = label, style = labelStyle)
                    val centerX = index * slot + slot / 2f
                    val maxX = (size.width - layout.size.width).coerceAtLeast(0f)
                    val x = (centerX - layout.size.width / 2f).coerceIn(0f, maxX)
                    drawText(layout, topLeft = Offset(x, baseline + 4.dp.toPx()))
                }
            }
        }

        Text(
            text = if (useCost) "Bars show daily spend" else "Bars show daily tokens",
            modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** Keeps only parseable day keys, oldest first, trimmed to the trailing window. */
private fun sanitize(points: List<DailyUsagePoint>): List<DailyUsagePoint> =
    points
        .filter { Fmt.parseMillis(it.dayKey) != null }
        .sortedBy { it.dayKey }
        .takeLast(MaxDays)
