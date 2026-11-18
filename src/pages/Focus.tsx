import { useState } from "react";
import { Brain, Coffee, Flame, Play, Square, Timer } from "lucide-react";
import { api } from "../lib/api";
import { useLoad, useNow } from "../lib/hooks";
import { clock, dayLabel, fmtDuration } from "../lib/format";
import type { FocusPlan, FocusStatus } from "../lib/types";
import { Button, Card, Empty, ErrorNote, PageHeader, Stat, Toggle, cx } from "../components/ui";
import { GoalManager } from "../components/Goals";

const PRESETS: { id: string; title: string; desc: string; plan: Omit<FocusPlan, "blockDistracting"> }[] = [
  { id: "pomodoro", title: "Pomodoro", desc: "4 × 25 min, 5 min breaks, 15 min long break", plan: { label: "Pomodoro", focusMins: 25, breakMins: 5, longBreakMins: 15, rounds: 4 } },
  { id: "deep", title: "Deep work", desc: "50 minutes, one session", plan: { label: "Deep work", focusMins: 50, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "study", title: "Study session", desc: "90 minutes, one session", plan: { label: "Study session", focusMins: 90, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "custom", title: "Custom", desc: "Your own length, breaks and rounds", plan: { label: "Focus", focusMins: 30, breakMins: 5, longBreakMins: 0, rounds: 1 } },
];

function parseTs(ts: string): number {
  return new Date(ts.replace(" ", "T")).getTime();
}

function mmss(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

function Ring({ pct, phase }: { pct: number; phase: FocusStatus["phase"] }) {
  const r = 88;
  const c = 2 * Math.PI * r;
  return (
    <svg viewBox="0 0 200 200" className="h-56 w-56 -rotate-90" aria-hidden>
      <circle cx="100" cy="100" r={r} fill="none" strokeWidth="10" className="stroke-slate-100 dark:stroke-slate-800" />
      <circle
        cx="100"
        cy="100"
        r={r}
        fill="none"
        strokeWidth="10"
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - pct)}
        className={phase === "focus" ? "stroke-indigo-500" : "stroke-emerald-500"}
        style={{ transition: "stroke-dashoffset 1s linear" }}
      />
    </svg>
  );
}

function ActiveTimer({ f, onStop }: { f: FocusStatus; onStop: () => void }) {
  const now = useNow(1000);
  const left = Math.max(0, (parseTs(f.endsAt) - now) / 1000);
  const pct = f.totalSecs ? 1 - left / f.totalSecs : 0;
  return (
    <div className="flex flex-col items-center py-4">
      <div className="relative">
        <Ring pct={pct} phase={f.phase} />
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          <span className="mb-1 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500">
            {f.phase === "focus" ? <Brain size={13} /> : <Coffee size={13} />}
            {f.phase === "focus" ? "Focus" : "Break"}
          </span>
          <span className="tabular text-4xl font-semibold">{mmss(left)}</span>
          {f.rounds > 1 && (
            <span className="mt-1 text-xs text-slate-500">
              Round {f.round} of {f.rounds}
            </span>
          )}
        </div>
      </div>
      <div className="mt-4 text-center">
        <div className="font-medium">{f.label}</div>
        <div className="text-sm text-slate-500">
          Ends at {clock(f.endsAt)}
          {f.blockDistracting && f.phase === "focus" && " · distracting sites blocked"}
        </div>
      </div>
      <div className="mt-5">
        <Button variant="danger" onClick={onStop}>
          <Square size={14} /> End session
        </Button>
      </div>
    </div>
  );
}

function Starter({ onStart }: { onStart: (p: FocusPlan) => void }) {
  const [presetId, setPresetId] = useState("pomodoro");
  const [plan, setPlan] = useState<FocusPlan>({ ...PRESETS[0].plan, blockDistracting: true });
  const field = "rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";
  const num = (v: string, min: number, max: number) => Math.max(min, Math.min(max, Number(v) || min));
  const total = plan.focusMins * plan.rounds + (plan.rounds - 1) * plan.breakMins;

  return (
    <div>
      <div className="grid gap-2 sm:grid-cols-2">
        {PRESETS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => {
              setPresetId(p.id);
              setPlan({ ...p.plan, blockDistracting: plan.blockDistracting });
            }}
            className={cx(
              "rounded-lg border p-3 text-left transition-colors",
              presetId === p.id
                ? "border-indigo-500 bg-indigo-50 dark:bg-indigo-950/50"
                : "border-slate-200 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800",
            )}
          >
            <div className="text-sm font-medium">{p.title}</div>
            <div className="text-xs text-slate-500">{p.desc}</div>
          </button>
        ))}
      </div>

      <div className="mt-4 grid gap-3 text-sm sm:grid-cols-2">
        <label className="sm:col-span-2">
          <span className="mb-1 block text-slate-500">What are you working on?</span>
          <input
            value={plan.label}
            maxLength={80}
            onChange={(e) => setPlan({ ...plan, label: e.target.value })}
            className={cx(field, "w-full")}
          />
        </label>
        {presetId === "custom" && (
          <>
            <label>
              <span className="mb-1 block text-slate-500">Focus (minutes)</span>
              <input type="number" min={1} max={240} value={plan.focusMins} onChange={(e) => setPlan({ ...plan, focusMins: num(e.target.value, 1, 240) })} className={cx(field, "w-full")} />
            </label>
            <label>
              <span className="mb-1 block text-slate-500">Rounds</span>
              <input type="number" min={1} max={12} value={plan.rounds} onChange={(e) => setPlan({ ...plan, rounds: num(e.target.value, 1, 12) })} className={cx(field, "w-full")} />
            </label>
            <label>
              <span className="mb-1 block text-slate-500">Break (minutes)</span>
              <input type="number" min={0} max={60} value={plan.breakMins} onChange={(e) => setPlan({ ...plan, breakMins: Math.max(0, Math.min(60, Number(e.target.value) || 0)) })} className={cx(field, "w-full")} />
            </label>
            <label>
              <span className="mb-1 block text-slate-500">Long break every 4 rounds</span>
              <input type="number" min={0} max={90} value={plan.longBreakMins} onChange={(e) => setPlan({ ...plan, longBreakMins: Math.max(0, Math.min(90, Number(e.target.value) || 0)) })} className={cx(field, "w-full")} />
            </label>
          </>
        )}
      </div>

      <label className="mt-4 flex items-center gap-3 text-sm">
        <Toggle label="Block distracting websites" checked={plan.blockDistracting} onChange={(v) => setPlan({ ...plan, blockDistracting: v })} />
        Block distracting websites (entertainment, social media, gaming) while focusing
      </label>

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" onClick={() => onStart(plan)} disabled={!plan.label.trim()}>
          <Play size={14} /> Start
        </Button>
        <span className="text-sm text-slate-500">
          {plan.rounds > 1 ? `${plan.rounds} rounds · about ${fmtDuration(total * 60)} total` : `${fmtDuration(plan.focusMins * 60)}`}
        </span>
      </div>
    </div>
  );
}

export default function Focus() {
  const { data, error, reload } = useLoad(() => api.focus(), [], 5000);
  const [actionError, setActionError] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>) => {
    setActionError(null);
    try {
      await fn();
      reload();
    } catch (e) {
      setActionError(String(e));
    }
  };

  return (
    <>
      <PageHeader title="Focus Mode" subtitle="Timed focus sessions, Pomodoro cycles and daily goals. The timer keeps running when the window is closed." />
      <ErrorNote error={error ?? actionError} />

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat label="Focused today" value={fmtDuration(data?.todaySecs ?? 0)} hint={`${data?.todaySessions ?? 0} completed`} />
        <Stat label="This week" value={fmtDuration(data?.weekSecs ?? 0)} hint={`${data?.weekSessions ?? 0} completed`} />
        <Stat
          label="Focus streak"
          value={
            <span className="inline-flex items-center gap-1.5">
              {data?.streakDays ?? 0} <Flame size={20} className={(data?.streakDays ?? 0) > 0 ? "text-orange-500" : "text-slate-300"} />
            </span>
          }
          hint="Days in a row with a completed session"
        />
        <Stat label="Sessions recorded" value={data?.history.length ?? 0} hint="Most recent 30 shown below" />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-5">
        <Card className="lg:col-span-3" title={data?.active ? "In progress" : "Start a session"}>
          {data?.active ? (
            <ActiveTimer f={data.active} onStop={() => run(() => api.stopFocus())} />
          ) : (
            <Starter onStart={(p) => run(() => api.startFocus(p))} />
          )}
        </Card>
        <Card className="lg:col-span-2" title="History">
          {!data?.history.length ? (
            <Empty>Completed sessions will show up here.</Empty>
          ) : (
            <ul className="max-h-[420px] divide-y divide-slate-100 overflow-y-auto dark:divide-slate-800/60">
              {data.history.map((h) => (
                <li key={h.id} className="flex items-center gap-3 py-2 text-sm">
                  <Timer size={15} className={h.completed ? "text-indigo-500" : "text-slate-400"} />
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-medium">{h.label}</div>
                    <div className="text-xs text-slate-500">
                      {dayLabel(h.day)} · {clock(h.start)}
                    </div>
                  </div>
                  <div className="text-right">
                    <div className="tabular">{fmtDuration(h.actualSecs)}</div>
                    <div className={cx("text-xs", h.completed ? "text-emerald-600 dark:text-emerald-400" : "text-slate-400")}>
                      {h.completed ? "completed" : `ended early (${fmtDuration(h.plannedSecs)} planned)`}
                    </div>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>

      <Card className="mt-4" title="Daily goals">
        <GoalManager />
      </Card>
    </>
  );
}
