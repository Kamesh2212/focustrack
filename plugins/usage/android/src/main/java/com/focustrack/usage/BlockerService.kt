package com.focustrack.usage

import android.accessibilityservice.AccessibilityService
import android.content.pm.PackageManager
import android.os.Handler
import android.os.Looper
import android.view.accessibility.AccessibilityEvent
import android.widget.Toast
import java.util.concurrent.atomic.AtomicBoolean

/**
 * FocusTrack's blocker (#69). Android tells it which app comes to the front;
 * it only reads the app's package name (never window contents), and shows
 * FocusTrack's block screen over apps that are over their limit or blocked
 * by a focus session.
 */
class BlockerService : AccessibilityService() {
    private val handler = Handler(Looper.getMainLooper())
    private var current: String? = null
    private var essentials: Set<String> = emptySet()
    private var essentialsAt = 0L
    private var lastRefresh = 0L
    private val refreshing = AtomicBoolean(false)
    private val warned = HashSet<String>()
    private val recheck = Runnable { check() }

    override fun onServiceConnected() {
        super.onServiceConnected()
        essentials = Blocker.essentials(this)
        essentialsAt = System.currentTimeMillis()
    }

    override fun onAccessibilityEvent(event: AccessibilityEvent?) {
        if (event?.eventType != AccessibilityEvent.TYPE_WINDOW_STATE_CHANGED) return
        val pkg = event.packageName?.toString() ?: return
        // The keyboard and system bars open over the app in front; it's still in front.
        if (pkg == "com.android.systemui" || pkg == Blocker.keyboard(this)) return
        val now = System.currentTimeMillis()
        if (now - essentialsAt > 10 * 60_000) {
            essentials = Blocker.essentials(this) // the default dialer or home screen may change
            essentialsAt = now
        }
        current = pkg
        check()
        refreshSoon(now)
    }

    override fun onInterrupt() {}

    override fun onDestroy() {
        handler.removeCallbacks(recheck)
        super.onDestroy()
    }

    private fun label(pkg: String): String = try {
        val pm = packageManager
        @Suppress("DEPRECATION")
        pm.getApplicationLabel(pm.getApplicationInfo(pkg, PackageManager.GET_META_DATA)).toString()
    } catch (e: Exception) {
        pkg
    }

    private fun check() {
        handler.removeCallbacks(recheck)
        val pkg = current ?: return
        val d = try {
            Blocker.decide(this, pkg, label(pkg), System.currentTimeMillis(), essentials)
        } catch (e: Exception) {
            Blocker.Decision.Allow
        }
        when (d) {
            is Blocker.Decision.Block -> Blocker.showBlock(this, d, pkg)
            is Blocker.Decision.Warn -> {
                val key = "${d.limitId}:${java.text.SimpleDateFormat("yyyy-MM-dd").format(java.util.Date())}"
                if (warned.add(key)) Toast.makeText(this, d.message, Toast.LENGTH_LONG).show()
                handler.postDelayed(recheck, d.inMs + 500)
            }
            is Blocker.Decision.Later -> handler.postDelayed(recheck, d.ms)
            Blocker.Decision.Allow -> {}
        }
    }

    /**
     * Every couple of minutes while the phone is in use, import screen time,
     * sync and rewrite the plan (FocusTrack may be closed), then check again.
     */
    private fun refreshSoon(now: Long) {
        if (now - lastRefresh < 2 * 60_000 || !refreshing.compareAndSet(false, true)) return
        lastRefresh = now
        Thread {
            try {
                BackgroundImport.runNow(applicationContext)
            } finally {
                refreshing.set(false)
                handler.post { check() }
            }
        }.start()
    }
}
