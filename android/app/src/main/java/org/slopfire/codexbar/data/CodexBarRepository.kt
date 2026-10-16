package org.slopfire.codexbar.data

import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import org.slopfire.codexbar.data.model.DeviceList
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.model.UsageSnapshot
import org.slopfire.codexbar.data.net.CodexBarApi
import org.slopfire.codexbar.data.settings.SyncSettings

/** Everything one refresh produces. */
data class SyncBundle(
    val snapshot: UsageSnapshot,
    val devices: DeviceList?,
    val health: HealthInfo?,
    val healthError: String?,
)

/**
 * Reads the merged cross-device view from the sync server.
 *
 * `/health` is best-effort: a healthy `/v1/usage` response is far more
 * meaningful, so a failing health probe only annotates the UI.
 */
class CodexBarRepository(private val api: CodexBarApi) {

    suspend fun load(settings: SyncSettings, provider: String?): SyncBundle = coroutineScope {
        val usage = async { api.usage(settings.baseUrl, settings.token, provider) }
        val devices = async {
            runCatching { api.devices(settings.baseUrl, settings.token) }.getOrNull()
        }
        val health = async {
            runCatching { api.health(settings.baseUrl, settings.token) }
        }

        val snapshot = usage.await()
        val healthResult = health.await()
        SyncBundle(
            snapshot = snapshot,
            devices = devices.await(),
            health = healthResult.getOrNull(),
            healthError = healthResult.exceptionOrNull()?.let { describe(it) },
        )
    }

    suspend fun health(settings: SyncSettings): HealthInfo = api.health(settings.baseUrl, settings.token)

    suspend fun forgetDevice(settings: SyncSettings, device: String) {
        api.forgetDevice(settings.baseUrl, settings.token, device)
    }

    private fun describe(error: Throwable): String =
        (error as? SyncServerException)?.message ?: error.message ?: error::class.simpleName.orEmpty()
}
