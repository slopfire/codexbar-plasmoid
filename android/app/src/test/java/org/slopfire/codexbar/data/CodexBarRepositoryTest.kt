package org.slopfire.codexbar.data

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Test
import org.slopfire.codexbar.data.model.DeviceList
import org.slopfire.codexbar.data.model.HealthInfo
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.model.UsageSnapshot
import org.slopfire.codexbar.data.net.CodexBarApi
import org.slopfire.codexbar.data.settings.SyncSettings

class CodexBarRepositoryTest {

    private val settings = SyncSettings(baseUrl = "http://nas.local:8188", token = "tok")

    @Test
    fun successfulLoadReturnsSnapshotDevicesAndHealth() = runTest {
        val api = FakeApi()
        val snapshot = UsageSnapshot("2026-10-15T04:00:00Z", "all", emptyList(), null)
        api.usageResult = snapshot
        api.devicesResult = DeviceList(emptyList(), corrupt = 3)
        api.healthResult = HealthInfo("ok", "0.1.11")

        val bundle = CodexBarRepository(api).load(settings, "codex")

        assertSame(snapshot, bundle.snapshot)
        assertEquals(3, bundle.devices?.corrupt ?: -1)
        assertEquals("0.1.11", bundle.health?.version)
        assertNull(bundle.healthError)
    }

    @Test
    fun failingHealthDoesNotFailLoad() = runTest {
        val api = FakeApi()
        api.healthFailure = SyncServerException(503, "health down")

        val bundle = CodexBarRepository(api).load(settings, null)

        assertNull(bundle.health)
        assertEquals("health down", bundle.healthError)
        assertNotNull(bundle.snapshot)
    }

    @Test
    fun failingDevicesDoesNotFailLoad() = runTest {
        val api = FakeApi()
        api.devicesFailure = SyncServerException(500, "devices down")

        val bundle = CodexBarRepository(api).load(settings, null)

        assertNull(bundle.devices)
        assertNotNull(bundle.snapshot)
    }

    @Test
    fun failingUsagePropagates() = runTest {
        val api = FakeApi()
        api.usageFailure = SyncServerException(401, "unauthorized")

        val error = failure { CodexBarRepository(api).load(settings, null) }

        assertEquals(401, error.statusCode)
        assertEquals("unauthorized", error.message)
    }

    @Test
    fun loadPassesSettingsAndProviderThrough() = runTest {
        val api = FakeApi()

        CodexBarRepository(api).load(settings, "codex")

        assertEquals(Triple("http://nas.local:8188", "tok", "codex"), api.usageCalls.single())
        assertEquals("http://nas.local:8188" to "tok", api.deviceCalls.single())
        assertEquals("http://nas.local:8188" to "tok", api.healthCalls.single())
    }

    @Test
    fun forgetDevicePassesDeviceNameThrough() = runTest {
        val api = FakeApi()

        CodexBarRepository(api).forgetDevice(settings, "old-box")

        assertEquals(Triple("http://nas.local:8188", "tok", "old-box"), api.forgetCalls.single())
    }

    @Test
    fun healthDelegatesToApi() = runTest {
        val api = FakeApi()
        api.healthResult = HealthInfo("ok", "0.1.11")

        val info = CodexBarRepository(api).health(settings)

        assertEquals("ok", info.status)
        assertEquals("0.1.11", info.version)
        assertEquals("http://nas.local:8188" to "tok", api.healthCalls.single())
    }

    @Test
    fun healthPropagatesFailure() = runTest {
        val api = FakeApi()
        api.healthFailure = SyncServerException(500, "boom")

        val error = failure { CodexBarRepository(api).health(settings) }

        assertEquals(500, error.statusCode)
        assertEquals("boom", error.message)
    }

    private suspend fun failure(block: suspend () -> Unit): SyncServerException {
        try {
            block()
        } catch (error: SyncServerException) {
            return error
        }
        throw AssertionError("Expected SyncServerException")
    }

    private class FakeApi : CodexBarApi {
        var usageResult: UsageSnapshot = UsageSnapshot(null, null, emptyList(), null)
        var usageFailure: Throwable? = null
        var devicesResult: DeviceList = DeviceList(emptyList())
        var devicesFailure: Throwable? = null
        var healthResult: HealthInfo = HealthInfo("ok", null)
        var healthFailure: Throwable? = null

        val usageCalls = mutableListOf<Triple<String, String?, String?>>()
        val deviceCalls = mutableListOf<Pair<String, String?>>()
        val healthCalls = mutableListOf<Pair<String, String?>>()
        val forgetCalls = mutableListOf<Triple<String, String?, String>>()

        override suspend fun health(baseUrl: String, token: String?): HealthInfo {
            healthCalls += baseUrl to token
            healthFailure?.let { throw it }
            return healthResult
        }

        override suspend fun usage(baseUrl: String, token: String?, provider: String?): UsageSnapshot {
            usageCalls += Triple(baseUrl, token, provider)
            usageFailure?.let { throw it }
            return usageResult
        }

        override suspend fun devices(baseUrl: String, token: String?): DeviceList {
            deviceCalls += baseUrl to token
            devicesFailure?.let { throw it }
            return devicesResult
        }

        override suspend fun forgetDevice(baseUrl: String, token: String?, device: String) {
            forgetCalls += Triple(baseUrl, token, device)
        }
    }
}
