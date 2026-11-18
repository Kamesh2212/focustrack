import { useState } from "react";
import { AppWindow, Globe, Pencil, Plus, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { addDays, fmtDuration, today } from "../lib/format";
import type { LimitStatus } from "../lib/types";
import { AppBadge, Button, Card, Empty, ErrorNote, PageHeader, Segmented, Toggle, cx } from "../components/ui";

type TargetType = "app" | "website";

interface Draft {
  targetType: TargetType;
  targetId: number | null;
  hours: number;
  minutes: number;
  warnPct: number;
  block: boolean;
  enabled: boolean;
}

const PRESETS = [15, 30, 60, 90, 120, 180];

function emptyDraft(targetType: TargetType = "website"): Draft {
  return { targetType, targetId: null, hours: 1, minutes: 0, warnPct: 80, block: false, enabled: true };
}

function LimitEditor({
  initial,
  existing,
  onDone,
  onCancel,
}: {
  initial: Draft;
  existing: LimitStatus[];
  onDone: () => void;
  onCancel: () => void;
}) {
  const [d, setD] = useState<Draft>(initial);
  const [error, setError] = useState<string | null>(null);
  const editing = initial.targetId !== null;
  const from = addDays(today(), -29);
  const apps = useLoad(() => api.apps(from, today()), []);
  const sites = useLoad(() => api.websites(from, today()), []);

  const taken = new Set(existing.filter((l) => l.targetType === d.targetType).map((l) => l.targetId));
  const options =
    d.targetType === "app"
      ? (apps.data ?? []).map((a) => ({ id: a.id, label: a.name, secs: a.secs }))
      : (sites.data ?? []).map((s) => ({ id: s.id, label: s.domain, secs: s.secs }));
  const available = options.filter((o) => editing || !taken.has(o.id));
  const limitSecs = d.hours * 3600 + d.minutes * 60;

  const save = async () => {
    setError(null);
    if (d.targetId === null) return setError(`Choose ${d.targetType === "app" ? "an application" : "a website"}.`);
    if (limitSecs < 60) return setError("The limit must be at least 1 minute.");
    try {
      await api.setLimit({
        targetType: d.targetType,
        targetId: d.targetId,
        dailyLimitSecs: limitSecs,
        warnPct: d.warnPct,
        block: d.targetType === "website" && d.block,
        enabled: d.enabled,
      });
      onDone();
    } catch (e) {
      setError(String(e));
    }
  };

  const field = "rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";

  return (
    <Card title={editing ? "Edit limit" : "New daily limit"} className="mb-4 border-indigo-200 dark:border-indigo-900">
      <ErrorNote error={error} />
      <div className="grid gap-4 md:grid-cols-2">
        <div className="space-y-3">
          {!editing && (
            <Segmented<TargetType>
              value={d.targetType}
              onChange={(t) => setD({ ...d, targetType: t, targetId: null, block: false })}
              options={[
                { value: "website", label: "Website" },
                { value: "app", label: "Application" },
              ]}
            />
          )}
          <label className="block text-sm">
            <span className="mb-1 block text-slate-500">{d.targetType === "app" ? "Application" : "Website"}</span>
            <select
              value={d.targetId ?? ""}
              disabled={editing}
              onChange={(e) => setD({ ...d, targetId: e.target.value ? Number(e.target.value) : null })}
              className={cx(field, "w-full")}
            >
              <option value="">Choose…</option>
              {available.map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label} {o.secs > 0 ? `· ${fmtDuration(o.secs / 30)}/day avg` : ""}
                </option>
              ))}
            </select>
            {!editing && available.length === 0 && (
              <span className="mt-1 block text-xs text-slate-500">
                {d.targetType === "website"
                  ? "Websites appear here once the browser extension has recorded them."
                  : "Applications appear here once they have been used."}
              </span>
            )}
          </label>
        </div>

        <div className="space-y-3">
          <div className="text-sm">
            <span className="mb-1 block text-slate-500">Daily limit</span>
            <div className="flex flex-wrap items-center gap-2">
              <input
                type="number"
                min={0}
                max={23}
                value={d.hours}
                onChange={(e) => setD({ ...d, hours: Math.max(0, Math.min(23, Number(e.target.value) || 0)) })}
                className={cx(field, "w-16")}
                aria-label="Hours"
              />
              h
              <input
                type="number"
                min={0}
                max={59}
                step={5}
                value={d.minutes}
                onChange={(e) => setD({ ...d, minutes: Math.max(0, Math.min(59, Number(e.target.value) || 0)) })}
                className={cx(field, "w-16")}
                aria-label="Minutes"
              />
              m
            </div>
            <div className="mt-2 flex flex-wrap gap-1.5">
              {PRESETS.map((m) => (
                <button
                  key={m}
                  type="button"
                  onClick={() => setD({ ...d, hours: Math.floor(m / 60), minutes: m % 60 })}
                  className={cx(
                    "rounded-full border px-2.5 py-0.5 text-xs",
                    limitSecs === m * 60
                      ? "border-indigo-600 bg-indigo-600 text-white"
                      : "border-slate-200 text-slate-600 hover:bg-slate-50 dark:border-slate-700 dark:text-slate-300 dark:hover:bg-slate-800",
                  )}
                >
                  {fmtDuration(m * 60)}
                </button>
              ))}
            </div>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <span className="text-slate-500">Warn me at</span>
            <select value={d.warnPct} onChange={(e) => setD({ ...d, warnPct: Number(e.target.value) })} className={field}>
              {[50, 75, 80, 90].map((p) => (
                <option key={p} value={p}>
                  {p}%
                </option>
              ))}
            </select>
          </label>
          {d.targetType === "website" && (
            <label className="flex items-center gap-3 text-sm">
              <Toggle label="Block when limit is reached" checked={d.block} onChange={(v) => setD({ ...d, block: v })} />
              Block the site in my browser when the limit is reached
            </label>
          )}
        </div>
      </div>
      <div className="mt-5 flex gap-2">
        <Button variant="primary" onClick={save}>
          {editing ? "Save changes" : "Add limit"}
        </Button>
        <Button onClick={onCancel}>Cancel</Button>
      </div>
    </Card>
  );
}

function LimitRow({ l, onEdit, onChange }: { l: LimitStatus; onEdit: () => void; onChange: () => void }) {
  const pct = Math.min(100, (l.usedSecs / l.dailyLimitSecs) * 100);
  const over = l.usedSecs >= l.dailyLimitSecs;
  const warn = !over && pct >= l.warnPct;
  const Icon = l.targetType === "app" ? AppWindow : Globe;
  return (
    <li className={cx("flex flex-wrap items-center gap-4 py-4", !l.enabled && "opacity-50")}>
      <AppBadge name={l.name} color={l.color} />
      <div className="min-w-[200px] flex-1">
        <div className="flex items-center gap-2">
          <span className="font-medium">{l.name}</span>
          <Icon size={13} className="text-slate-400" />
          {l.block && (
            <span className="rounded-full bg-slate-100 px-2 py-0.5 text-[11px] text-slate-600 dark:bg-slate-800 dark:text-slate-300">
              blocks when reached
            </span>
          )}
        </div>
        <div className="mt-2 h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
          <div
            className={cx("h-full rounded-full", over ? "bg-rose-500" : warn ? "bg-amber-500" : "bg-indigo-500")}
            style={{ width: `${Math.max(1.5, pct)}%` }}
          />
        </div>
        <div className="tabular mt-1.5 flex justify-between text-xs text-slate-500">
          <span>
            {fmtDuration(l.usedSecs)} of {fmtDuration(l.dailyLimitSecs)} today
          </span>
          <span className={cx(over && "font-medium text-rose-600 dark:text-rose-400", warn && "text-amber-600 dark:text-amber-400")}>
            {over ? (l.block ? "Limit reached · blocked" : "Limit reached") : `${fmtDuration(l.dailyLimitSecs - l.usedSecs)} left`}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-1">
        <Toggle
          label={`Limit for ${l.name} enabled`}
          checked={l.enabled}
          onChange={async (on) => {
            await api.setLimit({ ...l, enabled: on });
            onChange();
          }}
        />
        <button onClick={onEdit} className="ml-2 rounded p-1.5 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label={`Edit limit for ${l.name}`}>
          <Pencil size={15} />
        </button>
        <button
          onClick={async () => {
            await api.deleteLimit(l.id);
            onChange();
          }}
          className="rounded p-1.5 text-slate-500 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950"
          aria-label={`Delete limit for ${l.name}`}
        >
          <Trash2 size={15} />
        </button>
      </div>
    </li>
  );
}

export default function Limits() {
  const { data, error, reload } = useLoad(() => api.limits(), [], 15000);
  const [editor, setEditor] = useState<{ draft: Draft; key: number } | null>(null);
  const limits = data ?? [];

  const openNew = () => setEditor({ draft: emptyDraft(), key: Date.now() });
  const openEdit = (l: LimitStatus) =>
    setEditor({
      key: Date.now(),
      draft: {
        targetType: l.targetType,
        targetId: l.targetId,
        hours: Math.floor(l.dailyLimitSecs / 3600),
        minutes: Math.round((l.dailyLimitSecs % 3600) / 60),
        warnPct: l.warnPct,
        block: l.block,
        enabled: l.enabled,
      },
    });

  return (
    <>
      <PageHeader
        title="Usage Limits"
        subtitle="Daily limits reset at midnight. You'll get a Windows notification at the warning level and when a limit is reached."
        actions={
          !editor && (
            <Button variant="primary" onClick={openNew}>
              <Plus size={15} /> Add limit
            </Button>
          )
        }
      />
      <ErrorNote error={error} />
      {editor && (
        <LimitEditor
          key={editor.key}
          initial={editor.draft}
          existing={limits}
          onCancel={() => setEditor(null)}
          onDone={() => {
            setEditor(null);
            reload();
          }}
        />
      )}
      <Card>
        {limits.length === 0 ? (
          <Empty>No limits yet. Add one for an app or website you'd like to spend less time on.</Empty>
        ) : (
          <ul className="divide-y divide-slate-100 dark:divide-slate-800/60">
            {limits.map((l) => (
              <LimitRow key={l.id} l={l} onEdit={() => openEdit(l)} onChange={reload} />
            ))}
          </ul>
        )}
      </Card>
      <p className="mt-3 text-xs text-slate-500">
        Website blocking is done by the FocusTrack browser extension. Application limits notify you but don't close the app.
      </p>
    </>
  );
}
