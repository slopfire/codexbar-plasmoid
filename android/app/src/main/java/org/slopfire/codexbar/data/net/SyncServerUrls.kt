package org.slopfire.codexbar.data.net

import java.net.URLEncoder
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * The user types a host, sometimes with a scheme and sometimes with a trailing
 * slash. Normalize once so every request and every error message agrees.
 */
object SyncServerUrls {

    /** Returns the base URL without a trailing slash, or null when unusable. */
    fun normalize(raw: String?): String? {
        val trimmed = raw?.trim().orEmpty()
        if (trimmed.isEmpty()) return null
        val withScheme = if (trimmed.contains("://")) trimmed else "http://$trimmed"
        val parsed: HttpUrl = withScheme.toHttpUrlOrNull() ?: return null
        if (parsed.host.isBlank()) return null
        return parsed.newBuilder()
            .query(null)
            .fragment(null)
            .build()
            .toString()
            .trimEnd('/')
    }

    /** Builds a `/v1/...` URL, percent-encoding any path segments. */
    fun route(baseUrl: String, vararg segments: String, query: Map<String, String> = emptyMap()): HttpUrl? {
        val base = baseUrl.trimEnd('/').toHttpUrlOrNull() ?: return null
        val builder = base.newBuilder()
        for (segment in segments) {
            builder.addPathSegment(segment)
        }
        for ((key, value) in query) {
            builder.addQueryParameter(key, value)
        }
        return builder.build()
    }

    /** Human-readable host for the UI, e.g. `nas.local:8188`. */
    fun displayHost(baseUrl: String?): String {
        val normalized = normalize(baseUrl) ?: return ""
        return normalized.substringAfter("://").trimEnd('/')
    }

    fun encodePathSegment(value: String): String =
        URLEncoder.encode(value, Charsets.UTF_8.name()).replace("+", "%20")
}
