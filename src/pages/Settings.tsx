import { useState, type ReactNode } from "react";
import { ShieldCheck } from "lucide-react";
import { api, isTauri } from "../lib/api";
import { useLoad } from "../lib/hooks";
import type { Settings as SettingsT, Theme } from "../lib/types";
import { Button, Card, ConfirmButton, ErrorNote, PageHeader, Segmented, Toggle } from "../components/ui";

const IDLE_OPTIONS = [60, 120, 180, 300, 600, 900, 1800];
const RETENTION_OPTIONS = [30, 90, 180, 365];

function Row({ title, desc, children }: { title: string; desc?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-4 border-b border-slate-100 py-3.5 last:border-0 dark:border-slate-800/60">
      <div className="min-w-0 max-w-xl">
        <div className="text-sm font-medium">{title}</div>
        {desc && <div className="mt-0.5 text-xs text-slate-500 dark:text-slate-400">{desc}</div>}
      </div>
      {children}
    </div>
  );
}

export default function Settings({ onThemeChange }: { onThemeChange: (t: Theme) => void }) {
  const { data, error, reload } = useLoad(() => api.settings(), []);
  const [retention, setRetention] = useState(90);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const run = async (fn: () => Promise<unknown>, ok?: string) => {
    setActionError(null);
    try {
      await fn();
      if (ok) setNotice(ok);
      reload();
    } catch (e) {
      setActionError(String(e));
    }
  };

  if (!data) return <ErrorNote error={error} />;
  const s: SettingsT = data;

  return (
    <>
      <PageHeader title="Settings" subtitle={!isTauri ? "Browser preview — changes use sample data only." : undefined} />
      <ErrorNote error={error ?? actionError} />
      {notice && (
        <div className="mb-4 rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-2.5 text-sm text-emerald-800 dark:border-emerald-900 dark:bg-emerald-950 dark:text-emerald-200">
          {notice}
        </div>
      )}

      <div className="grid gap-4 xl:grid-cols-2">
        <Card title="Tracking">
          <Row title="Track my computer usage" desc="When paused, nothing is recorded until you resume. Also available from the tray icon.">
            <Toggle label="Tracking enabled" checked={!s.paused} onChange={(on) => run(() => api.setPaused(!on))} />
          </Row>
          <Row
            title="Idle after"
            desc="With no keyboard or mouse input for this long, time counts as idle instead of active."
          >
            <select
              value={s.idleThresholdSecs}
              onChange={(e) => run(() => api.setIdleThreshold(Number(e.target.value)))}
              className="rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
            >
              {IDLE_OPTIONS.map((v) => (
                <option key={v} value={v}>
                  {v / 60} minute{v === 60 ? "" : "s"}
                </option>
              ))}
            </select>
          </Row>
          <Row title="Start with Windows" desc="Launches minimised to the tray when you sign in, so tracking is never missed.">
            <Toggle label="Start with Windows" checked={s.autostart} onChange={(on) => run(() => api.setAutostart(on))} />
          </Row>
        </Card>

        <Card title="Appearance">
          <Row title="Theme">
            <Segmented<Theme>
              value={s.theme}
              onChange={(t) => {
                onThemeChange(t);
                run(() => api.setTheme(t));
              }}
              options={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
            />
          </Row>
        </Card>

        <Card title="Privacy & data" className="xl:col-span-2">
          <div className="mb-2 flex gap-3 rounded-lg bg-slate-50 p-3 text-sm text-slate-600 dark:bg-slate-800/50 dark:text-slate-300">
            <ShieldCheck size={18} className="mt-0.5 shrink-0 text-emerald-600" />
            <div>
              FocusTrack records only <b>which application is in the foreground</b> and <b>when</b>, plus whether
              you are idle. It never records keystrokes, window contents, screenshots or passwords, and all data stays
              on this computer.
            </div>
          </div>
          <Row title="Data location" desc={<code className="break-all">{s.dataDir}</code>}>
            <span />
          </Row>
          <Row title="Delete old history" desc="Remove usage records older than the selected age.">
            <div className="flex items-center gap-2">
              <select
                value={retention}
                onChange={(e) => setRetention(Number(e.target.value))}
                className="rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
              >
                {RETENTION_OPTIONS.map((d) => (
                  <option key={d} value={d}>
                    Older than {d} days
                  </option>
                ))}
              </select>
              <Button
                onClick={() =>
                  run(async () => {
                    const n = await api.deleteOlderThan(retention);
                    setNotice(`Deleted ${n} record${n === 1 ? "" : "s"} older than ${retention} days.`);
                  })
                }
              >
                Delete
              </Button>
            </div>
          </Row>
          <Row title="Clear all usage history" desc="Permanently deletes every recorded session. App names and categories are kept.">
            <ConfirmButton
              label="Clear history…"
              confirmLabel="Yes, delete everything"
              onConfirm={() => run(() => api.deleteAllData(), "All usage history was deleted.")}
            />
          </Row>
        </Card>
      </div>
    </>
  );
}
