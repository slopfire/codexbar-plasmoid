package org.slopfire.codexbar.widget

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import org.slopfire.codexbar.data.widget.WidgetBackground
import org.slopfire.codexbar.data.widget.WidgetBarHeight
import org.slopfire.codexbar.data.widget.WidgetColorMode
import org.slopfire.codexbar.data.widget.WidgetCorners
import org.slopfire.codexbar.data.widget.WidgetKind
import org.slopfire.codexbar.data.widget.WidgetPaceStyle
import org.slopfire.codexbar.data.widget.WidgetSettings
import org.slopfire.codexbar.data.widget.WidgetSort
import org.slopfire.codexbar.data.widget.WidgetSnapshotStore
import org.slopfire.codexbar.ui.common.SectionHeader
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

        setContent {
            CodexBarTheme {
                var settings by remember { mutableStateOf(initial) }
                WidgetConfigScreen(
                    kind = kind,
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

    private fun widgetFor(kind: WidgetKind) = when (kind) {
        WidgetKind.Compact -> CompactWidget()
        WidgetKind.Providers -> ProvidersWidget()
        WidgetKind.Spend -> SpendWidget()
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
    availableProviders: List<String>,
    settings: WidgetSettings,
    onChange: (WidgetSettings) -> Unit,
    onCancel: () -> Unit,
    onSave: (WidgetSettings) -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(
                            text = "Configure widget",
                            style = MaterialTheme.typography.titleLarge,
                            fontWeight = FontWeight.SemiBold,
                        )
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
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
            )
        },
        containerColor = MaterialTheme.colorScheme.surface,
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 12.dp),
            verticalArrangement = Arrangement.spacedBy(20.dp),
        ) {
            item {
                ConfigSection(
                    title = "Providers",
                    subtitle = if (availableProviders.isEmpty()) {
                        "No providers published yet — everything will be shown once data arrives."
                    } else {
                        "Leave all unselected to follow every provider the server merges."
                    },
                ) {
                    if (availableProviders.isNotEmpty()) {
                        // One "All" chip, then the multi-select list. Rendering the
                        // provider names in the All row as well would make every one
                        // of those chips clear the filter instead of picking a
                        // provider.
                        ChipRow(
                            options = listOf("" to "All"),
                            selected = if (settings.providers.isEmpty()) "" else null,
                            onSelect = { onChange(settings.copy(providers = emptySet())) },
                        )
                        ChipRow(
                            options = availableProviders.map { it to providerLabel(it) },
                            selected = null,
                            selectedSet = settings.providers,
                            onSelect = { id ->
                                val next = settings.providers.toMutableSet()
                                if (!next.add(id)) next.remove(id)
                                onChange(settings.copy(providers = next))
                            },
                        )
                    }
                }
            }

            if (kind == WidgetKind.Compact) {
                item {
                    ConfigSection(
                        title = "Show",
                        subtitle = "Pin one provider instead of following whichever is tightest.",
                    ) {
                        ChipRow(
                            options = listOf("" to "Tightest") +
                                availableProviders.map { it to providerLabel(it) },
                            selected = settings.heroProvider,
                            onSelect = { onChange(settings.copy(heroProvider = it)) },
                        )
                    }
                }
            }

            if (kind == WidgetKind.Providers) {
                item {
                    ConfigSection(title = "Order", subtitle = null) {
                        ChipRow(
                            options = WidgetSort.entries.map { it.name to it.label },
                            selected = settings.sort.name,
                            onSelect = { name ->
                                onChange(
                                    settings.copy(
                                        sort = WidgetSort.entries.first { it.name == name },
                                    ),
                                )
                            },
                        )
                    }
                }
                item {
                    ConfigSection(
                        title = "Rows",
                        subtitle = "Auto fits the height the launcher grants.",
                    ) {
                        ChipRow(
                            options = listOf("0" to "Auto") +
                                listOf(2, 3, 4, 5).map { it.toString() to it.toString() },
                            selected = settings.maxRows.toString(),
                            onSelect = { rows ->
                                onChange(settings.copy(maxRows = rows.toIntOrNull() ?: 0))
                            },
                        )
                    }
                }
            }

            item {
                ConfigSection(
                    title = "Details",
                    subtitle = "The reset countdown and pace verdict under each bar.",
                ) {
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = "Show reset and pace",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Switch(
                            checked = settings.showCaptions,
                            onCheckedChange = { onChange(settings.copy(showCaptions = it)) },
                        )
                    }
                }
            }

            item {
                ConfigSection(
                    title = "Appearance",
                    subtitle = "Material You surface with an M3 Expressive shape scale.",
                ) {
                    ConfigLabel("Background")
                    ChipRow(
                        options = WidgetBackground.entries.map { it.name to it.label },
                        selected = settings.background.name,
                        onSelect = { name ->
                            onChange(
                                settings.copy(
                                    background = WidgetBackground.entries.first { it.name == name },
                                ),
                            )
                        },
                    )
                    ConfigLabel("Corners")
                    ChipRow(
                        options = WidgetCorners.entries.map { it.name to it.label },
                        selected = settings.corners.name,
                        onSelect = { name ->
                            onChange(
                                settings.copy(
                                    corners = WidgetCorners.entries.first { it.name == name },
                                ),
                            )
                        },
                    )
                    ConfigLabel("Bar thickness")
                    ChipRow(
                        options = WidgetBarHeight.entries.map { it.name to it.label },
                        selected = settings.barHeight.name,
                        onSelect = { name ->
                            onChange(
                                settings.copy(
                                    barHeight = WidgetBarHeight.entries.first { it.name == name },
                                ),
                            )
                        },
                    )
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = "Show header",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Switch(
                            checked = settings.showHeader,
                            onCheckedChange = { onChange(settings.copy(showHeader = it)) },
                        )
                    }
                    Row(
                        modifier = Modifier.fillMaxWidth(),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(
                            text = "Show provider badges",
                            style = MaterialTheme.typography.bodyMedium,
                            modifier = Modifier.weight(1f),
                        )
                        Switch(
                            checked = settings.showBadges,
                            onCheckedChange = { onChange(settings.copy(showBadges = it)) },
                        )
                    }
                }
            }

            if (kind != WidgetKind.Spend) {
                item {
                    ConfigSection(
                        title = "Pacing marker",
                        subtitle = "Split bar at the time marker, like the desktop widget.",
                    ) {
                        ChipRow(
                            options = WidgetPaceStyle.entries.map { it.name to it.label },
                            selected = settings.paceStyle.name,
                            onSelect = { name ->
                                onChange(
                                    settings.copy(
                                        paceStyle = WidgetPaceStyle.entries.first { it.name == name },
                                    ),
                                )
                            },
                        )
                    }
                }
            }

            item {
                ConfigSection(
                    title = "Colour",
                    subtitle = "Material You follows your wallpaper.",
                ) {
                    ChipRow(
                        options = WidgetColorMode.entries.map { it.name to it.label },
                        selected = settings.colorMode.name,
                        onSelect = { name ->
                            onChange(
                                settings.copy(
                                    colorMode = WidgetColorMode.entries.first { it.name == name },
                                ),
                            )
                        },
                    )
                }
            }

            item {
                Row(
                    modifier = Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    TextButton(onClick = onCancel, modifier = Modifier.weight(1f)) {
                        Text("Cancel")
                    }
                    Button(onClick = { onSave(settings) }, modifier = Modifier.weight(1f)) {
                        Text("Save")
                    }
                }
            }

            item { Spacer(modifier = Modifier.height(24.dp)) }
        }
    }
}

@Composable
private fun ConfigSection(
    title: String,
    subtitle: String?,
    content: @Composable () -> Unit,
) {
    Surface(
        modifier = Modifier.fillMaxWidth(),
        shape = MaterialTheme.shapes.medium,
        color = MaterialTheme.colorScheme.surfaceContainer,
    ) {
        Column(
            modifier = Modifier.padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            SectionHeader(title = title, subtitle = subtitle)
            content()
        }
    }
}

/** A small group label inside a config section. */
@Composable
private fun ConfigLabel(text: String) {
    Text(
        text = text,
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** A wrapping row of chips, laid out by hand to avoid the FlowRow experimental API. */
@Composable
private fun ChipRow(
    options: List<Pair<String, String>>,
    selected: String?,
    selectedSet: Set<String> = emptySet(),
    onSelect: (String) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        options.chunked(3).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                row.forEach { (value, label) ->
                    FilterChip(
                        selected = selectedSet.contains(value) || (selected != null && selected == value),
                        onClick = { onSelect(value) },
                        label = { Text(label, maxLines = 1) },
                    )
                }
            }
        }
    }
}

private fun kindTitle(kind: WidgetKind): String = when (kind) {
    WidgetKind.Compact -> "Usage widget"
    WidgetKind.Providers -> "Providers widget"
    WidgetKind.Spend -> "Spend widget"
}

/** Convenience for the launcher entry points that need the config component. */
internal fun configComponent(context: Context): ComponentName =
    ComponentName(context, WidgetConfigActivity::class.java)
