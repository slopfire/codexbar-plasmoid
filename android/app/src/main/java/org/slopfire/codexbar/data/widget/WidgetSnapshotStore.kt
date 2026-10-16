package org.slopfire.codexbar.data.widget

import android.content.Context

/**
 * App-private cache of the last widget payload.
 *
 * Deliberately a plain SharedPreferences file rather than Glance state: several
 * widget instances (and several widget sizes) share one payload, so refreshing
 * once must serve them all instead of storing a copy per widget id.
 */
class WidgetSnapshotStore(context: Context) {

    private val prefs = context.applicationContext
        .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    fun read(): WidgetSnapshot? = WidgetSnapshot.parse(prefs.getString(KEY_SNAPSHOT, null))

    fun write(snapshot: WidgetSnapshot) {
        prefs.edit().putString(KEY_SNAPSHOT, snapshot.toJson()).apply()
    }

    /** Records a failure without discarding the last good payload. */
    fun writeFailure(host: String, message: String): WidgetSnapshot {
        val previous = read()
        val next = WidgetSnapshot.failure(host, message, previous)
        write(next)
        return next
    }

    private companion object {
        const val PREFS_NAME = "codexbar-widget"
        const val KEY_SNAPSHOT = "snapshot"
    }
}
