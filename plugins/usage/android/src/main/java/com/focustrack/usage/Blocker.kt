package com.focustrack.usage

import android.app.usage.UsageEvents
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.provider.Settings
import android.telecom.TelecomManager
import org.json.JSONObject
import java.io.File
import java.text.DateFormat
import java.util.Calendar
import java.util.Date

/**
 * Decides what the phone blocks (#69). FocusTrack's Rust code writes the plan
 * (`blocks.json` next to the database): the running focus session, and each
 * limit with the usage counted so far. This adds the time Android recorded
 * since, so limits apply on time even while FocusTrack is closed.
 */
object Blocker {
    /** Implemented in Rust (`phone.rs`): adds 15 minutes to a limit today. Returns "" or the reason it couldn't. */
    @JvmStatic
    external fun extend(dataDir: String, limitId: Long): String

    class FocusRule(val label: String, val until: Long, val allowOnly: Boolean, val packages: Set<String>)

    class LimitRule(
        val id: Long,
        val name: String,
        val packages: Set<String>,
        val allowedSecs: Long,
        val usedSecs: Long,
        val warnFirst: Boolean,
    )

    class Plan(
        val countedUntil: Long,
        val dayStart: Long,
        val focus: FocusRule?,
        val limits: List<LimitRule>,
        val extensionsLeft: Long,
    )

    sealed class Decision {
        object Allow : Decision()
        /** Check again after this long (a limit or session is running out). */
        class Later(val ms: Long) : Decision()
        class Block(val title: String, val message: String, val limitId: Long, val canExtend: Boolean) : Decision()
        class Warn(val message: String, val limitId: Long, val inMs: Long) : Decision()
    }

    private var cached: Plan? = null
    private var cachedStamp = 0L

    fun dataDir(ctx: Context): String? =
        ctx.getSharedPreferences("focustrack_background", Context.MODE_PRIVATE).getString("dataDir", null)

    /** The current plan; re-read when FocusTrack rewrites it. */
    fun plan(ctx: Context): Plan? {
        val dir = dataDir(ctx) ?: return null
        val file = File(dir, "blocks.json")
        val stamp = file.lastModified()
        if (stamp == 0L) return null
        if (stamp == cachedStamp) return cached
        cached = try {
            parse(JSONObject(file.readText()))
        } catch (e: Exception) {
            null
        }
        cachedStamp = stamp
        return cached
    }

    private fun strings(o: JSONObject, key: String): Set<String> {
        val a = o.optJSONArray(key) ?: return emptySet()
        return (0 until a.length()).map { a.getString(it).lowercase() }.toSet()
    }

    fun parse(o: JSONObject): Plan {
        val f = o.optJSONObject("focus")
        val focus = f?.let {
            FocusRule(it.optString("label"), it.optLong("until"), it.optString("mode") == "allow_only", strings(it, "packages"))
        }
        val arr = o.optJSONArray("limits")
        val limits = (0 until (arr?.length() ?: 0)).map {
            val l = arr!!.getJSONObject(it)
            LimitRule(
                l.optLong("id"),
                l.optString("name"),
                strings(l, "packages"),
                l.optLong("allowedSecs"),
                l.optLong("usedSecs"),
                l.optString("action") == "warn_then_block",
            )
        }
        return Plan(o.optLong("countedUntil"), o.optLong("dayStart"), focus, limits, o.optLong("extensionsLeft"))
    }

    /**
     * Apps that are never blocked: FocusTrack, the phone and emergency calls,
     * Settings (to undo blocking), the home screen, the keyboard and system screens.
     */
    fun essentials(ctx: Context): Set<String> {
        val s = hashSetOf(
            ctx.packageName, "android", "com.android.systemui", "com.android.settings",
            "com.android.phone", "com.android.server.telecom", "com.android.emergency",
            "com.google.android.dialer", "com.android.dialer", "com.samsung.android.dialer",
            "com.android.permissioncontroller", "com.google.android.permissioncontroller",
            "com.android.packageinstaller", "com.google.android.packageinstaller",
        )
        try {
            (ctx.getSystemService(Context.TELECOM_SERVICE) as? TelecomManager)?.defaultDialerPackage?.let { s.add(it) }
        } catch (e: Exception) {
        }
        keyboard(ctx)?.let { s.add(it) }
        val home = UsageReader.homeApps(ctx.packageManager)
        for (i in 0 until home.length()) s.add(home.getString(i))
        return s
    }

    /** The current keyboard: its windows come and go over other apps. */
    fun keyboard(ctx: Context): String? =
        Settings.Secure.getString(ctx.contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)?.substringBefore('/')

    /** Milliseconds any of `packages` was in front between `from` and `now`. */
    fun foregroundMs(ctx: Context, from: Long, now: Long, packages: Set<String>): Long {
        if (from >= now) return 0
        val usm = ctx.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager
        val events = usm.queryEvents(from, now)
        val e = UsageEvents.Event()
        var current: String? = null
        var since = from
        var sawResume = false
        var total = 0L
        while (events.hasNextEvent()) {
            events.getNextEvent(e)
            val pkg = e.packageName?.lowercase()
            when (e.eventType) {
                1 -> { // ACTIVITY_RESUMED
                    if (current != null && current in packages) total += e.timeStamp - since
                    current = pkg
                    since = e.timeStamp
                    sawResume = true
                }
                2 -> { // ACTIVITY_PAUSED
                    if (current == null && !sawResume && pkg in packages) {
                        total += e.timeStamp - from // in front since before `from`
                    } else if (current == pkg) {
                        if (pkg in packages) total += e.timeStamp - since
                        current = null
                    }
                    sawResume = true
                }
                16 -> { // SCREEN_NON_INTERACTIVE
                    if (current != null && current in packages) total += e.timeStamp - since
                    current = null
                }
            }
        }
        if (current != null && current in packages) total += now - since
        return total
    }

    private fun clock(ms: Long): String = DateFormat.getTimeInstance(DateFormat.SHORT).format(Date(ms))

    private fun duration(secs: Long): String {
        val h = secs / 3600
        val m = (secs % 3600) / 60
        return when {
            h > 0 && m > 0 -> "${h} h ${m} min"
            h > 0 -> "${h} h"
            else -> "${m} min"
        }
    }

    private fun startOfToday(now: Long): Long {
        val c = Calendar.getInstance()
        c.timeInMillis = now
        c.set(Calendar.HOUR_OF_DAY, 0)
        c.set(Calendar.MINUTE, 0)
        c.set(Calendar.SECOND, 0)
        c.set(Calendar.MILLISECOND, 0)
        return c.timeInMillis
    }

    fun decide(ctx: Context, pkg: String, label: String, now: Long, essentials: Set<String>): Decision {
        val app = pkg.lowercase()
        if (app in essentials) return Decision.Allow
        val plan = plan(ctx) ?: return Decision.Allow
        var next = Long.MAX_VALUE
        val f = plan.focus
        if (f != null && now < f.until) {
            val listed = app in f.packages
            if (if (f.allowOnly) !listed else listed) {
                return Decision.Block(
                    "Focus session",
                    "\"${f.label}\" is on until ${clock(f.until)}. $label opens again when it ends.",
                    -1,
                    false,
                )
            }
            next = minOf(next, f.until - now)
        }
        // After midnight, until FocusTrack writes a new plan: only today's time counts.
        val today = startOfToday(now)
        val fresh = plan.dayStart >= today
        val from = if (fresh) maxOf(plan.countedUntil, plan.dayStart) else today
        for (l in plan.limits) {
            if (app !in l.packages) continue
            val used = (if (fresh) l.usedSecs * 1000 else 0) + foregroundMs(ctx, from, now, l.packages)
            val left = l.allowedSecs * 1000 - used
            if (left <= 0) {
                return Decision.Block(
                    "Daily limit reached",
                    "You've used ${l.name} for ${duration(l.allowedSecs)} today, your limit. It opens again tomorrow.",
                    l.id,
                    plan.extensionsLeft > 0,
                )
            }
            if (l.warnFirst && left <= 60_000L) {
                return Decision.Warn("1 minute left on ${l.name} today.", l.id, left)
            }
            next = minOf(next, if (l.warnFirst) left - 60_000L else left)
        }
        // Re-check at least every minute: the plan and the usage keep changing.
        return if (next == Long.MAX_VALUE) Decision.Allow else Decision.Later(minOf(next, 60_000L) + 500L)
    }

    fun showBlock(ctx: Context, d: Decision.Block, pkg: String) {
        val i = Intent(ctx, BlockActivity::class.java)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            .putExtra("title", d.title)
            .putExtra("message", d.message)
            .putExtra("limitId", d.limitId)
            .putExtra("canExtend", d.canExtend)
            .putExtra("package", pkg)
        ctx.startActivity(i)
    }

    fun enabled(ctx: Context): Boolean {
        val list = Settings.Secure.getString(ctx.contentResolver, Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES) ?: return false
        val me = "${ctx.packageName}/${BlockerService::class.java.name}"
        val short = "${ctx.packageName}/.${BlockerService::class.java.simpleName}"
        return list.split(':').any { it.equals(me, true) || it.equals(short, true) }
    }
}
