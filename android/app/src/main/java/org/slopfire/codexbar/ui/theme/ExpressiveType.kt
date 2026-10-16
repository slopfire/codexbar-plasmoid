package org.slopfire.codexbar.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp

/**
 * The M3 Expressive **emphasized** type set.
 *
 * Material Compose does not expose the emphasized scale as a library feature in
 * this version, so these are built from the published token values
 * (`md.sys.typescale.emphasized.*` in `references/tokens.md`). Emphasized styles
 * share the baseline size and line height and change **only weight and
 * tracking**, and components do not use them by default — they mark the
 * exception, not the rule.
 *
 * Use them for selection, actions, headlines and the one number that matters on
 * a screen. Do not use them as a whole type scale.
 */
object ExpressiveType {

    /** Display/headline/title-large/body go Regular → Medium; tracking normalizes to 0. */
    private const val BRAND_TRACKING = 0f

    val titleLarge = TextStyle(
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = BRAND_TRACKING.sp,
        fontWeight = FontWeight.Medium,
    )

    val titleMedium = TextStyle(
        fontSize = 16.sp,
        lineHeight = 24.sp,
        letterSpacing = 0.15.sp,
        fontWeight = FontWeight.Bold,
    )

    val titleSmall = TextStyle(
        fontSize = 14.sp,
        lineHeight = 20.sp,
        letterSpacing = 0.1.sp,
        fontWeight = FontWeight.Bold,
    )

    val bodyMedium = TextStyle(
        fontSize = 14.sp,
        lineHeight = 20.sp,
        letterSpacing = 0.25.sp,
        fontWeight = FontWeight.Medium,
    )

    /** Label large/medium/small go Medium → Bold. */
    val labelLarge = TextStyle(
        fontSize = 14.sp,
        lineHeight = 20.sp,
        letterSpacing = 0.1.sp,
        fontWeight = FontWeight.Bold,
    )

    val labelMedium = TextStyle(
        fontSize = 12.sp,
        lineHeight = 16.sp,
        letterSpacing = 0.5.sp,
        fontWeight = FontWeight.Bold,
    )
}

/**
 * Emphasized style for the headline number on a surface — the metric a glance
 * should land on first. Falls back to the baseline role when a caller wants to
 * opt out.
 */
@Composable
@ReadOnlyComposable
fun emphasizedMetricStyle(): TextStyle = ExpressiveType.titleLarge

/** Baseline typography, kept for reference in one place. */
@Composable
@ReadOnlyComposable
fun baselineTypography(): Typography = MaterialTheme.typography
