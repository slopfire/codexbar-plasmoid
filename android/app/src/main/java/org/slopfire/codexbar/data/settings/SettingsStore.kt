package org.slopfire.codexbar.data.settings

import android.content.Context
import android.os.Build
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Persists [SyncSettings] in app-private SharedPreferences and exposes them as a
 * [StateFlow] so the UI updates the moment the user saves.
 *
 * The token lives in the same private store; the backup rules exclude this file
 * so a device transfer never carries the bearer token along.
 */
class SettingsStore(context: Context) {

    private val prefs = context.applicationContext
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private val _settings = MutableStateFlow(read())

    val settings: StateFlow<SyncSettings> = _settings.asStateFlow()

    fun read(): SyncSettings = SyncSettings(
        baseUrl = prefs.getString(KEY_BASE_URL, "").orEmpty(),
        token = prefs.getString(KEY_TOKEN, "").orEmpty(),
        deviceName = prefs.getString(KEY_DEVICE_NAME, "").orEmpty().ifBlank { defaultDeviceName() },
        autoRefreshSeconds = prefs.getInt(KEY_AUTO_REFRESH, DEFAULT_AUTO_REFRESH),
    )

    fun save(settings: SyncSettings): SyncSettings {
        val normalized = settings.copy(
            baseUrl = settings.baseUrl.trim(),
            token = settings.token.trim(),
            deviceName = settings.deviceName.trim().ifBlank { defaultDeviceName() },
            autoRefreshSeconds = if (settings.autoRefreshSeconds in VALID_REFRESH) {
                settings.autoRefreshSeconds
            } else {
                DEFAULT_AUTO_REFRESH
            },
        )
        prefs.edit()
            .putString(KEY_BASE_URL, normalized.baseUrl)
            .putString(KEY_TOKEN, normalized.token)
            .putString(KEY_DEVICE_NAME, normalized.deviceName)
            .putInt(KEY_AUTO_REFRESH, normalized.autoRefreshSeconds)
            .apply()
        _settings.value = normalized
        return normalized
    }

    /** The name this device publishes under, derived from the hardware model. */
    fun defaultDeviceName(): String {
        val model = Build.MODEL.orEmpty()
            .lowercase()
            .map { character ->
                if (character.isLetterOrDigit() || character == '-' || character == '_' || character == '.') {
                    character
                } else {
                    '-'
                }
            }
            .joinToString("")
            .trim('-')
        return "android-${model.ifBlank { "device" }}"
    }

    private companion object {
        const val PREFS_NAME = "codexbar-sync"
        const val KEY_BASE_URL = "baseUrl"
        const val KEY_TOKEN = "token"
        const val KEY_DEVICE_NAME = "deviceName"
        const val KEY_AUTO_REFRESH = "autoRefreshSeconds"
        const val DEFAULT_AUTO_REFRESH = 300
        val VALID_REFRESH = setOf(0, 60, 300, 900, 3600)
    }
}
