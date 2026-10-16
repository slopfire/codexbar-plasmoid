package org.slopfire.codexbar.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Intent
import android.content.Context
import android.os.Build

/**
 * Asks the launcher to place a widget on the home screen.
 *
 * `requestPinAppWidget` shows the launcher's own confirmation, so the app never
 * places a widget behind the user's back, and it is the only reliable way to
 * offer "add this widget" from inside the app.
 */
object WidgetPinner {

    val compact = WidgetTarget("Usage", "One provider, its tightest limit", CompactWidgetReceiver::class.java)
    val providers = WidgetTarget("Providers", "Every merged provider, tightest first", ProvidersWidgetReceiver::class.java)
    val spend = WidgetTarget("Spend", "Today and 30-day local cost", SpendWidgetReceiver::class.java)

    val all: List<WidgetTarget> get() = listOf(compact, providers, spend)

    fun canPin(context: Context): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
            AppWidgetManager.getInstance(context).isRequestPinAppWidgetSupported

    fun pin(context: Context, target: WidgetTarget): Boolean {
        if (!canPin(context)) return false
        val provider = ComponentName(context, target.receiver)
        return AppWidgetManager.getInstance(context).requestPinAppWidget(provider, null, null)
    }

    /** How many instances of this widget are already on the home screen. */
    fun placedCount(context: Context, target: WidgetTarget): Int =
        AppWidgetManager.getInstance(context)
            .getAppWidgetIds(ComponentName(context, target.receiver))
            .size

    /**
     * Opens the configuration for one placed instance.
     *
     * The launcher's pin flow deliberately skips `android:configure`, and
     * long-press → Edit only exists from Android 12, so the app offers its own way
     * in: every placed instance can be configured from here.
     */
    fun configure(context: Context, appWidgetId: Int) {
        context.startActivity(
            Intent(context, WidgetConfigActivity::class.java).apply {
                action = AppWidgetManager.ACTION_APPWIDGET_CONFIGURE
                putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, appWidgetId)
            },
        )
    }

    /** The first placed instance of this widget, or null when there is none. */
    fun firstPlacedId(context: Context, target: WidgetTarget): Int? =
        AppWidgetManager.getInstance(context)
            .getAppWidgetIds(ComponentName(context, target.receiver))
            .firstOrNull()
}

/** One pinnable widget: its label, its pitch, and its receiver. */
data class WidgetTarget(
    val label: String,
    val description: String,
    val receiver: Class<*>,
)
