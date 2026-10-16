package org.slopfire.codexbar.data.widget

import android.content.Context
import androidx.glance.appwidget.updateAll
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import org.slopfire.codexbar.data.CodexBarRepository
import org.slopfire.codexbar.data.model.SyncServerException
import org.slopfire.codexbar.data.net.SyncServerClient
import org.slopfire.codexbar.data.net.SyncServerUrls
import org.slopfire.codexbar.data.settings.SettingsStore

/**
 * Fetches the merged usage view and caches the trimmed payload widgets render.
 *
 * A widget update must never block on the network, so the flow is always
 * "render the cache, then refresh the cache in the background and re-render".
 */
object WidgetRefresher {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    /** How long a cached payload is treated as good enough to render. */
    private const val FRESH_FOR_MILLIS = 5 * 60 * 1000L

    /**
     * Reads the cache, fetching first when it is missing or stale.
     *
     * This is the only fetch path a render needs: it runs inside the Glance
     * session coroutine, which the framework keeps alive for the duration of the
     * render. A refresh launched from a broadcast receiver instead would race the
     * receiver's process being torn down, leaving a widget stranded on its
     * placeholder — which is exactly what happened before this existed.
     */
    suspend fun readFresh(context: Context): WidgetSnapshot? {
        val store = WidgetSnapshotStore(context)
        val cached = store.read()
        val age = System.currentTimeMillis() - (cached?.generatedAtMillis ?: 0L)
        if (cached != null && cached.isUsable && age < FRESH_FOR_MILLIS) {
            return cached
        }
        runCatching { refresh(context) }
        return store.read() ?: cached
    }

    /** Fetches and caches. Returns true when fresh data was stored. */
    suspend fun refresh(context: Context): Boolean {
        val settings = SettingsStore(context).read()
        val store = WidgetSnapshotStore(context)
        val host = SyncServerUrls.displayHost(settings.baseUrl)

        if (!settings.configured) {
            store.writeFailure(host, "Add your sync server in the app")
            return false
        }

        return try {
            val bundle = CodexBarRepository(SyncServerClient()).load(settings, provider = null)
            store.write(
                WidgetSnapshot.from(
                    host = host,
                    providers = bundle.snapshot.entries.map(WidgetProvider::from),
                ),
            )
            true
        } catch (error: Throwable) {
            val message = (error as? SyncServerException)?.message
                ?: error.message
                ?: "Could not reach the sync server"
            store.writeFailure(host, message)
            false
        }
    }

    /** Fire-and-forget refresh, for a broadcast receiver or a tap action. */
    fun refreshAsync(context: Context, onDone: (() -> Unit)? = null) {
        val appContext = context.applicationContext
        scope.launch {
            runCatching { refresh(appContext) }
            runCatching { updateAllWidgets(appContext) }
            onDone?.invoke()
        }
    }

    /**
     * Re-renders every widget instance. Kept here (rather than in the receivers)
     * so a single refresh updates all three widget types at once.
     */
    suspend fun updateAllWidgets(context: Context) {
        org.slopfire.codexbar.widget.CompactWidget().updateAll(context)
        org.slopfire.codexbar.widget.ProvidersWidget().updateAll(context)
        org.slopfire.codexbar.widget.SpendWidget().updateAll(context)
    }
}
