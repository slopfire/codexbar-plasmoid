package org.slopfire.codexbar.ui

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.BarChart
import androidx.compose.material.icons.filled.Devices
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.LoadingIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.MediumFlexibleTopAppBar
import androidx.compose.material3.Scaffold
import androidx.compose.material3.ShortNavigationBar
import androidx.compose.material3.ShortNavigationBarItem
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.delay
import org.slopfire.codexbar.data.net.SyncServerUrls
import org.slopfire.codexbar.ui.common.LoadingBlock
import org.slopfire.codexbar.ui.detail.ProviderDetailScreen
import org.slopfire.codexbar.ui.devices.DevicesScreen
import org.slopfire.codexbar.ui.settings.SettingsScreen
import org.slopfire.codexbar.ui.usage.UsageScreen

/**
 * App root: the expressive short navigation bar, the shared top bar, the detail
 * route, and the auto-refresh loop.
 */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun CodexBarApp(viewModel: AppViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var tab by rememberSaveable { mutableStateOf(AppTab.Usage) }
    val snackbarHostState = remember { SnackbarHostState() }

    // Auto-refresh: the server merges devices, so a slow cadence is plenty.
    LaunchedEffect(state.settings.baseUrl, state.settings.autoRefreshSeconds) {
        val seconds = state.settings.autoRefreshSeconds
        if (seconds <= 0 || state.settings.baseUrl.isBlank()) return@LaunchedEffect
        while (true) {
            delay(seconds * 1000L)
            viewModel.refresh()
        }
    }

    LaunchedEffect(state.message) {
        val message = state.message ?: return@LaunchedEffect
        snackbarHostState.showSnackbar(message)
        viewModel.clearMessage()
    }

    // The detail screen owns the whole surface so a long history chart has room.
    // It replaces the Scaffold, so it has to claim the system back gesture itself
    // — without this, back finishes the activity and drops the user out of the app.
    val detail = state.detailEntry
    BackHandler(enabled = detail != null) { viewModel.closeDetail() }
    if (detail != null) {
        ProviderDetailScreen(
            entry = detail,
            onBack = viewModel::closeDetail,
            onRefresh = { viewModel.refresh() },
            refreshing = state.refreshing,
        )
        return
    }

    // The bar folds from the medium flexible height down to the small height as
    // the list scrolls, so a long provider list keeps most of the screen.
    val scrollBehavior = TopAppBarDefaults.exitUntilCollapsedScrollBehavior()

    Scaffold(
        modifier = Modifier.nestedScroll(scrollBehavior.nestedScrollConnection),
        topBar = {
            MediumFlexibleTopAppBar(
                title = {
                    Text(
                        text = tabTitle(tab),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                subtitle = {
                    Text(
                        text = serverSubtitle(state),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                actions = {
                    // One action, so it gets the filled tonal style: the bar's
                    // only control is also its primary one.
                    FilledTonalIconButton(
                        onClick = { viewModel.refresh() },
                        enabled = !state.refreshing,
                        modifier = Modifier.padding(end = 4.dp),
                    ) {
                        if (state.refreshing) {
                            LoadingIndicator(
                                modifier = Modifier.size(22.dp),
                                color = LocalContentColor.current,
                            )
                        } else {
                            Icon(Icons.Default.Refresh, contentDescription = "Refresh")
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.surface,
                    scrolledContainerColor = MaterialTheme.colorScheme.surfaceContainer,
                ),
                scrollBehavior = scrollBehavior,
            )
        },
        bottomBar = {
            ShortNavigationBar {
                AppTab.entries.forEach { item ->
                    ShortNavigationBarItem(
                        selected = tab == item,
                        onClick = { tab = item },
                        icon = { Icon(tabIcon(item), contentDescription = null) },
                        label = { Text(tabTitle(item)) },
                    )
                }
            }
        },
        snackbarHost = { SnackbarHost(snackbarHostState) },
        containerColor = MaterialTheme.colorScheme.surface,
    ) { padding ->
        val contentModifier = Modifier.padding(padding)

        if (state.loading && state.snapshot == null) {
            LoadingBlock(
                message = "Loading merged usage from ${SyncServerUrls.displayHost(state.settings.baseUrl)}…",
                modifier = contentModifier,
            )
            return@Scaffold
        }

        // Expressive springs from the theme's motion scheme: a spatial spring
        // for the move, an effects spring for the fade.
        val effectsSpec = MaterialTheme.motionScheme.defaultEffectsSpec<Float>()
        val spatialSpec = MaterialTheme.motionScheme.defaultSpatialSpec<Float>()
        val offsetSpec = MaterialTheme.motionScheme.defaultSpatialSpec<IntOffset>()
        AnimatedContent(
            targetState = tab,
            transitionSpec = {
                (
                    fadeIn(animationSpec = effectsSpec) +
                        scaleIn(initialScale = 0.97f, animationSpec = spatialSpec) +
                        slideInHorizontally(animationSpec = offsetSpec) { width -> width / 14 }
                    ) togetherWith (
                    fadeOut(animationSpec = effectsSpec) +
                        scaleOut(targetScale = 0.97f, animationSpec = spatialSpec) +
                        slideOutHorizontally(animationSpec = offsetSpec) { width -> -width / 14 }
                    )
            },
            label = "tab",
            modifier = Modifier.fillMaxSize(),
        ) { current ->
            when (current) {
                AppTab.Usage -> UsageScreen(
                    state = state,
                    onRefresh = { viewModel.refresh() },
                    onSelectProvider = viewModel::selectProvider,
                    onSortChange = viewModel::setSortMode,
                    onOpenEntry = viewModel::openDetail,
                    modifier = contentModifier,
                )

                AppTab.Devices -> DevicesScreen(
                    state = state,
                    onRefresh = { viewModel.refresh() },
                    onForgetDevice = viewModel::forgetDevice,
                    modifier = contentModifier,
                )

                AppTab.Settings -> SettingsScreen(
                    state = state,
                    onSave = viewModel::saveSettings,
                    onTestConnection = viewModel::testConnection,
                    modifier = contentModifier,
                )
            }
        }
    }
}

private fun tabTitle(tab: AppTab): String = when (tab) {
    AppTab.Usage -> "Usage"
    AppTab.Devices -> "Devices"
    AppTab.Settings -> "Settings"
}

private fun tabIcon(tab: AppTab): ImageVector = when (tab) {
    AppTab.Usage -> Icons.Default.BarChart
    AppTab.Devices -> Icons.Default.Devices
    AppTab.Settings -> Icons.Default.Settings
}

private fun serverSubtitle(state: AppUiState): String {
    val host = SyncServerUrls.displayHost(state.settings.baseUrl)
    return when {
        !state.configured -> "No sync server configured"
        state.health != null -> "$host · connected${state.health?.version?.let { " v$it" }.orEmpty()}"
        state.healthError != null -> "$host · health check failed"
        else -> host
    }
}
