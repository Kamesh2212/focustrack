import { useState } from "react";
import { Cloud, CloudOff, Copy, Download, KeyRound, Laptop, Loader2, LockKeyhole, RefreshCw, Smartphone, Trash2 } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import type { SyncDevice, SyncStatus } from "../lib/types";
import { Button, Card, ConfirmButton, ErrorNote, Toggle, cx } from "./ui";

/** Until the hosted service exists, sync talks to a server on this computer. */
const DEFAULT_SERVER = "http://127.0.0.1:8787";

const field =
  "w-full rounded-md border border-slate-200 bg-white px-2.5 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900";

/** Same look as `field`, sized by its own width class. */
const inline = field.replace("w-full ", "");

function since(t: string | null): string {
  if (!t) return "never";
  const ms = Date.now() - new Date(t.replace(" ", "T")).getTime();
  if (Number.isNaN(ms)) return t;
  if (ms < 60_000) return "just now";
  if (ms < 3_600_000) return `${Math.round(ms / 60_000)} min ago`;
  if (ms < 86_400_000) return `${Math.round(ms / 3_600_000)} h ago`;
  return `${Math.round(ms / 86_400_000)} days ago`;
}

function StatusLine({ s }: { s: SyncStatus }) {
  if (s.state === "syncing")
    return (
      <span className="inline-flex items-center gap-1.5 text-indigo-600">
        <Loader2 size={14} className="animate-spin" /> Syncing…
      </span>
    );
  if (s.state === "error")
    return <span className="text-amber-600">Couldn't sync — will retry. {s.lastError}</span>;
  if (!s.enabled) return <span className="text-slate-500">Sync is paused on this device.</span>;
  return (
    <span className="text-slate-600 dark:text-slate-300">
      Synced {since(s.lastSync)}
      {s.pending > 0 && ` · ${s.pending} change${s.pending === 1 ? "" : "s"} waiting`}
    </span>
  );
}

function RecoveryKey({ value, onDone }: { value: string; onDone: () => void }) {
  const [saved, setSaved] = useState(false);
  const [copied, setCopied] = useState(false);
  return (
    <div className="rounded-lg border border-amber-300 bg-amber-50 p-4 dark:border-amber-800 dark:bg-amber-950/40">
      <div className="mb-1 flex items-center gap-2 font-medium text-amber-900 dark:text-amber-200">
        <KeyRound size={16} /> Save your recovery key
      </div>
      <p className="mb-3 text-sm text-amber-900/80 dark:text-amber-200/80">
        Your data is encrypted with a key only your devices know. If you forget your password, this key is the{" "}
        <b>only</b> way to read your synced history. FocusTrack can't recover it for you. Write it down or keep it in a
        password manager.
      </p>
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <code className="select-all break-all rounded bg-white px-2 py-1.5 font-mono text-sm dark:bg-slate-900">{value}</code>
        <Button
          onClick={() => {
            navigator.clipboard?.writeText(value).then(() => setCopied(true));
          }}
        >
          <Copy size={14} /> {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" checked={saved} onChange={(e) => setSaved(e.target.checked)} /> I've saved my recovery key
      </label>
      <div className="mt-3">
        <Button variant="primary" disabled={!saved} onClick={onDone}>
          Done
        </Button>
      </div>
    </div>
  );
}

function SignInForm({ status, onSignedIn }: { status: SyncStatus; onSignedIn: (recoveryKey: string | null) => void }) {
  const [mode, setMode] = useState<"in" | "up">("in");
  const [server, setServer] = useState(status.server ?? DEFAULT_SERVER);
  const [email, setEmail] = useState(status.email ?? "");
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [name, setName] = useState(status.deviceName);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setError(null);
    if (mode === "up" && password !== confirm) return setError("The passwords don't match");
    setBusy(true);
    try {
      if (mode === "up") onSignedIn(await api.syncSignUp(server, email, password, name));
      else {
        await api.syncSignIn(server, email, password, name);
        onSignedIn(null);
      }
      setPassword("");
      setConfirm("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="max-w-md">
      <div className="mb-3 inline-flex rounded-lg bg-slate-100 p-0.5 text-sm dark:bg-slate-800">
        {(["in", "up"] as const).map((m) => (
          <button
            key={m}
            type="button"
            onClick={() => setMode(m)}
            className={cx("rounded-md px-3 py-1", mode === m && "bg-white shadow-sm dark:bg-slate-900")}
          >
            {m === "in" ? "Sign in" : "Create account"}
          </button>
        ))}
      </div>
      <form
        className="space-y-2.5"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <label className="block text-sm">
          <span className="text-slate-500">Email</span>
          <input type="email" autoComplete="username" value={email} onChange={(e) => setEmail(e.target.value)} className={field} required />
        </label>
        <label className="block text-sm">
          <span className="text-slate-500">Password{mode === "up" && " (at least 10 characters)"}</span>
          <input
            type="password"
            autoComplete={mode === "up" ? "new-password" : "current-password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            className={field}
            required
          />
        </label>
        {mode === "up" && (
          <label className="block text-sm">
            <span className="text-slate-500">Confirm password</span>
            <input type="password" autoComplete="new-password" value={confirm} onChange={(e) => setConfirm(e.target.value)} className={field} required />
          </label>
        )}
        <label className="block text-sm">
          <span className="text-slate-500">Name for this device</span>
          <input value={name} maxLength={40} onChange={(e) => setName(e.target.value)} className={field} required />
        </label>
        <details className="text-sm">
          <summary className="cursor-pointer text-slate-500">Server</summary>
          <input value={server} onChange={(e) => setServer(e.target.value)} className={cx(field, "mt-1.5 font-mono")} />
          <p className="mt-1 text-xs text-slate-500">
            The FocusTrack sync server you run yourself: <code>http://127.0.0.1:8787</code> on the computer running it, or its
            home-network address (e.g. <code>http://192.168.1.20:8787</code>) from your other devices.
          </p>
        </details>
        <ErrorNote error={error} />
        <button
          type="submit"
          disabled={busy}
          className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-indigo-700 disabled:opacity-50"
        >
          {busy && <Loader2 size={14} className="animate-spin" />}
          {busy ? (mode === "up" ? "Creating your encryption keys…" : "Signing in…") : mode === "up" ? "Create account and sync" : "Sign in and sync"}
        </button>
      </form>
    </div>
  );
}

function Devices({ onError }: { onError: (e: string) => void }) {
  const { data, reload, loading } = useLoad(() => api.syncDevices(), []);
  return (
    <div>
      <div className="mb-2 flex items-center justify-between">
        <div className="text-sm font-medium">Devices on this account</div>
        <button onClick={reload} className="rounded p-1 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label="Refresh devices">
          <RefreshCw size={14} className={cx(loading && "animate-spin")} />
        </button>
      </div>
      <ul className="divide-y divide-slate-100 rounded-lg border border-slate-200 dark:divide-slate-800 dark:border-slate-700">
        {(data ?? []).map((d: SyncDevice) => (
          <li key={d.id} className="flex flex-wrap items-center gap-3 px-3 py-2 text-sm">
            <span className="relative text-slate-400">
              {d.platform === "android" || d.platform === "ios" ? <Smartphone size={16} /> : <Laptop size={16} />}
              <span
                className={cx("absolute -right-0.5 -bottom-0.5 h-2 w-2 rounded-full ring-2 ring-white dark:ring-slate-900", d.online ? "bg-emerald-500" : "bg-slate-300 dark:bg-slate-600")}
                title={d.online ? "Online" : "Offline"}
              />
            </span>
            <div className="min-w-0 flex-1">
              <div className="truncate font-medium">
                {d.name} {d.current && <span className="ml-1 rounded bg-indigo-50 px-1.5 py-0.5 text-xs text-indigo-700 dark:bg-indigo-950 dark:text-indigo-300">This device</span>}
                {d.paused && <span className="ml-1 rounded bg-amber-50 px-1.5 py-0.5 text-xs text-amber-700 dark:bg-amber-950 dark:text-amber-300">Tracking paused</span>}
                {d.syncDisabled && <span className="ml-1 rounded bg-slate-100 px-1.5 py-0.5 text-xs text-slate-600 dark:bg-slate-800 dark:text-slate-300">Sync off</span>}
              </div>
              <div className="text-xs text-slate-500">
                {d.platform} · FocusTrack {d.appVersion} · {d.online ? "online" : `last synced ${since(d.lastSeen)}`}
                {d.lastActivity && ` · last activity ${since(d.lastActivity)}`}
              </div>
              {d.syncError && <div className="text-xs text-amber-600">Sync problem: {d.syncError}</div>}
            </div>
            {!d.current && (
              <div className="flex flex-wrap gap-2">
                <Button
                  onClick={() =>
                    api
                      .syncDeviceControl(d.id, !d.paused, null)
                      .then(reload)
                      .catch((e) => onError(String(e)))
                  }
                  title="Applies the next time that device syncs"
                >
                  {d.paused ? "Resume tracking" : "Pause tracking"}
                </Button>
                {!d.syncDisabled && (
                  <ConfirmButton
                    label="Turn sync off"
                    confirmLabel="Turn off (undo on that device)"
                    onConfirm={() =>
                      api
                        .syncDeviceControl(d.id, null, false)
                        .then(reload)
                        .catch((e) => onError(String(e)))
                    }
                  />
                )}
                <ConfirmButton
                  label="Delete data"
                  confirmLabel="Delete its history"
                  onConfirm={() =>
                    api
                      .syncDeleteHistory(null, d.id)
                      .then(reload)
                      .catch((e) => onError(String(e)))
                  }
                />
                <ConfirmButton
                  label="Remove"
                  confirmLabel="Remove (its history stays)"
                  onConfirm={() =>
                    api
                      .syncRemoveDevice(d.id)
                      .then(reload)
                      .catch((e) => onError(String(e)))
                  }
                />
              </div>
            )}
          </li>
        ))}
        {data && data.length === 0 && <li className="px-3 py-2 text-sm text-slate-500">No devices yet.</li>}
      </ul>
      <p className="mt-1.5 text-xs text-slate-500">
        Changes to another device apply the next time it syncs. Removing a device signs it out; its history stays
        until you delete it with Delete data.
      </p>
    </div>
  );
}

const RETENTION: { days: number | null; label: string }[] = [
  { days: null, label: "Forever" },
  { days: 730, label: "2 years" },
  { days: 365, label: "1 year" },
  { days: 180, label: "6 months" },
  { days: 90, label: "3 months" },
  { days: 30, label: "30 days" },
];

const DELETE_RANGES: { value: string; label: string; days: number | null }[] = [
  { value: "all", label: "Everything", days: null },
  { value: "30", label: "Older than 30 days", days: 30 },
  { value: "90", label: "Older than 3 months", days: 90 },
  { value: "365", label: "Older than 1 year", days: 365 },
];

function daysAgo(n: number): string {
  const d = new Date(Date.now() - n * 86_400_000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

/** Retention, export, deleting history and the account (#67). */
function CloudData({ onDone }: { onDone: () => void }) {
  const retention = useLoad(() => api.syncRetention(), []);
  const [range, setRange] = useState("all");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = async (what: string, fn: () => Promise<string | void>) => {
    setBusy(what);
    setError(null);
    setNote(null);
    try {
      const msg = await fn();
      if (msg) setNote(msg);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="space-y-3 border-t border-slate-100 pt-3 text-sm dark:border-slate-800">
      <div className="font-medium">Your synced data</div>
      <ErrorNote error={error} />
      {note && <div className="rounded-lg bg-emerald-50 px-3 py-2 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-200">{note}</div>}

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="max-w-md">
          <div>Keep synced history</div>
          <div className="text-xs text-slate-500">
            Older history is deleted from the server automatically. History already on your devices isn&apos;t affected.
          </div>
        </div>
        <select
          value={retention.data === null ? "" : String(retention.data)}
          disabled={!!busy || retention.loading}
          onChange={(e) => {
            const days = e.target.value === "" ? null : Number(e.target.value);
            run("retention", async () => {
              await api.syncSetRetention(days);
              retention.reload();
              return days
                ? `History older than ${RETENTION.find((r) => r.days === days)?.label} is now deleted from the server.`
                : "Synced history is kept until you delete it.";
            });
          }}
          className={cx(inline, "w-36")}
          aria-label="Keep synced history"
        >
          {RETENTION.map((r) => (
            <option key={r.label} value={r.days ?? ""}>
              {r.label}
            </option>
          ))}
        </select>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="max-w-md">
          <div>Export</div>
          <div className="text-xs text-slate-500">Download everything in your account, decrypted, as a JSON file.</div>
        </div>
        <Button disabled={!!busy} onClick={() => run("export", async () => `Saved to ${await api.syncExport()}`)}>
          {busy === "export" ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />} Export
        </Button>
      </div>

      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="max-w-md">
          <div>Delete synced history</div>
          <div className="text-xs text-slate-500">Removes it from the server and from all your devices. Limits, goals and settings stay.</div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <select value={range} onChange={(e) => setRange(e.target.value)} className={cx(inline, "w-44")} aria-label="What to delete">
            {DELETE_RANGES.map((r) => (
              <option key={r.value} value={r.value}>
                {r.label}
              </option>
            ))}
          </select>
          <ConfirmButton
            label="Delete…"
            confirmLabel="Delete from all devices"
            onConfirm={() => {
              const r = DELETE_RANGES.find((x) => x.value === range)!;
              run("delete", async () => {
                const n = await api.syncDeleteHistory(r.days === null ? null : daysAgo(r.days), null);
                onDone();
                return `Deleted ${n} record${n === 1 ? "" : "s"} from the server. Your other devices remove their copies when they next sync.`;
              });
            }}
          />
        </div>
      </div>

      <div className="rounded-lg border border-rose-200 p-3 dark:border-rose-900">
        <div className="flex items-center gap-2 font-medium text-rose-700 dark:text-rose-300">
          <Trash2 size={14} /> Delete account
        </div>
        <p className="mt-1 text-xs text-slate-500">
          Deletes your account and everything stored in it, and signs out all your devices. History already on each device
          stays there until you clear it under Privacy &amp; data.
        </p>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <input
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder="Password"
            autoComplete="current-password"
            className={cx(inline, "w-48")}
            aria-label="Password to delete the account"
          />
          {password && (
            <ConfirmButton
              label="Delete account…"
              confirmLabel="Delete my account"
              onConfirm={() =>
                run("account", async () => {
                  await api.syncDeleteAccount(password);
                  setPassword("");
                  onDone();
                })
              }
            />
          )}
        </div>
      </div>
    </div>
  );
}

/** Settings → Account & Sync (#59). */
export function SyncSettings() {
  const { data: s, reload } = useLoad(() => api.syncStatus(), [], 3000);
  const [recoveryKey, setRecoveryKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);

  const act = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
    reload();
  };

  return (
    <Card title="Account & Sync" className="xl:col-span-2">
      {!s ? null : recoveryKey ? (
        <RecoveryKey value={recoveryKey} onDone={() => setRecoveryKey(null)} />
      ) : !s.signedIn || s.state === "signed_out" ? (
        <div className="grid gap-6 md:grid-cols-2">
          <div className="space-y-3 text-sm text-slate-600 dark:text-slate-300">
            <div className="flex items-center gap-2 font-medium text-slate-900 dark:text-slate-100">
              <CloudOff size={16} /> Sync is off
            </div>
            {s.state === "signed_out" && <ErrorNote error={s.lastError} />}
            <p>
              Sign in to see your screen time from all your devices in one place. Each device keeps tracking on its own
              and syncs about once a minute when it's online.
            </p>
            <div className="flex gap-2.5 rounded-lg bg-slate-50 p-3 dark:bg-slate-800/50">
              <LockKeyhole size={16} className="mt-0.5 shrink-0 text-emerald-600" />
              <div>
                <b>End-to-end encrypted.</b> Your devices encrypt everything before it leaves this computer; the server
                can't read it. Synced: app names, website domains (never full addresses), times and durations, focus
                sessions, limits, goals, blocking profiles and the categories you pick. Never: keystrokes, window contents, notes or private browsing.
              </div>
            </div>
          </div>
          <SignInForm
            status={s}
            onSignedIn={(rk) => {
              setRecoveryKey(rk);
              reload();
            }}
          />
        </div>
      ) : (
        <div className="space-y-4">
          <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
            <div className="flex items-center gap-2">
              <Cloud size={18} className="text-indigo-600" />
              <div>
                <div className="text-sm font-medium">{s.email}</div>
                <div className="text-xs">
                  <StatusLine s={s} />
                </div>
              </div>
            </div>
            <div className="ml-auto flex items-center gap-3">
              <span className="text-sm text-slate-500">Sync this device</span>
              <Toggle checked={s.enabled} label="Sync this device" onChange={(v) => act(() => api.syncSetEnabled(v))} />
              <Button onClick={() => act(() => api.syncNow())} disabled={!s.enabled}>
                <RefreshCw size={14} /> Sync now
              </Button>
            </div>
          </div>
          <ErrorNote error={error} />
          <div className="flex flex-wrap items-center gap-2 text-sm">
            <span className="text-slate-500">This device is called</span>
            {renaming === null ? (
              <>
                <b>{s.deviceName}</b>
                <button className="text-indigo-600 hover:underline" onClick={() => setRenaming(s.deviceName)}>
                  Rename
                </button>
              </>
            ) : (
              <form
                className="flex items-center gap-2"
                onSubmit={(e) => {
                  e.preventDefault();
                  act(() => api.syncRenameDevice(renaming)).then(() => setRenaming(null));
                }}
              >
                <input value={renaming} maxLength={40} onChange={(e) => setRenaming(e.target.value)} className={cx(field, "w-48")} autoFocus />
                <Button variant="primary" onClick={() => act(() => api.syncRenameDevice(renaming)).then(() => setRenaming(null))}>
                  Save
                </Button>
                <Button onClick={() => setRenaming(null)}>Cancel</Button>
              </form>
            )}
          </div>
          <Devices onError={setError} />
          <CloudData onDone={reload} />
          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-slate-100 pt-3 text-sm dark:border-slate-800">
            <span className="max-w-xl text-xs text-slate-500">
              Signing out keeps this device's own history and tracking. Data from your other devices is removed from this
              computer (it stays in your account).
            </span>
            <ConfirmButton label="Sign out" confirmLabel="Yes, sign out" onConfirm={() => act(() => api.syncSignOut())} />
          </div>
        </div>
      )}
    </Card>
  );
}
