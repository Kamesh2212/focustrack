import { useEffect, useState } from "react";
import { AppWindow, Ban, Brain, CheckCircle2, Coffee, Globe, Lock, Play, Plus, ShieldCheck, Square, Trash2, X } from "lucide-react";
import { api } from "../lib/api";
import { useLoad, useNow } from "../lib/hooks";
import { clock, fmtDuration } from "../lib/format";
import type { BlockPreview, EndEarly, FocusPlan, FocusStatus, Profile } from "../lib/types";
import { Button, Card, Empty, ErrorNote, Toggle, cx } from "./ui";
import { startSessionMusic } from "./FocusMusic";
import { music } from "../lib/music";

export const ALL_CATEGORIES = ["Gaming", "Entertainment", "Social Media", "Communication", "Productivity", "Study", "Education", "Development", "Other"];

const END_EARLY_TEXT: Record<EndEarly, string> = {
  allow: "Can be ended any time",
  confirm: "Ending early needs a typed confirmation",
  strict: "Ending early needs a request, a 1-minute wait and a typed confirmation",
};

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

// ------------------------------------------------------------------ preview

function Chips({ items: raw, tone }: { items: string[]; tone: "block" | "allow" }) {
  const items = [...new Set(raw)];
  const [all, setAll] = useState(false);
  const shown = all ? items : items.slice(0, 14);
  return (
    <div className="flex flex-wrap gap-1.5">
      {shown.map((x) => (
        <span
          key={x}
          className={cx(
            "rounded-full px-2 py-0.5 text-xs",
            tone === "block" ? "bg-rose-50 text-rose-700 dark:bg-rose-950/60 dark:text-rose-300" : "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/60 dark:text-emerald-300",
          )}
        >
          {x}
        </span>
      ))}
      {items.length > 14 && (
        <button type="button" onClick={() => setAll(!all)} className="text-xs text-indigo-600 hover:underline dark:text-indigo-400">
          {all ? "Show less" : `+${items.length - 14} more`}
        </button>
      )}
    </div>
  );
}

/** "These apps and websites will be blocked" — shown before and during a session. */
export function BlockList({ preview }: { preview: BlockPreview }) {
  if (preview.mode === "allow_only") {
    return (
      <div className="space-y-3 text-sm">
        <p className="text-slate-600 dark:text-slate-300">
          <b>Allow only:</b> everything else is blocked. Browsers stay open but only these websites load.
        </p>
        <div>
          <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500"><AppWindow size={13} /> Allowed apps</div>
          {preview.allowedApps.length ? <Chips tone="allow" items={preview.allowedApps.map((a) => a.name)} /> : <span className="text-xs text-slate-500">Only your browsers</span>}
        </div>
        <div>
          <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500"><Globe size={13} /> Allowed websites</div>
          <Chips tone="allow" items={preview.allowedSites} />
        </div>
        {preview.blockedApps.length > 0 && (
          <div>
            <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500"><Ban size={13} /> Apps that will be closed if opened</div>
            <Chips tone="block" items={preview.blockedApps.map((a) => a.name)} />
          </div>
        )}
      </div>
    );
  }
  return (
    <div className="space-y-3 text-sm">
      <div>
        <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500"><AppWindow size={13} /> Apps blocked ({preview.blockedApps.length})</div>
        {preview.blockedApps.length ? <Chips tone="block" items={preview.blockedApps.map((a) => a.name)} /> : <span className="text-xs text-slate-500">None</span>}
      </div>
      <div>
        <div className="mb-1.5 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500"><Globe size={13} /> Websites blocked ({preview.blockedSites.length})</div>
        {preview.blockedSites.length ? <Chips tone="block" items={preview.blockedSites} /> : <span className="text-xs text-slate-500">None</span>}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------- starter

export interface Preset {
  id: string;
  title: string;
  desc: string;
  plan: Pick<FocusPlan, "label" | "focusMins" | "breakMins" | "longBreakMins" | "rounds">;
}

export const FOCUS_PRESETS: Preset[] = [
  { id: "pomodoro", title: "Pomodoro", desc: "4 × 25 min, 5 min breaks, 15 min long break", plan: { label: "Pomodoro", focusMins: 25, breakMins: 5, longBreakMins: 15, rounds: 4 } },
  { id: "deep", title: "Deep work", desc: "50 minutes", plan: { label: "Deep work", focusMins: 50, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "study", title: "Long session", desc: "90 minutes", plan: { label: "Focus", focusMins: 90, breakMins: 0, longBreakMins: 0, rounds: 1 } },
  { id: "custom", title: "Custom", desc: "Your own length, breaks and rounds", plan: { label: "Focus", focusMins: 30, breakMins: 5, longBreakMins: 0, rounds: 1 } },
];

export function SessionStarter({
  mode,
  presets,
  defaultProfile,
  subjects,
  onStarted,
}: {
  mode: "focus" | "study";
  presets: Preset[];
  defaultProfile: string;
  subjects?: string[];
  onStarted: () => void;
}) {
  const profiles = useLoad(() => api.profiles(), []);
  const goals = useLoad(() => api.goals(), []);
  const [presetId, setPresetId] = useState(presets[0].id);
  const [plan, setPlan] = useState<FocusPlan>({ ...presets[0].plan, profileId: null, mode, subject: null, goalText: null, goalId: null });
  const [preview, setPreview] = useState<BlockPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  // Pick the default profile once profiles load.
  useEffect(() => {
    if (profiles.data && plan.profileId === null) {
      const p = profiles.data.find((x) => x.name === defaultProfile) ?? profiles.data[0];
      if (p?.id) setPlan((pl) => ({ ...pl, profileId: p.id }));
    }
  }, [profiles.data, defaultProfile, plan.profileId]);

  useEffect(() => {
    setPreview(null);
    if (plan.profileId != null && plan.profileId > 0) api.previewProfile(plan.profileId).then(setPreview).catch((e) => setError(String(e)));
  }, [plan.profileId]);

  const field = "rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";
  const num = (v: string, min: number, max: number) => Math.max(min, Math.min(max, Number(v) || min));
  const total = plan.focusMins * plan.rounds + (plan.rounds - 1) * plan.breakMins;
  const profile = profiles.data?.find((p) => p.id === plan.profileId) ?? null;
  const customGoals = (goals.data ?? []).filter((g) => g.enabled && g.state !== "achieved");

  const start = async () => {
    setError(null);
    setStarting(true);
    try {
      const label = mode === "study" ? (plan.subject?.trim() || "Study") : plan.label;
      await api.startFocus({ ...plan, label, profileId: plan.profileId && plan.profileId > 0 ? plan.profileId : null });
      // Started from a click, so the browser engine lets the music play.
      startSessionMusic();
      onStarted();
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  };

  return (
    <div>
      <ErrorNote error={error} />
      <div className="grid gap-2 sm:grid-cols-2">
        {presets.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => {
              setPresetId(p.id);
              setPlan({ ...plan, ...p.plan, label: mode === "focus" ? p.plan.label : plan.label });
            }}
            className={cx(
              "rounded-lg border p-3 text-left transition-colors",
              presetId === p.id ? "border-indigo-500 bg-indigo-50 dark:bg-indigo-950/50" : "border-slate-200 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800",
            )}
          >
            <div className="text-sm font-medium">{p.title}</div>
            <div className="text-xs text-slate-500">{p.desc}</div>
          </button>
        ))}
      </div>

      <div className="mt-4 grid gap-3 text-sm sm:grid-cols-2">
        {mode === "study" ? (
          <label>
            <span className="mb-1 block text-slate-500">Subject</span>
            <input list="ft-subjects" value={plan.subject ?? ""} maxLength={60} placeholder="e.g. Maths" onChange={(e) => setPlan({ ...plan, subject: e.target.value })} className={cx(field, "w-full")} />
            <datalist id="ft-subjects">
              {(subjects ?? []).map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
          </label>
        ) : (
          <label>
            <span className="mb-1 block text-slate-500">Session name</span>
            <input value={plan.label} maxLength={80} onChange={(e) => setPlan({ ...plan, label: e.target.value })} className={cx(field, "w-full")} />
          </label>
        )}
        <label>
          <span className="mb-1 block text-slate-500">Goal for this session (optional)</span>
          <input
            list="ft-goals"
            value={plan.goalText ?? ""}
            maxLength={120}
            placeholder="e.g. Finish the assignment"
            onChange={(e) => {
              const text = e.target.value;
              const match = customGoals.find((g) => g.label === text);
              setPlan({ ...plan, goalText: text, goalId: match?.id ?? null });
            }}
            className={cx(field, "w-full")}
          />
          <datalist id="ft-goals">
            {customGoals.filter((g) => g.kind === "custom").map((g) => (
              <option key={g.id} value={g.label} />
            ))}
          </datalist>
        </label>
        {presetId === "custom" && (
          <>
            <label>
              <span className="mb-1 block text-slate-500">Minutes per round</span>
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
        <label className="sm:col-span-2">
          <span className="mb-1 block text-slate-500">Blocking profile</span>
          <select value={plan.profileId ?? 0} onChange={(e) => setPlan({ ...plan, profileId: Number(e.target.value) || -1 })} className={cx(field, "w-full")}>
            <option value={-1}>No blocking</option>
            {profiles.data?.map((p) => (
              <option key={p.id!} value={p.id!}>
                {p.name}
                {p.mode === "allow_only" ? " (allow only)" : ""}
              </option>
            ))}
          </select>
        </label>
      </div>

      {profile && (
        <div className="mt-4 rounded-lg border border-slate-200 p-3 dark:border-slate-700">
          <div className="mb-2 flex flex-wrap items-center gap-2 text-sm font-medium">
            <ShieldCheck size={16} className="text-indigo-500" /> During this session
            <span className="text-xs font-normal text-slate-500">
              · until the whole session ends{profile.unblockOnBreaks ? " (open during breaks)" : ", breaks included"} · {END_EARLY_TEXT[profile.endEarly]}
            </span>
          </div>
          {preview ? <BlockList preview={preview} /> : <p className="text-xs text-slate-500">Working out what will be blocked…</p>}
        </div>
      )}

      <div className="mt-5 flex items-center gap-3">
        <Button variant="primary" onClick={start} disabled={starting || (mode === "focus" && !plan.label.trim())}>
          <Play size={14} /> {mode === "study" ? "Start studying" : "Start"}
        </Button>
        <span className="text-sm text-slate-500">
          {plan.rounds > 1 ? `${plan.rounds} rounds · about ${fmtDuration(total * 60)} total` : fmtDuration(plan.focusMins * 60)}
        </span>
      </div>
    </div>
  );
}

// ------------------------------------------------------------ active timer

function Ring({ pct, phase }: { pct: number; phase: FocusStatus["phase"] }) {
  const r = 88;
  const c = 2 * Math.PI * r;
  return (
    <svg viewBox="0 0 200 200" className="h-52 w-52 -rotate-90" aria-hidden>
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

export function ActiveSession({ f, stopPhrase, onChange }: { f: FocusStatus; stopPhrase: string; onChange: () => void }) {
  const now = useNow(1000);
  const left = Math.max(0, (parseTs(f.endsAt) - now) / 1000);
  const pct = f.totalSecs ? 1 - left / f.totalSecs : 0;
  const [ending, setEnding] = useState(false);
  const [phrase, setPhrase] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [requestedAt, setRequestedAt] = useState<number | null>(f.stopWaitSecs != null ? Date.now() - (60 - f.stopWaitSecs) * 1000 : null);
  const waitLeft = requestedAt ? Math.max(0, 60 - Math.floor((now - requestedAt) / 1000)) : null;

  const stop = async () => {
    setError(null);
    try {
      await api.stopFocus(phrase);
      music.stop();
      onChange();
    } catch (e) {
      setError(String(e));
    }
  };

  const beginEnd = async () => {
    setEnding(true);
    if (f.endEarly === "allow") return stop();
    if (f.endEarly === "strict" && requestedAt === null) {
      await api.requestStopFocus();
      setRequestedAt(Date.now());
    }
  };

  const Icon = f.phase === "focus" ? (f.mode === "study" ? Brain : Brain) : Coffee;
  return (
    <div className="grid gap-6 lg:grid-cols-[auto_1fr]">
      <div className="flex flex-col items-center">
        <div className="relative">
          <Ring pct={pct} phase={f.phase} />
          <div className="absolute inset-0 flex flex-col items-center justify-center">
            <span className="mb-1 flex items-center gap-1.5 text-xs font-medium uppercase tracking-wide text-slate-500">
              <Icon size={13} />
              {f.phase === "focus" ? (f.mode === "study" ? "Studying" : "Focus") : "Break"}
            </span>
            <span className="tabular text-4xl font-semibold">{mmss(left)}</span>
            {f.rounds > 1 && <span className="mt-1 text-xs text-slate-500">Round {f.round} of {f.rounds}</span>}
          </div>
        </div>
        <div className="mt-3 text-center">
          <div className="font-medium">{f.subject || f.label}</div>
          {f.goalText && <div className="text-sm text-slate-600 dark:text-slate-300">Goal: {f.goalText}</div>}
          <div className="text-xs text-slate-500">Session ends at {clock(f.planEndsAt)}</div>
        </div>

        <div className="mt-4 w-full max-w-xs">
          {!ending ? (
            <div className="flex justify-center">
              <Button variant="secondary" onClick={beginEnd}>
                <Square size={14} /> End early
              </Button>
            </div>
          ) : (
            <div className="rounded-lg border border-slate-200 p-3 text-sm dark:border-slate-700">
              <ErrorNote error={error} />
              {f.endEarly === "strict" && waitLeft !== null && waitLeft > 0 ? (
                <p className="text-slate-600 dark:text-slate-300">
                  <Lock size={13} className="mr-1 inline" /> Take a breath. You can end this session in <b className="tabular">{waitLeft}s</b>.
                </p>
              ) : (
                <>
                  <p className="mb-2 text-slate-600 dark:text-slate-300">
                    Type <b>{stopPhrase}</b> to end now. Unfinished sessions are saved as ended early.
                  </p>
                  <input value={phrase} onChange={(e) => setPhrase(e.target.value)} className="mb-2 w-full rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900" aria-label="Confirmation phrase" autoFocus />
                  <div className="flex gap-2">
                    <Button variant="danger" onClick={stop} disabled={phrase.trim().toLowerCase() !== stopPhrase.toLowerCase()}>
                      End session
                    </Button>
                    <Button onClick={() => setEnding(false)}>Keep going</Button>
                  </div>
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <div>
        <div className="mb-2 flex items-center gap-2 text-sm font-medium">
          {f.blocking ? <Ban size={15} className="text-rose-500" /> : <CheckCircle2 size={15} className="text-emerald-500" />}
          {f.blocking ? `${f.profileName} is on — blocked until ${clock(f.planEndsAt)}` : f.preview ? "Blocked items are open during this break" : "Nothing is blocked in this session"}
        </div>
        {f.preview ? <BlockList preview={f.preview} /> : <Empty>Pick a blocking profile next time to keep distractions away.</Empty>}
        <p className="mt-3 text-xs text-slate-500">Blocked apps are asked to close (so you can save), then closed. Everything opens again automatically when the session ends.</p>
      </div>
    </div>
  );
}

// ---------------------------------------------------------- profile editor

function ListEditor({ label, items, onChange, placeholder, options }: { label: string; items: string[]; onChange: (v: string[]) => void; placeholder: string; options?: { value: string; label: string }[] }) {
  const [v, setV] = useState("");
  const add = () => {
    const x = v.trim().toLowerCase().replace(/^https?:\/\//, "").replace(/^www\./, "").replace(/\/.*$/, "");
    if (x && !items.includes(x)) onChange([...items, x]);
    setV("");
  };
  const listId = `opts-${label.replace(/\W/g, "")}`;
  return (
    <div>
      <div className="mb-1 text-xs text-slate-500">{label}</div>
      <div className="mb-2 flex flex-wrap gap-1.5">
        {items.map((x) => (
          <span key={x} className="inline-flex items-center gap-1 rounded-full bg-slate-100 px-2 py-0.5 text-xs dark:bg-slate-800">
            {options?.find((o) => o.value === x)?.label ?? x}
            <button type="button" onClick={() => onChange(items.filter((i) => i !== x))} aria-label={`Remove ${x}`}>
              <X size={11} />
            </button>
          </span>
        ))}
      </div>
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <input list={options ? listId : undefined} value={v} onChange={(e) => setV(e.target.value)} placeholder={placeholder} className="min-w-0 flex-1 rounded-md border border-slate-200 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900" />
        {options && (
          <datalist id={listId}>
            {options.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </datalist>
        )}
        <Button onClick={add}>Add</Button>
      </form>
    </div>
  );
}

export function ProfileManager() {
  const { data, reload } = useLoad(() => api.profiles(), []);
  const installed = useLoad(() => api.installedApps(), []);
  const [edit, setEdit] = useState<Profile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const appOptions = (installed.data ?? []).map((a) => ({ value: a.exeName.toLowerCase(), label: a.name }));

  const save = async () => {
    if (!edit) return;
    setError(null);
    try {
      await api.saveProfile(edit);
      setEdit(null);
      reload();
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <Card
      title="Blocking profiles"
      action={
        !edit && (
          <Button onClick={() => setEdit({ id: null, name: "", mode: "block", categories: ["Gaming", "Entertainment", "Social Media"], apps: [], sites: [], unblockOnBreaks: false, endEarly: "confirm", builtin: false })}>
            <Plus size={15} /> New profile
          </Button>
        )
      }
    >
      <ErrorNote error={error} />
      {edit ? (
        <div className="space-y-4 text-sm">
          <div className="flex flex-wrap gap-3">
            <label className="min-w-[200px] flex-1">
              <span className="mb-1 block text-xs text-slate-500">Name</span>
              <input value={edit.name} maxLength={60} onChange={(e) => setEdit({ ...edit, name: e.target.value })} className="w-full rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900" />
            </label>
            <label>
              <span className="mb-1 block text-xs text-slate-500">Rule</span>
              <select value={edit.mode} onChange={(e) => setEdit({ ...edit, mode: e.target.value as Profile["mode"] })} className="rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900">
                <option value="block">Block what I choose</option>
                <option value="allow_only">Allow only what I choose</option>
              </select>
            </label>
            <label>
              <span className="mb-1 block text-xs text-slate-500">Ending early</span>
              <select value={edit.endEarly} onChange={(e) => setEdit({ ...edit, endEarly: e.target.value as EndEarly })} className="rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900">
                <option value="allow">Allowed any time</option>
                <option value="confirm">Type a confirmation</option>
                <option value="strict">Request, wait 1 minute, confirm</option>
              </select>
            </label>
          </div>
          <div>
            <div className="mb-1.5 text-xs text-slate-500">{edit.mode === "block" ? "Block these categories" : "Allow these categories"}</div>
            <div className="flex flex-wrap gap-1.5">
              {ALL_CATEGORIES.map((c) => {
                const on = edit.categories.includes(c);
                return (
                  <button
                    key={c}
                    type="button"
                    onClick={() => setEdit({ ...edit, categories: on ? edit.categories.filter((x) => x !== c) : [...edit.categories, c] })}
                    className={cx("rounded-full border px-2.5 py-0.5 text-xs", on ? "border-indigo-600 bg-indigo-600 text-white" : "border-slate-200 text-slate-600 dark:border-slate-700 dark:text-slate-300")}
                  >
                    {c}
                  </button>
                );
              })}
            </div>
          </div>
          <div className="grid gap-4 md:grid-cols-2">
            <ListEditor label={edit.mode === "block" ? "Also block these apps" : "Also allow these apps"} items={edit.apps} onChange={(apps) => setEdit({ ...edit, apps })} placeholder="Start typing an app, e.g. discord.exe" options={appOptions} />
            <ListEditor label={edit.mode === "block" ? "Also block these websites" : "Also allow these websites"} items={edit.sites} onChange={(sites) => setEdit({ ...edit, sites })} placeholder="e.g. reddit.com" />
          </div>
          <label className="flex items-center gap-3">
            <Toggle label="Open blocked items during breaks" checked={edit.unblockOnBreaks} onChange={(v) => setEdit({ ...edit, unblockOnBreaks: v })} />
            Open blocked apps and websites during Pomodoro breaks
          </label>
          <div className="flex gap-2">
            <Button variant="primary" onClick={save}>Save profile</Button>
            <Button onClick={() => setEdit(null)}>Cancel</Button>
          </div>
        </div>
      ) : (
        <ul className="divide-y divide-slate-100 dark:divide-slate-800/60">
          {data?.map((p) => (
            <li key={p.id!} className="flex items-center gap-3 py-2.5 text-sm">
              <div className="min-w-0 flex-1">
                <div className="font-medium">{p.name}</div>
                <div className="truncate text-xs text-slate-500">
                  {p.mode === "allow_only" ? "Allows only: " : "Blocks: "}
                  {[...p.categories, ...p.apps, ...p.sites].join(", ") || "nothing yet"} · {END_EARLY_TEXT[p.endEarly]}
                </div>
              </div>
              <Button onClick={() => setEdit(p)}>Edit</Button>
              {!p.builtin && (
                <button
                  onClick={async () => {
                    await api.deleteProfile(p.id!);
                    reload();
                  }}
                  className="rounded p-1.5 text-slate-500 hover:bg-rose-50 hover:text-rose-600 dark:hover:bg-rose-950"
                  aria-label={`Delete ${p.name}`}
                >
                  <Trash2 size={15} />
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
