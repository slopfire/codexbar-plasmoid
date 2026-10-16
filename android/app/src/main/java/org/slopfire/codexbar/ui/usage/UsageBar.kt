package org.slopfire.codexbar.ui.usage

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.RoundRect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import kotlin.math.min
import org.slopfire.codexbar.data.model.UsageRow
import org.slopfire.codexbar.ui.common.Fmt
import org.slopfire.codexbar.ui.common.Health
import org.slopfire.codexbar.ui.theme.ExpressiveType

/**
 * One rate-limit window as a rounded remaining-budget bar with a pace tick.
 *
 * The bar draws the remaining fraction (not the used fraction) so the eye reads
 * "how much is left" directly, matching the Plasma widget. The filled part stops
 * a hair short of the track's end when the window is not full, which keeps a
 * sliver of track visible and gives the bar its segmented, expressive look.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun UsageLimitBar(
    row: UsageRow,
    modifier: Modifier = Modifier,
    compact: Boolean = false,
) {
    val fillColor = Health.remainingColor(row.percentLeft)
    val paceColor = Health.paceColor(row.pace?.willLastToReset)
    val trackColor = MaterialTheme.colorScheme.surfaceVariant
    val captionColor = MaterialTheme.colorScheme.onSurfaceVariant
    val expectedUsed = row.pace?.expectedUsedPercent

    val target = (row.percentLeft / 100.0).coerceIn(0.0, 1.0).toFloat()
    val fraction by animateFloatAsState(
        targetValue = target,
        animationSpec = MaterialTheme.motionScheme.slowSpatialSpec<Float>(),
        label = "usage-fill",
    )

    val barHeight = if (compact) 8.dp else 12.dp

    Column(modifier = modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier.fillMaxWidth(),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = row.title,
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = Fmt.percent(row.percentLeft),
                style = ExpressiveType.titleMedium,
                color = fillColor,
            )
        }

        Spacer(Modifier.height(if (compact) 4.dp else 6.dp))

        Canvas(
            modifier = Modifier
                .fillMaxWidth()
                .height(barHeight),
        ) {
            val radius = size.height / 2f
            val corner = CornerRadius(radius, radius)

            drawRoundRect(color = trackColor, size = size, cornerRadius = corner)

            val gap = 4.dp.toPx()
            val maxFill = if (fraction >= 1f) size.width else (size.width - gap).coerceAtLeast(0f)
            val fillWidth = (size.width * fraction).coerceIn(0f, maxFill)
            if (fillWidth > 0f) {
                val fillRadius = min(radius, fillWidth / 2f)
                drawRoundRect(
                    color = fillColor,
                    size = Size(fillWidth, size.height),
                    cornerRadius = CornerRadius(fillRadius, fillRadius),
                )
            }

            if (expectedUsed != null) {
                val ratio = ((100.0 - expectedUsed) / 100.0).toFloat().coerceIn(0f, 1f)
                val tickX = size.width * ratio
                val half = 1.5.dp.toPx()
                val clip = Path().apply {
                    addRoundRect(RoundRect(Rect(0f, 0f, size.width, size.height), corner))
                }
                clipPath(clip) {
                    drawRect(
                        color = paceColor,
                        topLeft = Offset((tickX - half).coerceAtLeast(0f), 0f),
                        size = Size(half * 2f, size.height),
                    )
                }
            }
        }

        val captions = buildList<Pair<String, Color?>> {
            Fmt.resetCountdown(row.resetsAt)?.let { add(it to null) }
            row.windowMinutes?.let { minutes ->
                Fmt.duration(minutes * 60)?.let { add("$it window" to null) }
            }
            Fmt.paceText(row.pace)?.let { add(it to paceColor) }
        }
        if (captions.isNotEmpty()) {
            Spacer(Modifier.height(if (compact) 2.dp else 4.dp))
            Text(
                text = buildAnnotatedString {
                    captions.forEachIndexed { index, (label, color) ->
                        if (index > 0) append(" · ")
                        if (color != null) {
                            withStyle(SpanStyle(color = color)) { append(label) }
                        } else {
                            append(label)
                        }
                    }
                },
                style = MaterialTheme.typography.bodySmall,
                color = captionColor,
            )
        }
    }
}
