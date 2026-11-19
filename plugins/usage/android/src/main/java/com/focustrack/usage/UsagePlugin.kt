package com.focustrack.usage

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class EventsArgs {
    var since: Long = 0
    var until: Long = 0
}

/**
 * FocusTrack's view of Android screen time: foreground and screen events from
 * UsageStatsManager. Needs "Usage access", which only the user can grant.
 */
@TauriPlugin
class UsagePlugin(private val activity: Activity) : Plugin(activity) {

    /** Status-bar and navigation-bar heights in CSS pixels (dp). */
    @Volatile private var insetTop = 0f
    @Volatile private var insetBottom = 0f

    /**
     * Android draws the app edge to edge, but the WebView doesn't report the
     * system bars to CSS (env(safe-area-inset-*) stays 0), so measure them here.
     */
    override fun load(webView: WebView) {
        super.load(webView)
        ViewCompat.setOnApplyWindowInsetsListener(webView) { v, insets ->
            val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
            val density = v.resources.displayMetrics.density
            insetTop = bars.top / density
            insetBottom = bars.bottom / density
            insets
        }
        webView.requestApplyInsets()
    }

    @Command
    fun insets(invoke: Invoke) {
        val ret = JSObject()
        ret.put("top", insetTop.toDouble())
        ret.put("bottom", insetBottom.toDouble())
        invoke.resolve(ret)
    }

    private fun granted(): Boolean = UsageReader.granted(activity)

    @Command
    fun hasAccess(invoke: Invoke) {
        val ret = JSObject()
        ret.put("granted", granted())
        invoke.resolve(ret)
    }

    @Command
    fun openAccessSettings(invoke: Invoke) {
        try {
            activity.startActivity(Intent(Settings.ACTION_USAGE_ACCESS_SETTINGS))
            invoke.resolve()
        } catch (e: Exception) {
            invoke.reject("Couldn't open the usage access settings: ${e.message}")
        }
    }

    @InvokeArg
    class BackgroundArgs {
        var dataDir: String = ""
    }

    @Command
    fun enableBackground(invoke: Invoke) {
        val args = invoke.parseArgs(BackgroundArgs::class.java)
        if (args.dataDir.isEmpty()) {
            invoke.reject("No data folder")
            return
        }
        BackgroundImport.schedule(activity.applicationContext, args.dataDir)
        invoke.resolve()
    }

    @Command
    fun events(invoke: Invoke) {
        if (!granted()) {
            invoke.reject("Usage access isn't granted")
            return
        }
        val args = invoke.parseArgs(EventsArgs::class.java)
        val read = UsageReader.read(activity, args.since, args.until)
        val ret = JSObject()
        ret.put("events", read.getJSONArray("events"))
        ret.put("labels", read.getJSONObject("labels"))
        ret.put("home", read.getJSONArray("home"))
        invoke.resolve(ret)
    }
}
