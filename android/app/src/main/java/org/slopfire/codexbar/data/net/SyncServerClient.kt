package org.slopfire.codexbar.data.net

import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import org.json.JSONObject
import org.slopfire.codexbar.data.model.DeviceList
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.model.UsageSnapshot
import org.slopfire.codexbar.data.model.parseDeviceList
import org.slopfire.codexbar.data.model.parseHealth
import org.slopfire.codexbar.data.model.parseUsageSnapshot

/**
 * HTTP client for the CodexBar self-hosted sync server.
 *
 * The server is plain HTTP on a LAN or VPN address and authenticates with a
 * bearer token, so every route is one small JSON request and response.
 */
class SyncServerClient(
    private val client: OkHttpClient = defaultClient(),
) : CodexBarApi {

    override suspend fun health(baseUrl: String, token: String?): HealthInfo =
        execute(baseUrl) { base ->
            Call(
                method = "GET",
                url = SyncServerUrls.route(base, "health")?.toString()
                    ?: throw SyncServerException(0, INVALID_URL),
                token = null,
                requiresToken = false,
            )
        }.let(::parseHealth)

    override suspend fun usage(baseUrl: String, token: String?, provider: String?): UsageSnapshot =
        execute(baseUrl) { base ->
            val query = if (provider.isNullOrBlank() || provider == "all") {
                emptyMap()
            } else {
                mapOf("provider" to provider)
            }
            Call(
                method = "GET",
                url = SyncServerUrls.route(base, "v1", "usage", query = query)?.toString()
                    ?: throw SyncServerException(0, INVALID_URL),
                token = token,
                requiresToken = true,
            )
        }.let(::parseUsageSnapshot)

    override suspend fun devices(baseUrl: String, token: String?): DeviceList =
        execute(baseUrl) { base ->
            Call(
                method = "GET",
                url = SyncServerUrls.route(base, "v1", "devices")?.toString()
                    ?: throw SyncServerException(0, INVALID_URL),
                token = token,
                requiresToken = true,
            )
        }.let(::parseDeviceList)

    override suspend fun forgetDevice(baseUrl: String, token: String?, device: String) {
        execute(baseUrl) { base ->
            Call(
                method = "DELETE",
                url = SyncServerUrls.route(base, "v1", "snapshot", device)?.toString()
                    ?: throw SyncServerException(0, INVALID_URL),
                token = token,
                requiresToken = true,
            )
        }
    }

    private data class Call(
        val method: String,
        val url: String,
        val token: String?,
        val requiresToken: Boolean,
    )

    /** Runs one request and returns the parsed JSON body, mapping every failure. */
    private suspend fun execute(baseUrl: String, build: (String) -> Call): JSONObject =
        withContext(Dispatchers.IO) {
            val base = SyncServerUrls.normalize(baseUrl)
                ?: throw SyncServerException(0, "Enter a sync server address such as http://nas.local:8188")
            val call = build(base)

            val builder = Request.Builder()
                .url(call.url)
                .header("Accept", "application/json")
            when (call.method) {
                "DELETE" -> builder.delete(EMPTY_BODY)
                else -> builder.get()
            }
            if (call.requiresToken) {
                val token = call.token?.trim().orEmpty()
                if (token.isEmpty()) {
                    throw SyncServerException(401, "This route needs the sync server token")
                }
                builder.header("Authorization", "Bearer $token")
            }

            val response = try {
                client.newCall(builder.build()).execute()
            } catch (error: IOException) {
                throw SyncServerException(0, error.message ?: "Could not reach the sync server", error)
            }
            response.use(::readJson)
        }

    private fun readJson(response: Response): JSONObject {
        val body = try {
            response.body?.string().orEmpty()
        } catch (error: IOException) {
            throw SyncServerException(response.code, "Could not read the response body", error)
        }
        val json = body.trim().takeIf { it.isNotEmpty() }?.let { text ->
            try {
                JSONObject(text)
            } catch (_: Exception) {
                null
            }
        }
        if (!response.isSuccessful) {
            val message = json?.optString("error")?.takeIf { it.isNotBlank() }
                ?: "Sync server returned HTTP ${response.code}"
            throw SyncServerException(response.code, message)
        }
        return json ?: throw SyncServerException(response.code, "Sync server returned an empty response")
    }

    companion object {
        private const val INVALID_URL = "Sync server URL is not valid"
        private val EMPTY_BODY = ByteArray(0).toRequestBody("application/json".toMediaType())

        fun defaultClient(): OkHttpClient = OkHttpClient.Builder()
            .connectTimeout(10, TimeUnit.SECONDS)
            .readTimeout(25, TimeUnit.SECONDS)
            .writeTimeout(25, TimeUnit.SECONDS)
            .callTimeout(35, TimeUnit.SECONDS)
            .retryOnConnectionFailure(true)
            .build()
    }
}
