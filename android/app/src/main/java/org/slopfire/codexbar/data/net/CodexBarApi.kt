package org.slopfire.codexbar.data.net

import org.slopfire.codexbar.data.model.DeviceList
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.model.UsageSnapshot

/**
 * Everything the app needs from the CodexBar sync server. Kept as an interface
 * so the repository can be exercised in unit tests without a socket.
 *
 * Implementations throw [org.slopfire.codexbar.data.model.SyncServerException]
 * on any failure.
 */
interface CodexBarApi {
    /** `GET /health` — the only route that does not need a token. */
    suspend fun health(baseUrl: String, token: String?): HealthInfo

    /** `GET /v1/usage[?provider=<id>]` — merged, cross-device entries. */
    suspend fun usage(baseUrl: String, token: String?, provider: String? = null): UsageSnapshot

    /** `GET /v1/devices` — which devices have published a snapshot. */
    suspend fun devices(baseUrl: String, token: String?): DeviceList

    /** `DELETE /v1/snapshot/<device>` — forget a renamed or reinstalled device. */
    suspend fun forgetDevice(baseUrl: String, token: String?, device: String)
}
