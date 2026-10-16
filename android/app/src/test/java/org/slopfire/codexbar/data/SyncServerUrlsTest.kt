package org.slopfire.codexbar.data

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.slopfire.codexbar.data.net.SyncServerUrls

class SyncServerUrlsTest {

    @Test
    fun normalizeAddsScheme() {
        assertEquals("http://nas.local:8188", SyncServerUrls.normalize("nas.local:8188"))
    }

    @Test
    fun normalizeStripsTrailingSlash() {
        assertEquals("http://nas.local:8188", SyncServerUrls.normalize("http://nas.local:8188/"))
    }

    @Test
    fun normalizeTrimsWhitespace() {
        assertEquals("https://box.local:9000", SyncServerUrls.normalize("  https://box.local:9000  "))
    }

    @Test
    fun normalizeBlankIsNull() {
        assertNull(SyncServerUrls.normalize(""))
        assertNull(SyncServerUrls.normalize("   "))
        assertNull(SyncServerUrls.normalize(null))
    }

    @Test
    fun normalizeSchemelessOnlyIsNull() {
        assertNull(SyncServerUrls.normalize("http://"))
    }

    @Test
    fun normalizeDropsQuery() {
        assertEquals("http://nas.local:8188/usage", SyncServerUrls.normalize("http://nas.local:8188/usage?x=1"))
    }

    @Test
    fun routeAppendsQuery() {
        val url = SyncServerUrls.route("http://nas.local:8188", "v1", "usage", query = mapOf("provider" to "codex"))
        assertEquals("http://nas.local:8188/v1/usage?provider=codex", url!!.toString())
    }

    @Test
    fun routePercentEncodesPathSegment() {
        val url = SyncServerUrls.route("http://nas.local:8188", "v1", "snapshot", "weird device/name")
        assertEquals("http://nas.local:8188/v1/snapshot/weird%20device%2Fname", url!!.toString())
        assertTrue(url.toString().contains("weird%20device%2Fname"))
    }

    @Test
    fun displayHostStripsScheme() {
        assertEquals("nas.local:8188", SyncServerUrls.displayHost("http://nas.local:8188"))
    }

    @Test
    fun displayHostBlankIsEmpty() {
        assertEquals("", SyncServerUrls.displayHost(""))
        assertEquals("", SyncServerUrls.displayHost(null))
    }
}
