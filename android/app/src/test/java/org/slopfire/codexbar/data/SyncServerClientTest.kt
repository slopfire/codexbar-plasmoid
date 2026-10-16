package org.slopfire.codexbar.data

import com.sun.net.httpserver.HttpExchange
import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import java.util.Collections
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.net.SyncServerClient

/**
 * Drives the real OkHttp client against a throwaway `com.sun.net.httpserver`
 * instance so the URL building, headers and error mapping are all exercised
 * over a socket without any Android dependency.
 */
class SyncServerClientTest {

    private data class Recorded(
        val method: String,
        val path: String,
        val query: String?,
        val authorization: String?,
        val body: String,
    )

    private lateinit var server: HttpServer
    private lateinit var baseUrl: String
    private val client = SyncServerClient()
    private val recorded = Collections.synchronizedList(mutableListOf<Recorded>())

    @Volatile
    private var handler: (HttpExchange) -> Unit = {}

    @Before
    fun setUp() {
        server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/") { exchange ->
            recorded += Recorded(
                method = exchange.requestMethod,
                path = exchange.requestURI.path,
                query = exchange.requestURI.rawQuery,
                authorization = exchange.requestHeaders.getFirst("Authorization"),
                body = exchange.requestBody.readBytes().toString(Charsets.UTF_8),
            )
            handler(exchange)
        }
        server.executor = null
        server.start()
        baseUrl = "http://127.0.0.1:${server.address.port}"
    }

    @After
    fun tearDown() {
        server.stop(0)
    }

    // ------------------------------------------------------------------
    // Happy paths
    // ------------------------------------------------------------------

    @Test
    fun healthHitsHealthWithoutAuthorization() = runTest {
        respond(200, """{"status":"ok","version":"0.1.11"}""")

        val info = client.health(baseUrl, "secret")

        assertEquals("ok", info.status)
        assertEquals("0.1.11", info.version)
        assertEquals(1, recorded.size)
        assertEquals("GET", recorded[0].method)
        assertEquals("/health", recorded[0].path)
        assertNull(recorded[0].query)
        assertNull(recorded[0].authorization)
    }

    @Test
    fun usageSendsProviderQueryAndBearerToken() = runTest {
        respond(200, USAGE_PAYLOAD)

        val snapshot = client.usage(baseUrl, "secret", "codex")

        assertEquals(1, snapshot.entries.size)
        assertEquals("codex", snapshot.entries[0].provider)
        assertEquals(2, snapshot.entries[0].rows.size)
        assertEquals(1, recorded.size)
        assertEquals("GET", recorded[0].method)
        assertEquals("/v1/usage", recorded[0].path)
        assertEquals("provider=codex", recorded[0].query)
        assertEquals("Bearer secret", recorded[0].authorization)
    }

    @Test
    fun usageWithoutProviderOmitsQuery() = runTest {
        respond(200, EMPTY_ENTRIES)

        client.usage(baseUrl, "secret", null)

        assertEquals(1, recorded.size)
        assertEquals("/v1/usage", recorded[0].path)
        assertNull(recorded[0].query)
        assertEquals("Bearer secret", recorded[0].authorization)
    }

    @Test
    fun usageWithAllProviderOmitsQuery() = runTest {
        respond(200, EMPTY_ENTRIES)

        client.usage(baseUrl, "secret", "all")

        assertEquals(1, recorded.size)
        assertEquals("/v1/usage", recorded[0].path)
        assertNull(recorded[0].query)
    }

    @Test
    fun devicesHitsDevicesWithBearerToken() = runTest {
        respond(200, """{"ok":true,"devices":[{"device":"alpha","entryCount":2}],"corrupt":1}""")

        val list = client.devices(baseUrl, "secret")

        assertEquals(1, list.devices.size)
        assertEquals("alpha", list.devices[0].device)
        assertEquals(2, list.devices[0].entryCount)
        assertEquals(1, list.corrupt)
        assertEquals(1, recorded.size)
        assertEquals("GET", recorded[0].method)
        assertEquals("/v1/devices", recorded[0].path)
        assertEquals("Bearer secret", recorded[0].authorization)
    }

    @Test
    fun forgetDeviceSendsDeleteWithBearerToken() = runTest {
        respond(200, """{"ok":true,"device":"old-box"}""")

        client.forgetDevice(baseUrl, "secret", "old-box")

        assertEquals(1, recorded.size)
        assertEquals("DELETE", recorded[0].method)
        assertEquals("/v1/snapshot/old-box", recorded[0].path)
        assertEquals("Bearer secret", recorded[0].authorization)
    }

    // ------------------------------------------------------------------
    // Error mapping
    // ------------------------------------------------------------------

    @Test
    fun unauthorizedMapsToException() = runTest {
        respond(401, """{"ok":false,"error":"unauthorized"}""")

        val error = failure { client.usage(baseUrl, "secret", null) }

        assertEquals(401, error.statusCode)
        assertEquals("unauthorized", error.message)
    }

    @Test
    fun serverErrorWithHtmlBodyMentionsHttpStatus() = runTest {
        respond(500, "<html>oops</html>", "text/html")

        val error = failure { client.usage(baseUrl, "secret", null) }

        assertEquals(500, error.statusCode)
        assertTrue(error.message.contains("HTTP 500"))
    }

    @Test
    fun emptySuccessfulBodyThrows() = runTest {
        respond(200, "")

        val error = failure { client.usage(baseUrl, "secret", null) }

        assertEquals(200, error.statusCode)
        assertTrue(error.message.contains("empty"))
    }

    @Test
    fun missingTokenFailsBeforeAnyHttpCall() = runTest {
        respond(200, EMPTY_ENTRIES)

        val error = failure { client.usage(baseUrl, null, null) }

        assertEquals(401, error.statusCode)
        assertTrue(recorded.isEmpty())
    }

    @Test
    fun unreachableServerMapsToStatusZero() = runTest {
        val deadPort = allocateClosedPort()

        val error = failure { client.usage("http://127.0.0.1:$deadPort", "secret", null) }

        assertEquals(0, error.statusCode)
    }

    @Test
    fun invalidBaseUrlMapsToStatusZero() = runTest {
        val error = failure { client.usage("", "secret", null) }

        assertEquals(0, error.statusCode)
        assertTrue(error.message.contains("Enter a sync server address"))
        assertTrue(recorded.isEmpty())
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /** Binds then immediately releases an ephemeral port so nothing listens on it. */
    private fun allocateClosedPort(): Int {
        val probe = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        val port = probe.address.port
        probe.stop(0)
        return port
    }

    private fun respond(status: Int, body: String, contentType: String = "application/json") {
        handler = { exchange -> write(exchange, status, body, contentType) }
    }

    private fun write(exchange: HttpExchange, status: Int, body: String, contentType: String) {
        val bytes = body.toByteArray(Charsets.UTF_8)
        exchange.responseHeaders.add("Content-Type", contentType)
        if (bytes.isEmpty()) {
            exchange.sendResponseHeaders(status, -1)
        } else {
            exchange.sendResponseHeaders(status, bytes.size.toLong())
            exchange.responseBody.use { it.write(bytes) }
        }
        exchange.close()
    }

    private suspend fun failure(block: suspend () -> Unit): SyncServerException {
        try {
            block()
        } catch (error: SyncServerException) {
            return error
        }
        throw AssertionError("Expected SyncServerException")
    }

    private companion object {
        const val EMPTY_ENTRIES = """{"ok":true,"requestedProvider":"codex","entries":[]}"""

        const val USAGE_PAYLOAD = """
        {"ok":true,"requestedProvider":"codex","entries":[
          {"provider":"codex","account":"user@example.com",
           "rows":[
             {"id":"primary","title":"Session","percentLeft":42.5},
             {"id":"secondary","title":"Weekly","percentLeft":88}
           ]}
        ]}
        """
    }
}
