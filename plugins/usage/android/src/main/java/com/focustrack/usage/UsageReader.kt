package com.focustrack.usage

import android.app.AppOpsManager
import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Process
import org.json.JSONArray
import org.json.JSONObject

/** Reads screen-time events; shared by the app plugin and the background job. */
object UsageReader {
    /** Event types FocusTrack needs (see UsageEvents.Event). */
    private val wanted = setOf(
        1,  // ACTIVITY_RESUMED (MOVE_TO_FOREGROUND)
        2,  // ACTIVITY_PAUSED (MOVE_TO_BACKGROUND)
        15, // SCREEN_INTERACTIVE
        16, // SCREEN_NON_INTERACTIVE
        17, // KEYGUARD_SHOWN
        18, // KEYGUARD_HIDDEN
    )

    fun granted(ctx: Context): Boolean {
        val ops = ctx.getSystemService(Context.APP_OPS_SERVICE) as AppOpsManager
        val mode = if (Build.VERSION.SDK_INT >= 29) {
            ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ctx.packageName)
        } else {
            @Suppress("DEPRECATION")
            ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), ctx.packageName)
        }
        return mode == AppOpsManager.MODE_ALLOWED
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

    /**
     * The home screen (default launcher): time on it isn't app use. Only the
     * default one: the Settings app also registers a fallback home screen.
     */
    private fun homeApps(pm: PackageManager): JSONArray {
        val out = JSONArray()
        val home = Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME)
        try {
            @Suppress("DEPRECATION")
            val default = pm.resolveActivity(home, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName
            // "android" means no default is set (the chooser); then take every launcher but Settings.
            if (default != null && default != "android") {
                out.put(default)
            } else {
                @Suppress("DEPRECATION")
                pm.queryIntentActivities(home, 0)
                    .mapNotNull { it.activityInfo?.packageName }
                    .filter { it != "com.android.settings" }
                    .distinct()
                    .forEach { out.put(it) }
            }
        } catch (e: Exception) {
        }
        return out
    }

    /** `{ events: [{package, kind, time}], labels: {package: name}, home: [package] }` between two times (ms). */
    fun read(ctx: Context, since: Long, until: Long): JSONObject {
        val usm = ctx.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
        val pm = ctx.packageManager
        val list = JSONArray()
        val labels = JSONObject()
        val seen = HashSet<String>()
        val iter = usm.queryEvents(since, until)
        val e = UsageEvents.Event()
        while (iter.hasNextEvent()) {
            iter.getNextEvent(e)
            if (e.eventType !in wanted) continue
            val item = JSONObject()
            item.put("package", e.packageName ?: "")
            item.put("kind", e.eventType)
            item.put("time", e.timeStamp)
            list.put(item)
            val pkg = e.packageName
            if (pkg != null && seen.add(pkg)) labels.put(pkg, label(pm, pkg))
        }
        val ret = JSONObject()
        ret.put("events", list)
        ret.put("labels", labels)
        ret.put("home", homeApps(pm))
        return ret
    }
}
