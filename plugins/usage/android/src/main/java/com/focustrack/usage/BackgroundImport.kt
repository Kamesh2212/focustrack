package com.focustrack.usage

import android.content.Context
import android.util.Log
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/**
 * Imports screen time (and syncs) about every 15 minutes, also while the app
 * is closed. Android reads the events; FocusTrack's Rust code (the same code
 * the app uses) records and syncs them.
 */
class BackgroundImport(ctx: Context, params: WorkerParameters) : Worker(ctx, params) {

    companion object {
        private const val TAG = "FocusTrack"
        private const val PREFS = "focustrack_background"
        private const val WORK = "focustrack-usage-import"
        private const val WEEK_MS = 7L * 24 * 60 * 60 * 1000

        /** Implemented in Rust: `phone.rs`, `Java_com_focustrack_usage_BackgroundImport_run`. */
        @JvmStatic
        external fun run(dataDir: String, input: String): String

        /** Remembers where the app's data is and schedules the job (kept if already scheduled). */
        fun schedule(ctx: Context, dataDir: String) {
            ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE).edit().putString("dataDir", dataDir).apply()
            val request = PeriodicWorkRequestBuilder<BackgroundImport>(15, TimeUnit.MINUTES).build()
            WorkManager.getInstance(ctx).enqueueUniquePeriodicWork(WORK, ExistingPeriodicWorkPolicy.KEEP, request)
        }

        /**
         * Imports, syncs and rewrites the blocker's plan now. Also used by the
         * blocker while the phone is in use. False if it failed (try again later).
         */
        @Synchronized
        fun runNow(ctx: Context): Boolean {
            val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            val dir = prefs.getString("dataDir", null) ?: return true // app never opened
            if (!UsageReader.granted(ctx)) return true
            val now = System.currentTimeMillis()
            val since = prefs.getLong("since", now - WEEK_MS)
            return try {
                System.loadLibrary("focustrack_lib")
                val out = JSONObject(run(dir, UsageReader.read(ctx, since, now).toString()))
                if (out.optLong("nextSince", 0) > 0) prefs.edit().putLong("since", out.getLong("nextSince")).apply()
                Log.i(TAG, "background import: $out")
                true
            } catch (e: Throwable) {
                Log.w(TAG, "background import failed", e)
                false
            }
        }
    }

    override fun doWork(): Result = if (runNow(applicationContext)) Result.success() else Result.retry()
}
