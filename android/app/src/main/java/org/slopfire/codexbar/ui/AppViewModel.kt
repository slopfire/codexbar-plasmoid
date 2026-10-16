package org.slopfire.codexbar.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import org.slopfire.codexbar.data.CodexBarRepository
import org.slopfire.codexbar.data.SyncBundle
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.settings.SettingsStore
import org.slopfire.codexbar.data.settings.SyncSettings

/**
 * Owns the refresh loop and the UI state for the whole app.
 *
 * The server already merges every device's snapshot, so the view model fetches
 * the whole `all` view once and filters locally: switching provider chips never
 * hits the network.
 */
class AppViewModel(
    private val repository: CodexBarRepository,
    private val settingsStore: SettingsStore,
) : ViewModel() {

    private val _state = MutableStateFlow(AppUiState(settings = settingsStore.read()))
    val state: StateFlow<AppUiState> = _state.asStateFlow()

    private var refreshJob: Job? = null

    init {
        if (_state.value.configured) {
            refresh(initial = true)
        }
    }

    /** Fetches the merged usage view. Concurrent calls collapse into one. */
    fun refresh(initial: Boolean = false) {
        if (refreshJob?.isActive == true) return
        val settings = _state.value.settings
        if (!settings.configured) {
            _state.value = _state.value.copy(
                loading = false,
                refreshing = false,
                error = "Add your sync server address in Settings",
            )
            return
        }
        refreshJob = viewModelScope.launch {
            _state.value = _state.value.copy(
                loading = initial && _state.value.snapshot == null,
                refreshing = true,
                error = null,
            )
            try {
                val bundle = repository.load(settings, provider = null)
                apply(bundle)
            } catch (error: Throwable) {
                _state.value = _state.value.copy(
                    loading = false,
                    refreshing = false,
                    error = describe(error),
                )
            }
        }
    }

    private fun apply(bundle: SyncBundle) {
        val previous = _state.value
        val selected = previous.selectedProvider?.takeIf { id ->
            bundle.snapshot.entries.any { it.provider == id }
        }
        _state.value = previous.copy(
            snapshot = bundle.snapshot,
            devices = bundle.devices,
            health = bundle.health ?: previous.health,
            healthError = bundle.healthError,
            healthCheckedAt = System.currentTimeMillis(),
            loading = false,
            refreshing = false,
            error = null,
            lastLoadedAt = System.currentTimeMillis(),
            selectedProvider = selected,
        )
    }

    /** Re-probes `/health` without touching the usage snapshot. */
    fun testConnection() {
        val settings = _state.value.settings
        if (!settings.configured) {
            _state.value = _state.value.copy(message = "Enter a server address first")
            return
        }
        viewModelScope.launch {
            _state.value = _state.value.copy(refreshing = true)
            val result = runCatching { repository.health(settings) }
            _state.value = _state.value.copy(
                refreshing = false,
                health = result.getOrNull() ?: _state.value.health,
                healthError = result.exceptionOrNull()?.let(::describe),
                healthCheckedAt = System.currentTimeMillis(),
                message = result.fold(
                    onSuccess = { health ->
                        "Connected to sync server${health.version?.let { " v$it" }.orEmpty()}"
                    },
                    onFailure = { error -> "Connection failed: ${describe(error)}" },
                ),
            )
            if (result.isSuccess) refresh()
        }
    }

    fun selectProvider(provider: String?) {
        _state.value = _state.value.copy(selectedProvider = provider)
    }

    fun setSortMode(mode: SortMode) {
        _state.value = _state.value.copy(sortMode = mode)
    }

    fun openDetail(entryKey: String) {
        _state.value = _state.value.copy(detailKey = entryKey)
    }

    fun closeDetail() {
        _state.value = _state.value.copy(detailKey = null)
    }

    /** Saves settings and reloads, because the server or token may have changed. */
    fun saveSettings(settings: SyncSettings) {
        val saved = settingsStore.save(settings)
        _state.value = _state.value.copy(settings = saved, message = "Settings saved")
        if (saved.configured) {
            refresh(initial = _state.value.snapshot == null)
        }
    }

    fun forgetDevice(device: String) {
        val settings = _state.value.settings
        viewModelScope.launch {
            _state.value = _state.value.copy(forgetting = device)
            val result = runCatching { repository.forgetDevice(settings, device) }
            _state.value = _state.value.copy(
                forgetting = null,
                message = result.fold(
                    onSuccess = { "Forgot $device" },
                    onFailure = { error -> "Could not forget $device: ${describe(error)}" },
                ),
            )
            if (result.isSuccess) refresh()
        }
    }

    fun clearMessage() {
        if (_state.value.message != null) {
            _state.value = _state.value.copy(message = null)
        }
    }

    private fun describe(error: Throwable): String = when (error) {
        is SyncServerException -> when (error.statusCode) {
            401 -> "The sync server rejected the token"
            0 -> error.message
            else -> error.message
        }

        else -> error.message ?: error::class.simpleName.orEmpty()
    }

    companion object {
        fun factory(
            repository: CodexBarRepository,
            settingsStore: SettingsStore,
        ): ViewModelProvider.Factory = object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: Class<T>): T =
                AppViewModel(repository, settingsStore) as T
        }
    }
}
