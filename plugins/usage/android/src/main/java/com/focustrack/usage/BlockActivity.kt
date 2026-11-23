package com.focustrack.usage

import android.app.Activity
import android.content.Intent
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView
import android.widget.Toast
import android.window.OnBackInvokedDispatcher

/** FocusTrack's screen over a blocked app (#69). */
class BlockActivity : Activity() {
    private lateinit var heading: TextView
    private lateinit var message: TextView
    private lateinit var extend: Button

    private fun dp(v: Int): Int =
        TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v.toFloat(), resources.displayMetrics).toInt()

    private fun button(label: String, primary: Boolean): Button = Button(this).apply {
        text = label
        isAllCaps = false
        setTextColor(if (primary) Color.WHITE else Color.rgb(199, 210, 254))
        background = GradientDrawable().apply {
            cornerRadius = dp(10).toFloat()
            if (primary) setColor(Color.rgb(79, 70, 229)) else setStroke(dp(1), Color.rgb(99, 102, 241))
        }
        layoutParams = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, dp(48)).apply { topMargin = dp(12) }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        heading = TextView(this).apply {
            setTextColor(Color.WHITE)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 24f)
            gravity = Gravity.CENTER
        }
        message = TextView(this).apply {
            setTextColor(Color.rgb(203, 213, 225))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
            gravity = Gravity.CENTER
            setPadding(0, dp(12), 0, dp(20))
        }
        val home = button("Close the app", true).apply { setOnClickListener { goHome() } }
        extend = button("+15 minutes", false).apply { setOnClickListener { addTime() } }
        val brand = TextView(this).apply {
            text = "FocusTrack"
            setTextColor(Color.rgb(129, 140, 248))
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            gravity = Gravity.CENTER
            setPadding(0, 0, 0, dp(24))
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            setBackgroundColor(Color.rgb(15, 23, 42))
            setPadding(dp(32), dp(32), dp(32), dp(32))
            addView(brand)
            addView(heading)
            addView(message)
            addView(home)
            addView(extend)
        })
        if (Build.VERSION.SDK_INT >= 33) {
            onBackInvokedDispatcher.registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT) { goHome() }
        }
        show(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        show(intent)
    }

    private fun show(i: Intent) {
        heading.text = i.getStringExtra("title") ?: "Blocked"
        message.text = i.getStringExtra("message") ?: ""
        val canExtend = i.getBooleanExtra("canExtend", false) && i.getLongExtra("limitId", -1) >= 0
        extend.visibility = if (canExtend) View.VISIBLE else View.GONE
    }

    @Deprecated("Android 12 and older")
    override fun onBackPressed() {
        goHome()
    }

    private fun goHome() {
        startActivity(Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_HOME).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        finish()
    }

    private fun addTime() {
        val id = intent.getLongExtra("limitId", -1)
        val dir = Blocker.dataDir(this)
        if (id < 0 || dir == null) return
        extend.isEnabled = false
        Thread {
            val error = try {
                System.loadLibrary("focustrack_lib")
                Blocker.extend(dir, id)
            } catch (e: Throwable) {
                "FocusTrack couldn't add the time"
            }
            runOnUiThread {
                if (error.isEmpty()) {
                    Toast.makeText(this, "15 more minutes", Toast.LENGTH_SHORT).show()
                    finish() // back to the app
                } else {
                    extend.isEnabled = true
                    Toast.makeText(this, error, Toast.LENGTH_LONG).show()
                }
            }
        }.start()
    }
}
