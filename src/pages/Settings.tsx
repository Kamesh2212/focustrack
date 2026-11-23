import { useState, type ReactNode } from "react";
import { Bell, ShieldAlert, ShieldCheck } from "lucide-react";
import { api, isTauri } from "../lib/api";
import { useLoad } from "../lib/hooks";
import type { Settings as SettingsT, Theme } from "../lib/types";
import { Button, Card, ConfirmButton, ErrorNote, PageHeader, Segmented, Toggle } from "../components/ui";
import { InstallSteps, browserLabel, StatusDot, useBrowserStatus } from "../components/ExtensionStatus";
import { SyncSettings } from "../components/SyncSettings";
import { PhoneAccessCard, PhoneBlockingCard } from "../components/PhoneAccess";
import { isPhone, usePlatform } from "../lib/platform";

const IDLE_OPTIONS = [60, 120, 180, 300, 600, 900, 1800];
const RETENTION_OPTIONS = [30, 90, 180, 365];
const BREAK_OPTIONS = [0, 45, 60, 90, 120, 180];

function ago(secs: number): string {
  if (secs < 60) return "just now";
  if (secs < 3600) return `${Math.round(secs / 60)} min ago`;
  return `${Math.round(secs / 3600)} h ago`;
}

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
  const browser = useBrowserStatus();
  const admin = useLoad(() => api.adminMode(), [], 15000);
  // Windows-only settings (tracker, start with Windows, administrator mode,
  // browser extension) don't apply on phones.
  const phone = isPhone(usePlatform());
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
        <PhoneAccessCard />
        <PhoneBlockingCard />
        {!phone && (
        <>
        <Card title="Tracking">
          <Row title="Track my computer usage" desc="When paused, nothing is recorded except apps and websites that have a usage limit — those keep counting and stay blocked when the limit is reached. Also available from the tray icon.">
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
          <Row
            title="Limit extensions per day"
            desc='How many times a day you can press "+15 min" on a blocked limit. Set to none to make limits strict.'
          >
            <select
              value={s.overridesPerDay}
              onChange={(e) => run(() => api.setOverridesPerDay(Number(e.target.value)))}
              className="rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
            >
              {[0, 1, 2, 3, 5].map((n) => (
                <option key={n} value={n}>
                  {n === 0 ? "None (strict)" : `${n} per day`}
                </option>
              ))}
            </select>
          </Row>
        </Card>

        <Card title="Administrator mode">
          <Row
            title={admin.data?.enabled ? (admin.data.elevated ? "On — FocusTrack is running as administrator" : "On — restart FocusTrack to apply") : "Off"}
            desc="Some games, like Genshin Impact, run as administrator. Windows doesn't let a normal app close them, so FocusTrack needs administrator rights to block them. Turning this on asks Windows for permission once and starts FocusTrack that way at sign-in."
          >
            {admin.data?.enabled ? (
              <Button onClick={() => run(() => api.disableAdminMode(), "Administrator mode turned off.")}>Turn off</Button>
            ) : (
              <Button variant="primary" onClick={() => run(() => api.enableAdminMode())}>
                <ShieldAlert size={15} /> Turn on
              </Button>
            )}
          </Row>
          {!!admin.data?.needsAdmin.length && !admin.data.elevated && (
            <p className="rounded-lg bg-amber-50 px-3 py-2 text-sm text-amber-800 dark:bg-amber-950/50 dark:text-amber-200">
              FocusTrack couldn't close {admin.data.needsAdmin.join(", ")} because{" "}
              {admin.data.needsAdmin.length === 1 ? "it runs" : "they run"} as administrator. Turn on Administrator mode to block{" "}
              {admin.data.needsAdmin.length === 1 ? "it" : "them"}.
            </p>
          )}
        </Card>
        </>
        )}

        <Card title="Notifications">
          <Row
            title={phone ? "Notifications" : "Desktop notifications"}
            desc={
              phone
                ? "Limit warnings, limits reached and goal reminders appear as phone notifications."
                : "Limit warnings, limits reached and break reminders appear as Windows notifications."
            }
          >
            <Toggle label="Notifications" checked={s.notificationsEnabled} onChange={(on) => run(() => api.setNotificationsEnabled(on))} />
          </Row>
          {!phone && (
          <Row title="Break reminder" desc="Remind me after this much continuous computer use. A 5-minute break resets it.">
            <select
              value={s.breakReminderMins}
              onChange={(e) => run(() => api.setBreakReminder(Number(e.target.value)))}
              className="rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
            >
              {BREAK_OPTIONS.map((m) => (
                <option key={m} value={m}>
                  {m === 0 ? "Off" : m < 60 ? `${m} minutes` : `${m / 60} hour${m === 60 ? "" : "s"}`}
                </option>
              ))}
            </select>
          </Row>
          )}
          <Row title="Goal reminder" desc="Once a day at this time, remind me about goals that aren't done yet.">
            <select
              value={s.goalReminder}
              onChange={(e) => run(() => api.setGoalReminder(e.target.value))}
              className="rounded-md border border-slate-200 bg-white px-2 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
            >
              <option value="">Off</option>
              {["17:00", "18:00", "19:00", "20:00", "21:00", "22:00"].map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </Row>
          <Row
            title="Test"
            desc={
              phone
                ? "Check that your phone shows FocusTrack notifications (Do Not Disturb can hide them)."
                : "Check that Windows shows FocusTrack notifications (Focus Assist / Do Not Disturb can hide them)."
            }
          >
            <Button onClick={() => run(() => api.testNotification(), "Test notification sent.")}>
              <Bell size={15} /> Send test
            </Button>
          </Row>
        </Card>

        {!phone && (
        <Card title="Website tracking">
          <Row title="Track websites" desc="Uses the FocusTrack browser extension. Only domains are recorded; private windows never are.">
            <Toggle label="Track websites" checked={s.websiteTracking} onChange={(on) => run(() => api.setWebsiteTracking(on))} />
          </Row>
          <Row
            title="Browser extensions"
            desc={
              browser.data && !browser.data.serverRunning
                ? `Not available: port ${browser.data.port} is in use by another program.`
                : browser.data?.browsers.length
                  ? undefined
                  : "No browser has connected yet."
            }
          >
            <div className="flex flex-col items-end gap-1 text-sm">
              {browser.data?.browsers.map((b) => (
                <span key={b.browser} className="flex items-center gap-2">
                  <StatusDot on={b.connected} />
                  {browserLabel(b.browser)}
                  <span className="text-xs text-slate-500">{ago(b.lastSeenSecs)}</span>
                </span>
              ))}
            </div>
          </Row>
          {!browser.data?.browsers.some((b) => b.connected) && (
            <div className="pt-3">
              <InstallSteps />
            </div>
          )}
        </Card>
        )}

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

        <SyncSettings />

        <Card title="Privacy & data" className="xl:col-span-2">
          <div className="mb-2 flex gap-3 rounded-lg bg-slate-50 p-3 text-sm text-slate-600 dark:bg-slate-800/50 dark:text-slate-300">
            <ShieldCheck size={18} className="mt-0.5 shrink-0 text-emerald-600" />
            <div>
              FocusTrack records only <b>which application is in the foreground</b>, the <b>domain</b> of your active
              browser tab (with the extension), <b>when</b>, and whether you are idle. It never records keystrokes, window contents, screenshots or passwords, and all data stays
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
