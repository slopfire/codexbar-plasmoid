package org.slopfire.codexbar.data.settings

/** Everything the user configures, persisted in app-private SharedPreferences. */
data class SyncSettings(
    /** e.g. `http://nas.local:8188`; empty until the user configures it. */
    val baseUrl: String = "",
    /** Bearer token for the `/v1` routes. */
    val token: String = "",
    /** Name this device publishes under; defaults to the Android model. */
    val deviceName: String = "",
    /** Auto-refresh cadence in seconds; 0 disables polling. */
    val autoRefreshSeconds: Int = 300,
) {
    val configured: Boolean get() = baseUrl.isNotBlank()

    companion object {
        val AUTO_REFRESH_CHOICES = listOf(0, 60, 300, 900, 3600)

        fun label(seconds: Int): String = when (seconds) {
            0 -> "Off"
            60 -> "1 min"
            300 -> "5 min"
            900 -> "15 min"
            3600 -> "1 hour"
            else -> "${seconds}s"
        }
    }
}
