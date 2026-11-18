import { useState } from "react";
import { CheckCircle2, Circle, Pencil, Plus, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { fmtDuration } from "../lib/format";
import type { GoalKind, GoalStatus } from "../lib/types";
import { Button, Empty, ErrorNote, Toggle, cx } from "./ui";

const KIND_LABELS: Record<GoalKind, string> = {
  custom: "Custom task (tick it off yourself)",
  min_category: "Spend at least … on a category",
  max_category: "Limit time on a category",
  max_screen: "Limit total screen time",
  min_focus_sessions: "Complete focus sessions",
};

function progressText(g: GoalStatus): string {
  if (g.kind === "custom") return g.state === "achieved" ? "Done" : "To do";
  return g.kind === "min_focus_sessions" ? `${g.current} of ${g.target}` : `${fmtDuration(g.current)} of ${fmtDuration(g.target)}`;
}

function GoalRow({ g, onToggle }: { g: GoalStatus; onToggle: (g: GoalStatus) => void }) {
  if (g.kind === "custom") {
    const done = g.state === "achieved";
    return (
      <button type="button" onClick={() => onToggle(g)} className="flex w-full items-center gap-2.5 py-0.5 text-left text-sm" aria-pressed={done}>
        {done ? <CheckCircle2 size={18} className="shrink-0 text-emerald-500" /> : <Circle size={18} className="shrink-0 text-slate-300 dark:text-slate-600" />}
        <span className={cx("truncate", done && "text-slate-400 line-through")}>{g.label}</span>
      </button>
    );
  }
  const pct = Math.min(100, (g.current / Math.max(1, g.target)) * 100);
  const tone =
    g.state === "achieved" ? "bg-emerald-500" : g.state === "exceeded" ? "bg-rose-500" : g.kind.startsWith("max") && pct >= 80 ? "bg-amber-500" : "bg-indigo-500";
  return (
    <div>
      <div className="flex items-baseline gap-2 text-sm">
        {g.state === "achieved" && <CheckCircle2 size={14} className="shrink-0 self-center text-emerald-500" />}
        <span className="truncate">{g.label}</span>
        <span className={cx("tabular ml-auto shrink-0 text-xs", g.state === "exceeded" ? "font-medium text-rose-600 dark:text-rose-400" : "text-slate-500")}>
          {g.state === "exceeded" ? `Over · ${progressText(g)}` : progressText(g)}
        </span>
      </div>
      <div className="mt-1.5 h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
        <div className={cx("h-full rounded-full", tone)} style={{ width: `${Math.max(2, pct)}%` }} />
      </div>
    </div>
  );
}

interface Draft {
  id: number | null;
  kind: GoalKind;
  categoryId: number | null;
  title: string;
  hours: number;
  minutes: number;
  count: number;
  enabled: boolean;
}

function toDraft(g?: GoalStatus): Draft {
  if (!g) return { id: null, kind: "custom", categoryId: null, title: "", hours: 2, minutes: 0, count: 3, enabled: true };
  return {
    id: g.id,
    kind: g.kind,
    categoryId: g.categoryId,
    title: g.title ?? "",
    hours: Math.floor(g.target / 3600),
    minutes: Math.round((g.target % 3600) / 60),
    count: g.kind === "min_focus_sessions" ? g.target : 3,
    enabled: g.enabled,
  };
}

function GoalEditor({ initial, onDone, onCancel }: { initial: Draft; onDone: () => void; onCancel: () => void }) {
  const [d, setD] = useState(initial);
  const [error, setError] = useState<string | null>(null);
  const cats = useLoad(() => api.categories(), []);
  const field = "rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";
  const needsCategory = d.kind === "min_category" || d.kind === "max_category";
  const isCount = d.kind === "min_focus_sessions";
  const isCustom = d.kind === "custom";

  const save = async () => {
    setError(null);
    if (isCustom && !d.title.trim()) return setError('Describe the goal, e.g. "Read 20 pages".');
    if (needsCategory && d.categoryId === null) return setError("Choose a category.");
    const target = isCustom ? 0 : isCount ? d.count : d.hours * 3600 + d.minutes * 60;
    if (!isCustom && !isCount && target < 60) return setError("Set at least 1 minute.");
    try {
      await api.setGoal({ id: d.id, kind: d.kind, categoryId: needsCategory ? d.categoryId : null, target, enabled: d.enabled, title: d.title.trim() || null });
      onDone();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="mb-4 rounded-lg border border-indigo-200 p-4 dark:border-indigo-900">
      <ErrorNote error={error} />
      <div className="flex flex-wrap items-end gap-3 text-sm">
        <label>
          <span className="mb-1 block text-slate-500">Type</span>
          <select value={d.kind} onChange={(e) => setD({ ...d, kind: e.target.value as GoalKind })} className={field}>
            {(Object.keys(KIND_LABELS) as GoalKind[]).map((k) => (
              <option key={k} value={k}>
                {KIND_LABELS[k]}
              </option>
            ))}
          </select>
        </label>
        <label className="min-w-[200px] flex-1">
          <span className="mb-1 block text-slate-500">{isCustom ? "Goal" : "Name (optional)"}</span>
          <input
            value={d.title}
            maxLength={120}
            placeholder={isCustom ? "Complete the university assignment" : needsCategory ? "e.g. Coding, Reading" : ""}
            onChange={(e) => setD({ ...d, title: e.target.value })}
            className={cx(field, "w-full")}
          />
        </label>
        {needsCategory && (
          <label>
            <span className="mb-1 block text-slate-500">Category</span>
            <select value={d.categoryId ?? ""} onChange={(e) => setD({ ...d, categoryId: e.target.value ? Number(e.target.value) : null })} className={field}>
              <option value="">Choose…</option>
              {cats.data?.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
        )}
        {isCount && (
          <label>
            <span className="mb-1 block text-slate-500">Sessions per day</span>
            <input type="number" min={1} max={20} value={d.count} onChange={(e) => setD({ ...d, count: Math.max(1, Math.min(20, Number(e.target.value) || 1)) })} className={cx(field, "w-20")} />
          </label>
        )}
        {!isCustom && !isCount && (
          <div>
            <span className="mb-1 block text-slate-500">{d.kind.startsWith("max") ? "At most" : "At least"}</span>
            <div className="flex items-center gap-1.5">
              <input type="number" min={0} max={23} value={d.hours} onChange={(e) => setD({ ...d, hours: Math.max(0, Math.min(23, Number(e.target.value) || 0)) })} className={cx(field, "w-16")} aria-label="Hours" />
              h
              <input type="number" min={0} max={59} step={5} value={d.minutes} onChange={(e) => setD({ ...d, minutes: Math.max(0, Math.min(59, Number(e.target.value) || 0)) })} className={cx(field, "w-16")} aria-label="Minutes" />
              m
            </div>
          </div>
        )}
        <div className="flex gap-2">
          <Button variant="primary" onClick={save}>
            {d.id ? "Save" : "Add goal"}
          </Button>
          <Button onClick={onCancel}>Cancel</Button>
        </div>
      </div>
      {needsCategory && <p className="mt-2 text-xs text-slate-500">Counts apps and websites in this category (browser time is counted by website).</p>}
    </div>
  );
}

/**
 * Today's goals with progress, tick-off for custom goals, quick add, and
 * (unless `compact`) edit / enable / delete.
 */
export function GoalManager({ compact = false }: { compact?: boolean }) {
  const { data, error, reload } = useLoad(() => api.goals(), [], 30000);
  const [editing, setEditing] = useState<{ draft: Draft; key: number } | null>(null);
  const [quick, setQuick] = useState("");
  const [manage, setManage] = useState(!compact);
  const goals = (data ?? []).filter((g) => manage || g.enabled);
  const pending = goals.filter((g) => g.enabled && g.state !== "achieved" && g.state !== "on_track").length;
  const done = goals.filter((g) => g.enabled && g.state === "achieved").length;

  const toggle = async (g: GoalStatus) => {
    await api.toggleGoal(g.id);
    reload();
  };
  const addQuick = async () => {
    const title = quick.trim();
    if (!title) return;
    await api.setGoal({ id: null, kind: "custom", categoryId: null, target: 0, enabled: true, title });
    setQuick("");
    reload();
  };

  return (
    <div>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500">
          {done} done · {pending} pending
        </span>
        <div className="ml-auto flex gap-2">
          {compact && (
            <Button onClick={() => setManage(!manage)}>{manage ? "Done editing" : "Edit goals"}</Button>
          )}
          {!editing && (
            <Button onClick={() => setEditing({ draft: toDraft(), key: Date.now() })}>
              <Plus size={15} /> Add goal
            </Button>
          )}
        </div>
      </div>
      <ErrorNote error={error} />
      {editing && (
        <GoalEditor
          key={editing.key}
          initial={editing.draft}
          onCancel={() => setEditing(null)}
          onDone={() => {
            setEditing(null);
            reload();
          }}
        />
      )}
      {goals.length === 0 ? (
        <Empty>No goals yet. Try "Study 3 hours", "Social media under 30m" or a task like "Read 20 pages".</Empty>
      ) : (
        <ul className={cx(manage ? "divide-y divide-slate-100 dark:divide-slate-800/60" : "space-y-3")}>
          {goals.map((g) => (
            <li key={g.id} className={cx("flex items-center gap-3", manage && "py-2.5", !g.enabled && "opacity-50")}>
              <div className="min-w-0 flex-1">
                <GoalRow g={g} onToggle={toggle} />
              </div>
              {manage && (
                <>
                  <Toggle
                    label={`Goal enabled: ${g.label}`}
                    checked={g.enabled}
                    onChange={async (on) => {
                      await api.setGoal({ id: g.id, kind: g.kind, categoryId: g.categoryId, target: g.target, enabled: on, title: g.title });
                      reload();
                    }}
                  />
                  <button onClick={() => setEditing({ draft: toDraft(g), key: Date.now() })} className="rounded p-1.5 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label={`Edit goal: ${g.label}`}>
                    <Pencil size={15} />
                  </button>
                  <button
                    onClick={async () => {
                      await api.deleteGoal(g.id);
                      reload();
                    }}
                    className="rounded p-1.5 text-slate-500 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950"
                    aria-label={`Delete goal: ${g.label}`}
                  >
                    <Trash2 size={15} />
                  </button>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
      <form
        className="mt-3 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          addQuick();
        }}
      >
        <input
          value={quick}
          maxLength={120}
          onChange={(e) => setQuick(e.target.value)}
          placeholder="Add a to-do for today, e.g. Read 20 pages"
          className="min-w-0 flex-1 rounded-lg border border-slate-200 bg-white px-3 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
          aria-label="New to-do"
        />
        <Button onClick={addQuick} disabled={!quick.trim()}>
          Add
        </Button>
      </form>
    </div>
  );
}

/** Read-only progress list (kept for small spaces). */
export function GoalProgress({ goals }: { goals: GoalStatus[]; compact?: boolean }) {
  const shown = goals.filter((g) => g.enabled);
  if (!shown.length) return <Empty>No goals yet.</Empty>;
  return (
    <ul className="space-y-3">
      {shown.map((g) => (
        <li key={g.id}>
          <GoalRow g={g} onToggle={() => {}} />
        </li>
      ))}
    </ul>
  );
}
