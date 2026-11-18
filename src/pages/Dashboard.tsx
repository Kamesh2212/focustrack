import { useEffect, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from "react";
import { Pause, Lock, Moon, Activity, Brain, Coffee, GripVertical, LayoutGrid } from "lucide-react";
import { api } from "../lib/api";
import { useLoad, useNow } from "../lib/hooks";
import { fmtDuration, pctChange } from "../lib/format";
import type { LiveStatus } from "../lib/types";
import { Button, Card, ErrorNote, PageHeader, Stat, cx } from "../components/ui";
import { CategoryDonut, DailyChart, HourlyChart } from "../components/charts";
import { AppList } from "../components/AppList";
import { SiteList } from "../components/SiteList";
import { GoalManager } from "../components/Goals";
import { QuickNote } from "../components/NotesPanel";
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
    paused: {
      icon: Pause,
      tone: "bg-slate-400",
      text: live.appName
        ? `Tracking paused · still counting ${live.domain ?? live.appName} (it has a limit)`
        : "Tracking paused · apps and sites with limits still count",
    },
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

type WidgetId = "overview" | "goals" | "focus" | "week" | "categories" | "apps" | "websites" | "productivity" | "hourly" | "limits" | "notes" | "insights";

const WIDGETS: { id: WidgetId; title: string; wide?: boolean }[] = [
  { id: "overview", title: "Screen time overview", wide: true },
  { id: "goals", title: "Today's goals" },
  { id: "focus", title: "Focus session" },
  { id: "week", title: "Last 7 days" },
  { id: "apps", title: "Application usage" },
  { id: "websites", title: "Website usage" },
  { id: "productivity", title: "Daily productivity summary" },
  { id: "limits", title: "Usage limits" },
  { id: "notes", title: "Quick note" },
  { id: "insights", title: "Insights" },
  { id: "categories", title: "Categories" },
  { id: "hourly", title: "Activity by hour" },
];

type Layout = { id: WidgetId; visible: boolean }[];

function parseLayout(raw: string): Layout {
  const known = new Set(WIDGETS.map((w) => w.id));
  let saved: Layout = [];
  try {
    const v = JSON.parse(raw || "[]");
    if (Array.isArray(v)) saved = v.filter((x) => x && known.has(x.id)).map((x) => ({ id: x.id, visible: !!x.visible }));
  } catch {
    saved = [];
  }
  for (const w of WIDGETS) if (!saved.some((x) => x.id === w.id)) saved.push({ id: w.id, visible: true });
  return saved;
}

const go = (page: string) => window.dispatchEvent(new CustomEvent("ft-navigate", { detail: page }));

/** Moves the item with `id` to position `to`. */
function moveTo(list: Layout, id: string, to: number): Layout {
  const from = list.findIndex((x) => x.id === id);
  if (from < 0 || to < 0 || to >= list.length || from === to) return list;
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item);
  return next;
}

function Customizer({ layout, onChange }: { layout: Layout; onChange: (l: Layout) => void }) {
  // While dragging, `draft` holds the live order; it is saved on release.
  const [draft, setDraft] = useState<Layout | null>(null);
  const [dragId, setDragId] = useState<string | null>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const list = draft ?? layout;

  const startDrag = (e: ReactPointerEvent<HTMLLIElement>, id: string) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("input")) return;
    const sx = e.clientX;
    const sy = e.clientY;
    let current = layout;
    let dragging = false;
    const onMove = (ev: PointerEvent) => {
      if (!dragging) {
        if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
        dragging = true;
        setDragId(id);
      }
      ev.preventDefault();
      const over = document.elementFromPoint(ev.clientX, ev.clientY)?.closest<HTMLElement>("[data-widget]");
      const overId = over?.dataset.widget;
      if (!overId || overId === id || !listRef.current?.contains(over)) return;
      current = moveTo(current, id, current.findIndex((x) => x.id === overId));
      setDraft(current);
    };
    const finish = (save: boolean) => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onCancel);
      if (dragging && save) {
        onChange(current);
        // Releasing over a label would otherwise toggle its checkbox.
        const swallow = (ev: MouseEvent) => {
          ev.stopPropagation();
          ev.preventDefault();
        };
        window.addEventListener("click", swallow, { capture: true, once: true });
        setTimeout(() => window.removeEventListener("click", swallow, true), 0);
      }
      setDraft(null);
      setDragId(null);
    };
    const onUp = () => finish(true);
    const onCancel = () => finish(false);
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
    window.addEventListener("pointercancel", onCancel);
  };

  const onKey = (e: KeyboardEvent, i: number, id: string) => {
    const d = e.key === "ArrowUp" || e.key === "ArrowLeft" ? -1 : e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : 0;
    if (!d) return;
    e.preventDefault();
    onChange(moveTo(layout, id, i + d));
  };

  return (
    <Card title="Customize your dashboard" className="mb-4 border-indigo-200 dark:border-indigo-900">
      <p className="mb-3 text-sm text-slate-500">Choose which sections to show, and drag them into the order you want.</p>
      <ul ref={listRef} className={cx("grid gap-1.5 sm:grid-cols-2 xl:grid-cols-3", dragId && "cursor-grabbing select-none")}>
        {list.map((item, i) => {
          const title = WIDGETS.find((w) => w.id === item.id)?.title;
          return (
            <li
              key={item.id}
              data-widget={item.id}
              onPointerDown={(e) => startDrag(e, item.id)}
              className={cx(
                "flex touch-none items-center gap-2 rounded-lg border px-2 py-1.5 text-sm transition-colors",
                dragId ? "" : "cursor-grab",
                dragId === item.id
                  ? "border-indigo-400 bg-indigo-50 shadow-md ring-2 ring-indigo-300 dark:border-indigo-500 dark:bg-indigo-950/60 dark:ring-indigo-700"
                  : "border-slate-200 dark:border-slate-700",
              )}
            >
              <button
                type="button"
                onKeyDown={(e) => onKey(e, i, item.id)}
                className="rounded p-0.5 text-slate-400 hover:bg-slate-100 hover:text-slate-600 dark:hover:bg-slate-800"
                aria-label={`Reorder ${title} (drag, or use the arrow keys)`}
                title="Drag to reorder"
              >
                <GripVertical size={14} />
              </button>
              <input
                type="checkbox"
                id={`dw-${item.id}`}
                checked={item.visible}
                onChange={(e) => onChange(layout.map((x) => (x.id === item.id ? { ...x, visible: e.target.checked } : x)))}
              />
              <label htmlFor={`dw-${item.id}`} className={cx("flex-1 truncate", dragId ? "cursor-grabbing" : "cursor-grab")}>
                {title}
              </label>
            </li>
          );
        })}
      </ul>
    </Card>
  );
}

export default function Dashboard() {
  const { data: d, error } = useLoad(() => api.dashboard(), [], 5000);
  const focus = useLoad(() => api.focus(), [], 5000);
  const insights = useLoad(() => api.insights(), [], 300000);
  const limits = useLoad(() => api.limits(), [], 30000);
  const settings = useLoad(() => api.settings(), []);
  const [layout, setLayout] = useState<Layout | null>(null);
  const [customizing, setCustomizing] = useState(false);

  useEffect(() => {
    if (settings.data && !layout) setLayout(parseLayout(settings.data.dashboardLayout));
  }, [settings.data, layout]);

  if (!d) return <ErrorNote error={error} />;

  const change = pctChange(d.activeSecs, d.prevDayActiveSecs);
  const productivePct = d.activeSecs ? Math.round((d.productiveSecs / d.activeSecs) * 100) : 0;
  const mostUsed = d.topApps[0];
  const current = layout ?? parseLayout("");

  const updateLayout = (l: Layout) => {
    setLayout(l);
    api.setDashboardLayout(JSON.stringify(l)).catch(() => {});
  };

  const render = (id: WidgetId) => {
    switch (id) {
      case "overview":
        return (
          <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
            <Stat
              label="Screen time"
              value={fmtDuration(d.screenSecs)}
              hint={change === null ? "No data for yesterday" : `${change > 0 ? "▲" : change < 0 ? "▼" : ""} ${Math.abs(change)}% active vs yesterday`}
              tone={change === null ? "default" : change > 0 ? "bad" : "good"}
            />
            <Stat label="Active time" value={fmtDuration(d.activeSecs)} hint={`Daily average ${fmtDuration(d.dailyAvgSecs)}`} />
            <Stat label="Idle time" value={fmtDuration(d.idleSecs)} hint={d.screenSecs ? `${Math.round((d.idleSecs / d.screenSecs) * 100)}% of screen time` : "—"} />
            <Stat label="Sessions" value={d.sessionCount} hint={mostUsed ? `Most used: ${mostUsed.name}` : "No apps yet"} />
          </div>
        );
      case "goals":
        return (
          <Card title="Today's goals">
            <GoalManager compact />
          </Card>
        );
      case "focus": {
        const f = focus.data?.active;
        return (
          <Card title="Focus session">
            {f ? (
              <div className="text-sm">
                <div className="font-medium">
                  {f.phase === "focus" ? (f.mode === "study" ? "Studying" : "Focusing") : "On a break"}: {f.subject || f.label}
                </div>
                {f.goalText && <div className="text-slate-600 dark:text-slate-300">Goal: {f.goalText}</div>}
                <div className="mt-1 text-slate-500">
                  {f.blocking ? `${f.profileName} is blocking distractions` : "Nothing blocked"} · ends {f.planEndsAt.slice(11, 16)}
                </div>
                <div className="mt-3 flex gap-2">
                  <Button onClick={() => go(f.mode === "study" ? "study" : "focus")}>Open</Button>
                </div>
              </div>
            ) : (
              <div className="text-sm">
                <p className="text-slate-500">
                  {fmtDuration(focus.data?.todaySecs ?? 0)} focused today · {focus.data?.todaySessions ?? 0} rounds completed
                  {(focus.data?.streakDays ?? 0) > 1 ? ` · ${focus.data?.streakDays}-day streak` : ""}
                </p>
                <div className="mt-3 flex gap-2">
                  <Button variant="primary" onClick={() => go("focus")}>
                    Start focusing
                  </Button>
                  <Button onClick={() => go("study")}>Study</Button>
                </div>
              </div>
            )}
          </Card>
        );
      }
      case "week":
        return (
          <Card title="Last 7 days" action={<span className="text-xs text-slate-500">Avg {fmtDuration(d.weeklyAvgSecs)} / day</span>}>
            <DailyChart days={d.week} />
          </Card>
        );
      case "categories":
        return (
          <Card title="Categories">
            <CategoryDonut categories={d.categories} />
          </Card>
        );
      case "apps":
        return (
          <Card title="Most-used applications">
            <AppList apps={d.topApps} limit={6} />
          </Card>
        );
      case "websites":
        return (
          <Card title="Most-used websites" action={d.siteSecs > 0 && <span className="tabular text-xs text-slate-500">{fmtDuration(d.siteSecs)} on the web</span>}>
            <SiteList sites={d.topSites} limit={6} empty="No websites yet. Install the browser extension (see Websites) to track them." />
          </Card>
        );
      case "productivity":
        return (
          <Card title="Productivity" action={<span className="tabular text-xs text-slate-500">{productivePct}% productive</span>}>
            <div className="flex h-3 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
              <div className="bg-emerald-500" style={{ width: `${productivePct}%` }} />
              <div className="bg-rose-500" style={{ width: `${d.activeSecs ? (d.distractingSecs / d.activeSecs) * 100 : 0}%` }} />
            </div>
            <div className="mt-3 flex flex-wrap gap-x-6 gap-y-1 text-sm">
              <span>
                <span className="mr-1.5 inline-block h-2 w-2 rounded-full bg-emerald-500" />
                Productive {fmtDuration(d.productiveSecs)}
              </span>
              <span>
                <span className="mr-1.5 inline-block h-2 w-2 rounded-full bg-rose-500" />
                Distracting {fmtDuration(d.distractingSecs)}
              </span>
              <span className="text-slate-500">Neutral {fmtDuration(Math.max(0, d.activeSecs - d.productiveSecs - d.distractingSecs))}</span>
            </div>
          </Card>
        );
      case "hourly":
        return (
          <Card title="Activity by hour">
            <HourlyChart hourly={d.hourly} height={150} />
          </Card>
        );
      case "limits": {
        const ls = (limits.data ?? []).filter((l) => l.enabled);
        return (
          <Card title="Usage limits" action={<Button onClick={() => go("limits")}>Manage</Button>}>
            {ls.length === 0 ? (
              <p className="text-sm text-slate-500">No limits set.</p>
            ) : (
              <ul className="space-y-3">
                {ls.map((l) => {
                  const allowed = l.dailyLimitSecs + l.extraSecs;
                  const pct = Math.min(100, (l.usedSecs / allowed) * 100);
                  const over = l.usedSecs >= allowed;
                  return (
                    <li key={l.id}>
                      <div className="flex justify-between text-sm">
                        <span className="truncate">{l.name}</span>
                        <span className={cx("tabular text-xs", over ? "font-medium text-rose-600 dark:text-rose-400" : "text-slate-500")}>
                          {over ? (l.action === "notify" ? "reached" : "blocked") : `${fmtDuration(allowed - l.usedSecs)} left`}
                        </span>
                      </div>
                      <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
                        <div
                          className={cx("h-full rounded-full", over ? "bg-rose-500" : pct >= l.warnPct ? "bg-amber-500" : "bg-indigo-500")}
                          style={{ width: `${Math.max(2, pct)}%` }}
                        />
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </Card>
        );
      }
      case "notes":
        return (
          <Card title="Quick note" action={<Button onClick={() => go("notes")}>All notes</Button>}>
            <QuickNote />
          </Card>
        );
      case "insights":
        return (
          <Card title="Insights">
            <InsightList insights={insights.data ?? []} />
          </Card>
        );
    }
  };

  return (
    <>
      <PageHeader
        title="Today"
        subtitle={new Date().toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" })}
        actions={
          <Button onClick={() => setCustomizing(!customizing)}>
            <LayoutGrid size={15} /> {customizing ? "Done" : "Customize"}
          </Button>
        }
      />
      <ErrorNote error={error} />
      {customizing && <Customizer layout={current} onChange={updateLayout} />}
      {focus.data?.active && <FocusBanner f={focus.data.active} />}
      <LiveBanner live={d.live} />

      <div className="grid gap-4 lg:grid-cols-2">
        {current
          .filter((x) => x.visible)
          .map((x) => (
            <div key={x.id} className={cx(WIDGETS.find((w) => w.id === x.id)?.wide && "lg:col-span-2")}>
              {render(x.id)}
            </div>
          ))}
      </div>
    </>
  );
}
