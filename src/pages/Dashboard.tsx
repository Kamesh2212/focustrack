import { Pause, Lock, Moon, Activity, Brain, Coffee } from "lucide-react";
import { api } from "../lib/api";
import { useLoad, useNow } from "../lib/hooks";
import { fmtDuration, pctChange } from "../lib/format";
import type { LiveStatus } from "../lib/types";
import { Card, ErrorNote, PageHeader, Stat, cx } from "../components/ui";
import { CategoryDonut, DailyChart, HourlyChart } from "../components/charts";
import { AppList } from "../components/AppList";
import { SiteList } from "../components/SiteList";
import { GoalProgress } from "../components/Goals";
import { InsightList } from "../components/Insights";
import type { FocusStatus } from "../lib/types";

function parseTs(ts: string): number {
  return new Date(ts.replace(" ", "T")).getTime();
}

function LiveBanner({ live }: { live: LiveStatus }) {
  const now = useNow();
  const elapsed = live.since ? Math.max(0, (now - parseTs(live.since)) / 1000) : 0;
  const view = {
    active: {
      icon: Activity,
      tone: "bg-emerald-500",
      text: live.appName ? `Using ${live.appName}${live.domain ? ` · ${live.domain}` : ""}` : "Active",
    },
    idle: { icon: Moon, tone: "bg-amber-500", text: "Idle — no keyboard or mouse input" },
    locked: { icon: Lock, tone: "bg-slate-400", text: "Computer locked" },
    paused: { icon: Pause, tone: "bg-slate-400", text: "Tracking paused" },
    starting: { icon: Activity, tone: "bg-slate-400", text: "Starting tracker…" },
  }[live.state];
  const Icon = view.icon;
  return (
    <div className="mb-6 flex items-center gap-3 rounded-xl border border-slate-200 bg-white px-4 py-3 shadow-sm dark:border-slate-800 dark:bg-slate-900">
      <span className="relative flex h-2.5 w-2.5">
        {live.state === "active" && <span className={cx("absolute inline-flex h-full w-full animate-ping rounded-full opacity-60", view.tone)} />}
        <span className={cx("relative inline-flex h-2.5 w-2.5 rounded-full", view.tone)} />
      </span>
      <Icon size={16} className="text-slate-500" />
      <span className="text-sm font-medium">{view.text}</span>
      {live.since && (
        <span className="tabular ml-auto text-sm text-slate-500 dark:text-slate-400">
          {live.state === "idle" ? "idle for" : "current session"} {fmtDuration(elapsed)}
        </span>
      )}
    </div>
  );
}

function FocusBanner({ f }: { f: FocusStatus }) {
  const now = useNow();
  const left = Math.max(0, (parseTs(f.endsAt) - now) / 1000);
  const m = Math.floor(left / 60);
  const s = String(Math.floor(left % 60)).padStart(2, "0");
  const Icon = f.phase === "focus" ? Brain : Coffee;
  return (
    <div className="mb-3 flex items-center gap-3 rounded-xl border border-indigo-200 bg-indigo-50 px-4 py-2.5 text-sm text-indigo-800 dark:border-indigo-900 dark:bg-indigo-950/50 dark:text-indigo-200">
      <Icon size={16} />
      <span className="font-medium">{f.phase === "focus" ? `Focusing: ${f.label}` : "On a break"}</span>
      {f.rounds > 1 && <span className="text-indigo-600/80 dark:text-indigo-300/80">round {f.round} of {f.rounds}</span>}
      <span className="tabular ml-auto font-medium">
        {m}:{s} left
      </span>
    </div>
  );
}

export default function Dashboard() {
  const { data: d, error } = useLoad(() => api.dashboard(), [], 5000);
  const focus = useLoad(() => api.focus(), [], 5000);
  const goals = useLoad(() => api.goals(), [], 30000);
  const insights = useLoad(() => api.insights(), [], 300000);

  if (!d) return <ErrorNote error={error} />;

  const change = pctChange(d.activeSecs, d.prevDayActiveSecs);
  const productivePct = d.activeSecs ? Math.round((d.productiveSecs / d.activeSecs) * 100) : 0;
  const mostUsed = d.topApps[0];

  return (
    <>
      <PageHeader
        title="Today"
        subtitle={new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
      />
      <ErrorNote error={error} />
      {focus.data?.active && <FocusBanner f={focus.data.active} />}
      <LiveBanner live={d.live} />

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat
          label="Screen time"
          value={fmtDuration(d.screenSecs)}
          hint={
            change === null
              ? "No data for yesterday"
              : `${change > 0 ? "▲" : change < 0 ? "▼" : ""} ${Math.abs(change)}% active vs yesterday`
          }
          tone={change === null ? "default" : change > 0 ? "bad" : "good"}
        />
        <Stat label="Active time" value={fmtDuration(d.activeSecs)} hint={`Daily average ${fmtDuration(d.dailyAvgSecs)}`} />
        <Stat
          label="Idle time"
          value={fmtDuration(d.idleSecs)}
          hint={d.screenSecs ? `${Math.round((d.idleSecs / d.screenSecs) * 100)}% of screen time` : "—"}
        />
        <Stat
          label="Sessions"
          value={d.sessionCount}
          hint={mostUsed ? `Most used: ${mostUsed.name}` : "No apps yet"}
        />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-3">
        <Card
          className="lg:col-span-2"
          title="Last 7 days"
          action={<span className="text-xs text-slate-500">Weekly average {fmtDuration(d.weeklyAvgSecs)} / day</span>}
        >
          <DailyChart days={d.week} />
        </Card>
        <Card title="Categories">
          <CategoryDonut categories={d.categories} />
        </Card>
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Card title="Most-used applications">
          <AppList apps={d.topApps} limit={6} />
        </Card>
        <div className="grid gap-4">
          <Card
            title="Productivity"
            action={<span className="tabular text-xs text-slate-500">{productivePct}% productive</span>}
          >
            <div className="flex h-3 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
              <div className="bg-emerald-500" style={{ width: `${productivePct}%` }} />
              <div
                className="bg-rose-500"
                style={{ width: `${d.activeSecs ? (d.distractingSecs / d.activeSecs) * 100 : 0}%` }}
              />
            </div>
            <div className="mt-3 flex flex-wrap gap-x-6 gap-y-1 text-sm">
              <span><span className="mr-1.5 inline-block h-2 w-2 rounded-full bg-emerald-500" />Productive {fmtDuration(d.productiveSecs)}</span>
              <span><span className="mr-1.5 inline-block h-2 w-2 rounded-full bg-rose-500" />Distracting {fmtDuration(d.distractingSecs)}</span>
              <span className="text-slate-500">Neutral {fmtDuration(Math.max(0, d.activeSecs - d.productiveSecs - d.distractingSecs))}</span>
            </div>
          </Card>
          <Card title="Activity by hour">
            <HourlyChart hourly={d.hourly} height={140} />
          </Card>
          <Card
            title="Most-used websites"
            action={d.siteSecs > 0 && <span className="tabular text-xs text-slate-500">{fmtDuration(d.siteSecs)} on the web</span>}
          >
            <SiteList
              sites={d.topSites}
              limit={5}
              empty="No websites yet. Install the browser extension (see Websites) to track them."
            />
          </Card>
        </div>
      </div>
      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <Card title="Today's goals">
          <GoalProgress goals={goals.data ?? []} />
        </Card>
        <Card title="Insights">
          <InsightList insights={insights.data ?? []} />
        </Card>
      </div>
    </>
  );
}
