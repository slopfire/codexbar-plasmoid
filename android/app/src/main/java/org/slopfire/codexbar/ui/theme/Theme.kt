package org.slopfire.codexbar.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MotionScheme
import androidx.compose.material3.Shapes
import androidx.compose.material3.Typography
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp

/**
 * Fallback palette used when the device has no Material You dynamic colors
 * (Android 11 and older). Seeded on the CodexBar teal so the app still reads as
 * itself without wallpaper colors.
 */
private val LightColors = androidx.compose.material3.lightColorScheme(
    primary = Color(0xFF1F6B72),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFA8EDF4),
    onPrimaryContainer = Color(0xFF00201F),
    secondary = Color(0xFF4A6363),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFCCE8E7),
    onSecondaryContainer = Color(0xFF051F1F),
    tertiary = Color(0xFF9B5B32),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFDBC7),
    onTertiaryContainer = Color(0xFF331200),
    error = Color(0xFFBA1A1A),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFFDAD6),
    onErrorContainer = Color(0xFF410002),
    background = Color(0xFFF5FBFA),
    onBackground = Color(0xFF171D1D),
    surface = Color(0xFFF5FBFA),
    onSurface = Color(0xFF171D1D),
    surfaceVariant = Color(0xFFDAE4E4),
    onSurfaceVariant = Color(0xFF3F4949),
    outline = Color(0xFF6F7979),
    outlineVariant = Color(0xFFBEC8C8),
)

private val DarkColors = androidx.compose.material3.darkColorScheme(
    primary = Color(0xFF8CD1D8),
    onPrimary = Color(0xFF003739),
    primaryContainer = Color(0xFF004F53),
    onPrimaryContainer = Color(0xFFA8EDF4),
    secondary = Color(0xFFB1CCCB),
    onSecondary = Color(0xFF1B3534),
    secondaryContainer = Color(0xFF324B4B),
    onSecondaryContainer = Color(0xFFCCE8E7),
    tertiary = Color(0xFFFFB689),
    onTertiary = Color(0xFF552100),
    tertiaryContainer = Color(0xFF773300),
    onTertiaryContainer = Color(0xFFFFDBC7),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
    errorContainer = Color(0xFF93000A),
    onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF0E1514),
    onBackground = Color(0xFFDDE4E3),
    surface = Color(0xFF0E1514),
    onSurface = Color(0xFFDDE4E3),
    surfaceVariant = Color(0xFF3F4949),
    onSurfaceVariant = Color(0xFFBEC8C8),
    outline = Color(0xFF899393),
    outlineVariant = Color(0xFF3F4949),
)

/**
 * Material 3's published 10-step corner scale, copied from
 * `md.sys.shape.corner.*` (references/tokens.md): 4 / 8 / 12 / 16 / 28dp, with
 * 20 / 32 / 48 available as the expressive "increased" and "extra extra large"
 * steps.
 *
 * The five roles below are exactly the published values — do not round or
 * retune them. Cards are *not* one of the big steps: `FilledCard`,
 * `ElevatedCard` and `OutlinedCard` all use `CornerMedium` (12dp), because a
 * large radius clips the content of an information-dense container.
 */
private val CodexBarShapes = Shapes(
    extraSmall = RoundedCornerShape(4.dp),
    small = RoundedCornerShape(8.dp),
    medium = RoundedCornerShape(12.dp),
    large = RoundedCornerShape(16.dp),
    extraLarge = RoundedCornerShape(28.dp),
)

/**
 * The scheme for the current configuration. Shared by the app and by the home
 * screen widgets, so a widget and the app are always the same Material You
 * palette.
 */
fun codexBarColorScheme(
    context: android.content.Context,
    dark: Boolean,
    dynamicColor: Boolean = true,
): ColorScheme = when {
    dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
        if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)

    dark -> DarkColors
    else -> LightColors
}

/**
 * Material You + Material 3 Expressive theme.
 *
 * Dynamic wallpaper colors are used on Android 12+, the expressive motion scheme
 * everywhere, and the published expressive shape scale above.
 */
@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun CodexBarTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val colorScheme = codexBarColorScheme(context, dark = darkTheme, dynamicColor = dynamicColor)

    MaterialExpressiveTheme(
        colorScheme = colorScheme,
        motionScheme = MotionScheme.expressive(),
        shapes = CodexBarShapes,
        typography = Typography(),
        content = content,
    )
}
