package org.slopfire.codexbar.ui

import org.slopfire.codexbar.data.model.DeviceList
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.model.ProviderEntry
import org.slopfire.codexbar.data.model.UsageSnapshot
import org.slopfire.codexbar.data.settings.SyncSettings

/** Top-level destinations in the short navigation bar. */
enum class AppTab { Usage, Devices, Settings }

/** How the usage list is ordered. */
enum class SortMode(val label: String) {
    Attention("Tightest first"),
    Name("Name"),
    Updated("Recently updated"),
}

/**
 * One immutable snapshot of everything the UI renders. Derived lists are
 * computed properties so screens never re-derive them inconsistently.
 */
data class AppUiState(
    val settings: SyncSettings = SyncSettings(),
    val snapshot: UsageSnapshot? = null,
    val devices: DeviceList? = null,
    val health: HealthInfo? = null,
    val healthError: String? = null,
    val healthCheckedAt: Long? = null,
    val loading: Boolean = false,
    val refreshing: Boolean = false,
    val error: String? = null,
    val lastLoadedAt: Long? = null,
    val selectedProvider: String? = null,
    val sortMode: SortMode = SortMode.Attention,
    val detailKey: String? = null,
    val forgetting: String? = null,
    val message: String? = null,
) {

    /** True once a server address is saved; drives the first-run empty state. */
    val configured: Boolean get() = settings.baseUrl.isNotBlank()

    val entries: List<ProviderEntry> get() = snapshot?.entries.orEmpty()

    /** Provider ids present in the snapshot, in display order. */
    val providerIds: List<String>
        get() = entries.map { it.provider }.distinct().sortedBy { it.lowercase() }

    /** Entries after the provider filter and the sort mode. */
    val visibleEntries: List<ProviderEntry>
        get() {
            val filtered = selectedProvider
                ?.let { selected -> entries.filter { it.provider == selected } }
                ?: entries
            return when (sortMode) {
                SortMode.Attention -> filtered.sortedBy { entry -> tightness(entry) }
                SortMode.Name -> filtered.sortedBy { entry -> entry.provider.lowercase() }
                SortMode.Updated -> filtered.sortedByDescending { entry -> entry.updatedAt.orEmpty() }
            }
        }

    /** The entry the detail screen is showing, if any. */
    val detailEntry: ProviderEntry? get() = entries.firstOrNull { it.key == detailKey }

    val totalTrackedDevices: Int get() = devices?.devices?.size ?: 0

    /** Worst-case remaining percent across every provider, for the tray-style summary. */
    val tightestPercentLeft: Double?
        get() = entries.flatMap { it.rows }.map { it.percentLeft }.minOrNull()

    val hasAnyLimits: Boolean get() = entries.any { it.rows.isNotEmpty() }

    val hasAnyCost: Boolean get() = entries.any { it.tokenUsage != null || it.dailyUsage.isNotEmpty() }

    private fun tightness(entry: ProviderEntry): Double =
        entry.rows.minOfOrNull { it.percentLeft } ?: 100.0
}
