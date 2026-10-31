package org.slopfire.codexbar.widget

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.widget.FrameLayout
import android.widget.RemoteViews
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredSize
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.datastore.preferences.core.mutablePreferencesOf
import androidx.glance.ExperimentalGlanceApi
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.compose
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetBarHeight
import org.slopfire.codexbar.data.widget.WidgetColorMode
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.data.widget.WidgetKind
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.WidgetSnapshotStore
import org.slopfire.codexbar.data.widget.WidgetSort
import org.slopfire.codexbar.ui.theme.CodexBarTheme
import org.slopfire.codexbar.ui.theme.providerLabel

/**
 * Per-widget configuration, launched by the launcher when a widget is added and
 * again on a reconfiguration (long-press the widget → Edit).
 *
 * Settings are stored in that widget instance's Glance preferences, so two
 * instances of the same widget can be configured differently.
 */
class WidgetConfigActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val appWidgetId = intent?.extras?.getInt(
            AppWidgetManager.EXTRA_APPWIDGET_ID,
            AppWidgetManager.INVALID_APPWIDGET_ID,
        ) ?: AppWidgetManager.INVALID_APPWIDGET_ID

        // Cancelling is the documented default until the user saves.
        setResult(
            Activity.RESULT_CANCELED,
            Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId),
        )
        if (appWidgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
            finish()
            return
        }

        val manager = AppWidgetManager.getInstance(this)
        val kind = kindOf(manager.getAppWidgetInfo(appWidgetId)?.provider)
        val available = WidgetSnapshotStore(this).read()
            ?.providers
            ?.map { it.id }
            ?.sorted()
            .orEmpty()
        val initial = runBlocking {
            WidgetSettings.from(WidgetSettingsStore.read(this@WidgetConfigActivity, appWidgetId))
        }

        val previewSize = previewSizeFor(this, appWidgetId, kind)

        setContent {
            CodexBarTheme {
                var settings by remember { mutableStateOf(initial) }
                WidgetConfigScreen(
                    kind = kind,
                    previewSize = previewSize,
                    availableProviders = available,
                    settings = settings,
                    onChange = { settings = it },
                    onCancel = { finish() },
                    onSave = { save(appWidgetId, kind, it) },
                )
            }
        }
    }

    private fun save(appWidgetId: Int, kind: WidgetKind, settings: WidgetSettings) {
        lifecycleScope.launch {
            WidgetSettingsStore.write(this@WidgetConfigActivity, appWidgetId) { preferences ->
                WidgetSettings.applyTo(preferences, settings)
            }
            WidgetSettingsStore.refresh(this@WidgetConfigActivity, widgetFor(kind), appWidgetId)
            setResult(
                Activity.RESULT_OK,
                Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId),
            )
            finish()
        }
    }

    private fun kindOf(provider: ComponentName?): WidgetKind = when (provider?.className) {
        CompactWidgetReceiver::class.java.name -> WidgetKind.Compact
        SpendWidgetReceiver::class.java.name -> WidgetKind.Spend
        else -> WidgetKind.Providers
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WidgetConfigScreen(
    kind: WidgetKind,
    previewSize: DpSize,
    availableProviders: List<String>,
    settings: WidgetSettings,
    onChange: (WidgetSettings) -> Unit,
    onCancel: () -> Unit,
    onSave: (WidgetSettings) -> Unit,
) {
    val usage = kind != WidgetKind.Spend
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Customize widget")
                        Text(
                            text = kindTitle(kind),
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onCancel) {
                        Icon(Icons.Default.Close, contentDescription = "Cancel")
                    }
                },
            )
        },
        bottomBar = {
            Surface(color = MaterialTheme.colorScheme.surfaceContainer) {
                Row(
                    modifier = Modifier
                        .fillMaxWidth()
                        .windowInsetsPadding(WindowInsets.navigationBars)
                        .padding(horizontal = 16.dp, vertical = 12.dp),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    OutlinedButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                        Text("Cancel")
                    }
                    Button(onClick = { onSave(settings) }, modifier = Modifier.weight(1f)) {
                        Text("Save")
                    }
                }
            }
        },
    ) { padding ->
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            // Pinned above the options so every change is visible as it is made.
            WidgetPreview(
                kind = kind,
                settings = settings,
                size = previewSize,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
            LazyColumn(
                modifier = Modifier.fillMaxSize(),
                contentPadding = PaddingValues(horizontal = 16.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                item {
                    ConfigGroup(title = if (kind == WidgetKind.Compact) "Provider" else "Providers") {
                        when {
                            availableProviders.isEmpty() -> SupportingText(
                                "No providers have been published yet. The widget will show " +
                                    "every provider once data arrives.",
                            )

                            kind == WidgetKind.Compact -> {
                                SupportingText("Follow whichever limit is tightest, or pin one provider.")
                                ProviderChips(
                                    options = listOf("" to "Tightest") +
                                        availableProviders.map { it to providerLabel(it) },
                                    isSelected = { it == settings.heroProvider },
                                    onClick = { onChange(settings.copy(heroProvider = it)) },
                                )
                            }

                            else -> {
                                SupportingText("Pick the providers to include.")
                                ProviderChips(
                                    options = listOf("" to "All") +
                                        availableProviders.map { it to providerLabel(it) },
                                    isSelected = { id ->
                                        if (id.isEmpty()) settings.providers.isEmpty() else id in settings.providers
                                    },
                                    onClick = { id ->
                                        val next = when {
                                            id.isEmpty() -> emptySet()
                                            id in settings.providers -> settings.providers - id
                                            else -> settings.providers + id
                                        }
                                        onChange(settings.copy(providers = next))
                                    },
                                )
                            }
                        }
                        if (kind == WidgetKind.Providers) {
                            OptionLabel("Lead with")
                            Segmented(
                                options = WidgetSort.entries,
                                selected = settings.sort,
                                label = { it.label },
                                onSelect = { onChange(settings.copy(sort = it)) },
                            )
                        }
                    }
                }

                item {
                    ConfigGroup(title = "Style") {
                        OptionLabel("Colours")
                        Segmented(
                            options = WidgetColorMode.entries,
                            selected = settings.colorMode,
                            label = { it.label },
                            onSelect = { onChange(settings.copy(colorMode = it)) },
                        )
                        OptionLabel("Background")
                        Segmented(
                            options = WidgetBackground.entries,
                            selected = settings.background,
                            label = { it.label },
                            onSelect = { onChange(settings.copy(background = it)) },
                        )
                        OptionLabel("Corners")
                        Segmented(
                            options = WidgetCorners.entries,
                            selected = settings.corners,
                            label = { it.label },
                            onSelect = { onChange(settings.copy(corners = it)) },
                        )
                        if (usage) {
                            OptionLabel("Bars")
                            Segmented(
                                options = WidgetBarHeight.entries,
                                selected = settings.barHeight,
                                label = { it.label },
                                onSelect = { onChange(settings.copy(barHeight = it)) },
                            )
                        }
                    }
                }

                item {
                    ConfigGroup(title = "Show") {
                        SwitchRow(
                            title = "Provider icons",
                            detail = null,
                            checked = settings.showBadges,
                            onChange = { onChange(settings.copy(showBadges = it)) },
                        )
                        if (usage) {
                            SwitchRow(
                                title = "Reset countdown",
                                detail = "When each limit refills",
                                checked = settings.showCaptions,
                                onChange = { onChange(settings.copy(showCaptions = it)) },
                            )
                            SwitchRow(
                                title = "Time marker",
                                detail = "A tick on each bar showing how much of the window is left. " +
                                    "Fill past the tick means the budget outlasts the clock.",
                                checked = settings.showTimeMarker,
                                onChange = { onChange(settings.copy(showTimeMarker = it)) },
                            )
                        }
                    }
                }
            }
        }
    }
}

/**
 * The real widget, rendered from the unsaved settings.
 *
 * Glance composes the same RemoteViews the launcher would get, at the size the
 * launcher reported for this instance, and the result is inflated in place. It
 * sits on a tonal backdrop so Translucent and Clear visibly differ from Solid.
 */
@OptIn(ExperimentalGlanceApi::class)
@Composable
private fun WidgetPreview(
    kind: WidgetKind,
    settings: WidgetSettings,
    size: DpSize,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    var views by remember { mutableStateOf<RemoteViews?>(null) }
    LaunchedEffect(settings) {
        val state = mutablePreferencesOf().also { WidgetSettings.applyTo(it, settings) }
        views = runCatching {
            widgetFor(kind).compose(context, size = size, state = state)
        }.getOrNull()
    }
    val backdrop = Brush.linearGradient(
        listOf(
            MaterialTheme.colorScheme.primaryContainer,
            MaterialTheme.colorScheme.tertiaryContainer,
        ),
    )
    // The widget is laid out at its real size, then scaled down so a tall
    // widget still leaves most of the screen to the options.
    val scale = minOf(1f, PREVIEW_MAX_HEIGHT / size.height)
    Box(
        modifier = modifier
            .fillMaxWidth()
            .clip(MaterialTheme.shapes.extraLarge)
            .background(backdrop)
            .padding(vertical = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            modifier = Modifier.size(size.width * scale, size.height * scale),
            contentAlignment = Alignment.Center,
        ) {
            AndroidView(
                factory = { FrameLayout(it) },
                update = { frame ->
                    frame.removeAllViews()
                    views?.let { frame.addView(it.apply(frame.context, frame)) }
                },
                modifier = Modifier
                    .requiredSize(size)
                    .graphicsLayer {
                        scaleX = scale
                        scaleY = scale
                    },
            )
        }
        // The preview is for looking at: swallow taps so it cannot open the app.
        Box(
            modifier = Modifier
                .matchParentSize()
                .clickable(
                    interactionSource = remember { MutableInteractionSource() },
                    indication = null,
                    onClick = {},
                ),
        )
    }
}

/** Tallest the pinned preview may get before it is scaled down. */
private val PREVIEW_MAX_HEIGHT = 220.dp

/** A titled group of options on one tonal surface. */
@Composable
private fun ConfigGroup(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(
            text = title,
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.padding(horizontal = 4.dp),
        )
        Surface(
            modifier = Modifier.fillMaxWidth(),
            shape = MaterialTheme.shapes.large,
            color = MaterialTheme.colorScheme.surfaceContainer,
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(10.dp),
            ) { content() }
        }
    }
}

@Composable
private fun OptionLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier.padding(top = 4.dp),
    )
}

@Composable
private fun SupportingText(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** A single-choice M3 segmented button row spanning the group. */
@Composable
private fun <T> Segmented(
    options: List<T>,
    selected: T,
    label: (T) -> String,
    onSelect: (T) -> Unit,
) {
    // Selected reads as a filled primary toggle (M3 Expressive): the baseline
    // secondary-container fill is too close to the unselected segments under
    // some wallpaper palettes.
    val colors = SegmentedButtonDefaults.colors(
        activeContainerColor = MaterialTheme.colorScheme.primary,
        activeContentColor = MaterialTheme.colorScheme.onPrimary,
        activeBorderColor = MaterialTheme.colorScheme.primary,
    )
    SingleChoiceSegmentedButtonRow(modifier = Modifier.fillMaxWidth()) {
        options.forEachIndexed { index, option ->
            SegmentedButton(
                selected = option == selected,
                onClick = { onSelect(option) },
                shape = SegmentedButtonDefaults.itemShape(index = index, count = options.size),
                colors = colors,
                // The selected fill already says which one is on; a check mark
                // would only squeeze the labels.
                icon = {},
            ) {
                Text(label(option), maxLines = 1, softWrap = false)
            }
        }
    }
}

/** Provider filter chips, wrapping onto as many lines as they need. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ProviderChips(
    options: List<Pair<String, String>>,
    isSelected: (String) -> Boolean,
    onClick: (String) -> Unit,
) {
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        verticalArrangement = Arrangement.spacedBy(0.dp),
    ) {
        options.forEach { (value, label) ->
            val selected = isSelected(value)
            FilterChip(
                selected = selected,
                onClick = { onClick(value) },
                label = { Text(label, maxLines = 1) },
                leadingIcon = if (selected) {
                    { Icon(Icons.Default.Check, contentDescription = null, modifier = Modifier.size(18.dp)) }
                } else {
                    null
                },
            )
        }
    }
}

@Composable
private fun SwitchRow(
    title: String,
    detail: String?,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clip(MaterialTheme.shapes.medium)
            .clickable { onChange(!checked) }
            .padding(vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(text = title, style = MaterialTheme.typography.bodyLarge)
            if (detail != null) {
                Text(
                    text = detail,
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        Spacer(modifier = Modifier.width(16.dp))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

private fun widgetFor(kind: WidgetKind): GlanceAppWidget = when (kind) {
    WidgetKind.Compact -> CompactWidget()
    WidgetKind.Providers -> ProvidersWidget()
    WidgetKind.Spend -> SpendWidget()
}

/**
 * The size the launcher reported for this instance (portrait: min width, max
 * height), or the widget's target size when it has not reported one yet — which
 * is the case while it is being added.
 */
private fun previewSizeFor(context: Context, appWidgetId: Int, kind: WidgetKind): DpSize {
    val options = AppWidgetManager.getInstance(context).getAppWidgetOptions(appWidgetId)
    val width = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH)
    val height = options.getInt(AppWidgetManager.OPTION_APPWIDGET_MAX_HEIGHT)
    val fallback = when (kind) {
        WidgetKind.Compact -> DpSize(300.dp, 96.dp)
        WidgetKind.Providers -> DpSize(320.dp, 200.dp)
        WidgetKind.Spend -> DpSize(300.dp, 200.dp)
    }
    val screen = context.resources.configuration.screenWidthDp.dp - 64.dp
    val size = if (width > 0 && height > 0) DpSize(width.dp, height.dp) else fallback
    return DpSize(size.width.coerceAtMost(screen), size.height.coerceIn(56.dp, 420.dp))
}

private fun kindTitle(kind: WidgetKind): String = when (kind) {
    WidgetKind.Compact -> "Usage widget"
    WidgetKind.Providers -> "Providers widget"
    WidgetKind.Spend -> "Spend widget"
}

/** Convenience for the launcher entry points that need the config component. */
internal fun configComponent(context: Context): ComponentName =
    ComponentName(context, WidgetConfigActivity::class.java)
