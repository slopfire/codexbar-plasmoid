package org.slopfire.codexbar.widget

import android.content.Context
import androidx.datastore.preferences.core.MutablePreferences
import androidx.glance.GlanceId
import androidx.glance.appwidget.GlanceAppWidget
import androidx.glance.appwidget.GlanceAppWidgetManager
import androidx.glance.appwidget.state.getAppWidgetState
import androidx.glance.appwidget.state.updateAppWidgetState
import androidx.glance.state.PreferencesGlanceStateDefinition
import androidx.datastore.preferences.core.Preferences

/**
 * Reads and writes one widget instance's settings.
 *
 * `getGlanceIdBy` works for an app widget id the host has not bound yet, so the
 * same path serves both the first configuration and a later reconfiguration.
 */
object WidgetSettingsStore {

    suspend fun read(context: Context, appWidgetId: Int): Preferences =
        getAppWidgetState(context, PreferencesGlanceStateDefinition, glanceId(context, appWidgetId))

    suspend fun write(
        context: Context,
        appWidgetId: Int,
        update: (MutablePreferences) -> Unit,
    ) {
        updateAppWidgetState(context, glanceId(context, appWidgetId), update)
    }

    /** Re-renders just this instance, so a reconfiguration applies immediately. */
    suspend fun refresh(context: Context, widget: GlanceAppWidget, appWidgetId: Int) {
        widget.update(context, glanceId(context, appWidgetId))
    }

    private fun glanceId(context: Context, appWidgetId: Int): GlanceId =
        GlanceAppWidgetManager(context).getGlanceIdBy(appWidgetId)
}
