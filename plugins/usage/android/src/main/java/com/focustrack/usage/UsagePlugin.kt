package com.focustrack.usage

import android.app.Activity
import android.app.AppOpsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import android.provider.Settings
import android.webkit.WebView
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
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

    /** Event types FocusTrack needs (see UsageEvents.Event). */
    private val wanted = setOf(
        1,  // ACTIVITY_RESUMED (MOVE_TO_FOREGROUND)
        2,  // ACTIVITY_PAUSED (MOVE_TO_BACKGROUND)
        15, // SCREEN_INTERACTIVE
        16, // SCREEN_NON_INTERACTIVE
        17, // KEYGUARD_SHOWN
        18, // KEYGUARD_HIDDEN
    )

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

    private fun granted(): Boolean {
        val ops = activity.getSystemService(Context.APP_OPS_SERVICE) as AppOpsManager
        val mode = if (Build.VERSION.SDK_INT >= 29) {
            ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), activity.packageName)
        } else {
            @Suppress("DEPRECATION")
            ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), activity.packageName)
        }
        return mode == AppOpsManager.MODE_ALLOWED
    }

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

    private fun label(pm: PackageManager, pkg: String): String = try {
        val info = if (Build.VERSION.SDK_INT >= 33) {
            pm.getApplicationInfo(pkg, PackageManager.ApplicationInfoFlags.of(0))
        } else {
            @Suppress("DEPRECATION")
            pm.getApplicationInfo(pkg, 0)
        }
        pm.getApplicationLabel(info).toString()
    } catch (e: Exception) {
        pkg
    }

    @Command
    fun events(invoke: Invoke) {
        if (!granted()) {
            invoke.reject("Usage access isn't granted")
            return
        }
        val args = invoke.parseArgs(EventsArgs::class.java)
        val usm = activity.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
        val pm = activity.packageManager
        val list = JSArray()
        val labels = JSObject()
        val seen = HashSet<String>()
        val iter = usm.queryEvents(args.since, args.until)
        val e = UsageEvents.Event()
        while (iter.hasNextEvent()) {
            iter.getNextEvent(e)
            if (e.eventType !in wanted) continue
            val item = JSObject()
            item.put("package", e.packageName ?: "")
            item.put("kind", e.eventType)
            item.put("time", e.timeStamp)
            list.put(item)
            val pkg = e.packageName
            if (pkg != null && seen.add(pkg)) labels.put(pkg, label(pm, pkg))
        }
        val ret = JSObject()
        ret.put("events", list)
        ret.put("labels", labels)
        invoke.resolve(ret)
    }
}
