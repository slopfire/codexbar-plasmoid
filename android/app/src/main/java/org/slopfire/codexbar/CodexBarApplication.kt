package org.slopfire.codexbar

import android.app.Application
import org.slopfire.codexbar.data.CodexBarRepository
import org.slopfire.codexbar.data.net.SyncServerClient
import org.slopfire.codexbar.data.settings.SettingsStore

/**
 * Process-wide singletons: one HTTP client, one settings store, one repository.
 * Small enough that a DI framework would only add indirection.
 */
class AppContainer(application: Application) {
    val settingsStore: SettingsStore = SettingsStore(application)
    val api: SyncServerClient = SyncServerClient()
    val repository: CodexBarRepository = CodexBarRepository(api)
}

class CodexBarApplication : Application() {

    lateinit var container: AppContainer
        private set

    override fun onCreate() {
        super.onCreate()
        container = AppContainer(this)
    }
}
