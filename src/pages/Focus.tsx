import { Flame, Timer } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { clock, dayLabel, fmtDuration } from "../lib/format";
import type { FocusRecord } from "../lib/types";
import { Card, Empty, ErrorNote, PageHeader, Stat, cx } from "../components/ui";
import { GoalManager } from "../components/Goals";
import { ActiveSession, FOCUS_PRESETS, ProfileManager, SessionStarter } from "../components/Session";
import { FocusMusicCard } from "../components/FocusMusic";

export function SessionHistory({ items }: { items: FocusRecord[] }) {
  if (!items.length) return <Empty>Completed sessions will show up here.</Empty>;
  return (
    <ul className="max-h-[440px] divide-y divide-slate-100 overflow-y-auto dark:divide-slate-800/60">
      {items.map((h) => (
        <li key={h.id} className="flex items-center gap-3 py-2 text-sm">
          <Timer size={15} className={h.completed ? "text-indigo-500" : "text-slate-400"} />
          <div className="min-w-0 flex-1">
            <div className="truncate font-medium">{h.subject || h.label}</div>
            <div className="truncate text-xs text-slate-500">
              {dayLabel(h.day)} · {clock(h.start)}
              {h.goalText && ` · ${h.goalText}`}
            </div>
          </div>
          <div className="text-right">
            <div className="tabular">{fmtDuration(h.actualSecs)}</div>
            <div className={cx("text-xs", h.completed ? "text-emerald-600 dark:text-emerald-400" : "text-slate-400")}>
              {h.completed ? "completed" : "ended early"}
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}

export default function Focus() {
  const { data, error, reload } = useLoad(() => api.focus(), [], 5000);

  return (
    <>
      <PageHeader
        title="Focus Mode"
        subtitle="Pick a blocking profile, see exactly what will be blocked, and focus. Everything opens again when the session ends."
      />
      <ErrorNote error={error} />

      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <Stat label="Focused today" value={fmtDuration(data?.todaySecs ?? 0)} hint={`${data?.todaySessions ?? 0} rounds completed`} />
        <Stat label="This week" value={fmtDuration(data?.weekSecs ?? 0)} hint={`${data?.weekSessions ?? 0} rounds completed`} />
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

      <Card className="mt-4" title={data?.active ? "In progress" : "Start a session"}>
        {data?.active ? (
          <ActiveSession f={data.active} stopPhrase={data.stopPhrase} onChange={reload} />
        ) : (
          <SessionStarter mode="focus" presets={FOCUS_PRESETS} defaultProfile="Work Mode" onStarted={reload} />
        )}
      </Card>

      <div className="mt-4">
        <FocusMusicCard />
      </div>

      <div className="mt-4 grid gap-4 lg:grid-cols-2">
        <ProfileManager />
        <Card title="History">
          <SessionHistory items={data?.history ?? []} />
        </Card>
      </div>

      <Card className="mt-4" title="Daily goals">
        <GoalManager />
      </Card>
    </>
  );
}
