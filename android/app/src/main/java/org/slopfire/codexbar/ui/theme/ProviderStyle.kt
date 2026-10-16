package org.slopfire.codexbar.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.lerp
import kotlin.math.absoluteValue

/**
 * Provider accent colors and display names.
 *
 * The color table mirrors the Plasma widget so the same provider looks the same
 * on the desktop and on the phone. Unknown providers fall back to a stable hue
 * derived from the id, which keeps new providers distinguishable without a code
 * change.
 */
private val providerColors = mapOf(
    "codex" to Color(0xFF4B929B),
    "claude" to Color(0xFFB57861),
    "commandcode" to Color(0xFFA1579C),
    "cursor" to Color(0xFF3C9487),
    "gemini" to Color(0xFF8972B5),
    "copilot" to Color(0xFF8C68B7),
    "openai" to Color(0xFF398979),
    "minimax" to Color(0xFFBD684E),
    "grok" to Color(0xFF3D8D76),
    "groq" to Color(0xFFB86C55),
    "openrouter" to Color(0xFF4D88AD),
    "devin" to Color(0xFF6769AD),
    "jetbrains" to Color(0xFF6B8CAE),
    "opencode" to Color(0xFF5A8F6B),
    "opencodego" to Color(0xFF4F9C8C),
    "clinepass" to Color(0xFF7A8F3C),
    "deepseek" to Color(0xFF4E6BC4),
    "kilo" to Color(0xFF8E7CC3),
    "amp" to Color(0xFFB4574F),
    "warp" to Color(0xFF5E7CB8),
    "zai" to Color(0xFF6E6E9C),
    "mistral" to Color(0xFFC4713F),
    "perplexity" to Color(0xFF3E8F96),
)

private val providerNames = mapOf(
    "codex" to "Codex",
    "claude" to "Claude",
    "commandcode" to "Command Code",
    "clinepass" to "ClinePass",
    "cursor" to "Cursor",
    "gemini" to "Gemini",
    "copilot" to "Copilot",
    "openai" to "OpenAI",
    "openrouter" to "OpenRouter",
    "opencodego" to "OpenCode Go",
    "opencode" to "OpenCode",
    "llmproxy" to "LLM Proxy",
    "t3chat" to "T3 Chat",
    "vertexai" to "Vertex AI",
    "jetbrains" to "JetBrains",
    "elevenlabs" to "ElevenLabs",
    "deepgram" to "Deepgram",
    "stepfun" to "StepFun",
    "synthetic" to "Synthetic",
    "augment" to "Augment",
    "antigravity" to "Antigravity",
    "windsurf" to "Windsurf",
    "codebuff" to "Codebuff",
    "kilo" to "Kilo",
    "kimi" to "Kimi",
    "kiro" to "Kiro",
    "manus" to "Manus",
    "mimo" to "MiMo",
    "minimax" to "MiniMax",
    "mistral" to "Mistral",
    "ollama" to "Ollama",
    "perplexity" to "Perplexity",
    "venice" to "Venice",
    "zai" to "Z.ai",
    "deepseek" to "DeepSeek",
    "grok" to "Grok",
    "groq" to "Groq",
    "devin" to "Devin",
    "warp" to "Warp",
    "factory" to "Factory",
    "crof" to "Crof",
    "doubao" to "Doubao",
    "alibaba" to "Alibaba",
    "bedrock" to "Bedrock",
    "abacus" to "Abacus",
    "amp" to "Amp",
    "demo" to "Demo",
)

/** Accent for a provider id; falls back to a stable generated hue. */
fun providerColor(provider: String): Color {
    val id = provider.lowercase()
    providerColors[id]?.let { return it }
    val hue = (id.hashCode().absoluteValue % 360).toFloat()
    return Color.hsv(hue, 0.42f, 0.68f)
}

/** The accent tuned for the current surface, so it stays readable in both themes. */
@Composable
@ReadOnlyComposable
fun providerAccent(provider: String): Color {
    val base = providerColor(provider)
    val scheme = MaterialTheme.colorScheme
    return if (scheme.surface.luminance() < 0.5f) {
        lerp(base, Color.White, 0.22f)
    } else {
        lerp(base, Color.Black, 0.12f)
    }
}

/**
 * How much of a provider's own color a tinted card container carries.
 *
 * Enough that a long list reads as a set of colored panels instead of one gray
 * slab, little enough that the card's text keeps its contrast in both themes.
 */
const val ProviderCardTint: Float = 0.12f

/**
 * The container color for a provider's cards: the theme's card surface with a
 * little of the provider's accent stirred in.
 *
 * The tint is a lerp **from** the resolved surface role rather than a fixed
 * provider color, so it harmonizes with Material You wallpapers and stays
 * legible in both themes. The strong accent is still used for identity cues —
 * avatars, the selected filter chip, chart bars — where contrast is guaranteed
 * by the pairing, not by the surface.
 */
@Composable
@ReadOnlyComposable
fun providerContainerColor(
    provider: String,
    fraction: Float = ProviderCardTint,
): Color {
    val base = MaterialTheme.colorScheme.surfaceContainer
    return lerp(base, providerAccent(provider), fraction)
}

private fun Color.luminance(): Float = 0.2126f * red + 0.7152f * green + 0.0722f * blue

/**
 * Day/night accent pair for surfaces that have no Compose theme to read — the
 * home screen widgets render through Glance, which resolves colors from a
 * day/night [androidx.glance.unit.ColorProvider] instead of a ColorScheme.
 */
fun providerAccentPair(provider: String): Pair<Color, Color> {
    val base = providerColor(provider)
    return lerp(base, Color.Black, 0.12f) to lerp(base, Color.White, 0.22f)
}

/** Two-letter monogram used where a provider icon is too detailed to read. */
fun providerMonogram(provider: String): String {
    val words = providerLabel(provider).split(' ', '-').filter { it.isNotBlank() }
    return when {
        words.isEmpty() -> "?"
        words.size == 1 -> words.first().take(1).uppercase()
        else -> words.take(2).map { it.take(1).uppercase() }.joinToString("")
    }
}

/** Title-cased provider name, with the well-known multi-word ids spelled out. */
fun providerLabel(provider: String): String {
    val id = provider.lowercase()
    providerNames[id]?.let { return it }
    if (id.isBlank()) return "Unknown"
    return id
        .split('-', '_')
        .filter { it.isNotBlank() }
        .joinToString(" ") { part -> part.replaceFirstChar { it.uppercaseChar() } }
}

/**
 * The bundled drawable for a provider icon, or 0 when this build has no icon
 * for that id.
 *
 * The icons are white silhouettes rasterized from the plasmoid's own
 * `ProviderIcon-*.svg` set (see `scripts/`), so both the app and the home screen
 * widgets can tint them with the provider's accent color. Resolution is cached:
 * `getIdentifier` is a resource-table lookup and widgets render often.
 */
private val iconResourceCache = java.util.concurrent.ConcurrentHashMap<String, Int>()

fun providerIconRes(context: android.content.Context, provider: String): Int {
    val key = provider.lowercase().filter { it.isLetterOrDigit() }
    if (key.isEmpty()) return 0
    return iconResourceCache.getOrPut(key) {
        context.resources.getIdentifier("provider_$key", "drawable", context.packageName)
    }
}
