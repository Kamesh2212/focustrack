import { useState } from "react";
import { Laptop, Smartphone } from "lucide-react";
import type { DeviceStats, DeviceTotal } from "../lib/types";
import { fmtDuration } from "../lib/format";
import { Card, cx } from "./ui";

const KEY = "ft-device-filter";
const MOBILE = ["android", "ios"];

/** The device filter, remembered on this computer. */
export function useDeviceFilter(): [string, (v: string) => void] {
  const [value, setValue] = useState<string>(() => {
    try {
      return localStorage.getItem(KEY) ?? "all";
    } catch {
      return "all";
    }
  });
  const set = (v: string) => {
    setValue(v);
    try {
      localStorage.setItem(KEY, v);
    } catch {
      /* not remembered; fine */
    }
  };
  return [value, set];
}

/** More than one device has ever contributed data. */
export function hasSeveral(devices: DeviceTotal[] | undefined): boolean {
  return (devices?.length ?? 0) > 1;
}

function ago(ts: string | null): string | null {
  if (!ts) return null;
  const ms = Date.now() - new Date(ts.replace(" ", "T")).getTime();
  if (Number.isNaN(ms) || ms < 0) return null;
  if (ms < 90_000) return "up to date";
  if (ms < 3_600_000) return `up to ${Math.round(ms / 60_000)} min ago`;
  if (ms < 86_400_000) return `up to ${Math.round(ms / 3_600_000)} h ago`;
  return `last data ${Math.round(ms / 86_400_000)} days ago`;
}

export function PlatformIcon({ platform, size = 16 }: { platform: string; size?: number }) {
  return MOBILE.includes(platform) ? <Smartphone size={size} /> : <Laptop size={size} />;
}

/** "All devices / Computers / Phones / each device" (only when there's more than one device). */
export function DevicePicker({ devices, value, onChange }: { devices: DeviceTotal[]; value: string; onChange: (v: string) => void }) {
  if (!hasSeveral(devices)) return null;
  const hasMobile = devices.some((d) => MOBILE.includes(d.platform));
  const known = value === "all" || value === "desktop" || value === "mobile" || devices.some((d) => d.id === value);
  return (
    <select
      value={known ? value : "all"}
      onChange={(e) => onChange(e.target.value)}
      aria-label="Show devices"
      className="rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-sm dark:border-slate-700 dark:bg-slate-900"
    >
      <option value="all">All devices</option>
      <option value="desktop">Computers</option>
      {hasMobile && <option value="mobile">Phones & tablets</option>}
      <optgroup label="One device">
        {devices.map((d) => (
          <option key={d.id} value={d.id}>
            {d.name}
            {d.current ? " (this device)" : ""}
          </option>
        ))}
      </optgroup>
    </select>
  );
}

/** "Home PC 2h · Pixel 8 30m" under an app or website (#63). */
export function DeviceParts({ parts }: { parts: { id: string; name: string; platform: string; secs: number }[] }) {
  if (parts.length < 2) return null;
  return (
    <div className="mt-0.5 flex flex-wrap gap-x-2.5 text-xs text-slate-500">
      {parts.map((p) => (
        <span key={p.id} className="tabular inline-flex items-center gap-1">
          <PlatformIcon platform={p.platform} size={11} />
          {p.name} {fmtDuration(p.secs)}
        </span>
      ))}
    </div>
  );
}

/** Time per device, with combined and unique totals (#64, #65). */
export function DeviceBreakdown({
  devices,
  uniqueSecs,
  onPick,
  title = "Devices",
}: {
  devices: DeviceTotal[];
  uniqueSecs: number;
  onPick?: (id: string) => void;
  title?: string;
}) {
  const combined = devices.reduce((s, d) => s + d.activeSecs, 0);
  const max = Math.max(1, ...devices.map((d) => d.activeSecs));
  if (!hasSeveral(devices)) {
    return (
      <Card title={title}>
        <p className="text-sm text-slate-500">
          Only this device so far. To see your other computers and phones here, sign in under{" "}
          <b>Settings → Account & Sync</b> on each of them.
        </p>
      </Card>
    );
  }
  return (
    <Card title={title}>
      <div className="mb-3 grid grid-cols-2 gap-3 text-sm">
        <div>
          <div className="text-xs uppercase tracking-wide text-slate-500">Combined</div>
          <div className="tabular text-lg font-semibold">{fmtDuration(combined)}</div>
          <div className="text-xs text-slate-500">every device added up</div>
        </div>
        <div>
          <div className="text-xs uppercase tracking-wide text-slate-500">Unique</div>
          <div className="tabular text-lg font-semibold">{fmtDuration(uniqueSecs)}</div>
          <div className="text-xs text-slate-500">time on two devices at once counted once</div>
        </div>
      </div>
      <ul className="space-y-2">
        {devices.map((d) => (
          <li key={d.id}>
            <button
              type="button"
              onClick={() => onPick?.(d.id)}
              disabled={!onPick}
              className="w-full rounded-lg px-1 py-1 text-left hover:bg-slate-50 disabled:hover:bg-transparent dark:hover:bg-slate-800/50"
              title={onPick ? `Show only ${d.name}` : undefined}
            >
              <div className="flex items-center gap-2 text-sm">
                <span className="text-slate-400">
                  <PlatformIcon platform={d.platform} />
                </span>
                <span className="truncate font-medium">{d.name}</span>
                {d.current && (
                  <span className="rounded bg-indigo-50 px-1.5 py-0.5 text-xs text-indigo-700 dark:bg-indigo-950 dark:text-indigo-300">This device</span>
                )}
                <span className="tabular ml-auto">{fmtDuration(d.activeSecs)}</span>
              </div>
              <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
                <div
                  className={cx("h-full rounded-full", MOBILE.includes(d.platform) ? "bg-emerald-500" : "bg-indigo-500")}
                  style={{ width: `${Math.round((d.activeSecs / max) * 100)}%` }}
                />
              </div>
              {!d.current && ago(d.lastActivity) && <div className="mt-0.5 text-xs text-slate-500">{ago(d.lastActivity)}</div>}
            </button>
          </li>
        ))}
      </ul>
    </Card>
  );
}

/** Devices side by side (#65): which one takes your time, and for what. */
export function DeviceComparison({ devices, onPick }: { devices: DeviceStats[]; onPick?: (id: string) => void }) {
  const most = devices[0];
  const sum = (mobile: boolean) => devices.filter((d) => MOBILE.includes(d.platform) === mobile).reduce((s, d) => s + d.activeSecs, 0);
  const phones = sum(true);
  const computers = sum(false);
  const leisure = [...devices].sort((a, b) => b.distractingSecs - a.distractingSecs)[0];
  const work = [...devices].sort((a, b) => b.productiveSecs - a.productiveSecs)[0];
  return (
    <Card title="Compare devices">
      <ul className="mb-3 space-y-1 text-sm text-slate-600 dark:text-slate-300">
        <li>
          Most used: <b>{most.name}</b> ({fmtDuration(most.activeSecs)})
        </li>
        {phones > 0 && computers > 0 && (
          <li>
            Computers {fmtDuration(computers)} · phones {fmtDuration(phones)}
          </li>
        )}
        {work && work.productiveSecs > 0 && (
          <li>
            Most productive time on <b>{work.name}</b>
            {leisure && leisure.distractingSecs > 0 && leisure.id !== work.id && (
              <>
                , most entertainment and social on <b>{leisure.name}</b>
              </>
            )}
          </li>
        )}
      </ul>
      <div className="overflow-x-auto">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-slate-200 text-left text-xs uppercase tracking-wide text-slate-500 dark:border-slate-800">
              <th className="py-2 pr-3 font-medium">Device</th>
              <th className="py-2 pr-3 text-right font-medium">Total</th>
              <th className="py-2 pr-3 text-right font-medium">Daily avg</th>
              <th className="py-2 pr-3 text-right font-medium">Weekday / weekend</th>
              <th className="w-[22%] py-2 pr-3 font-medium">Productive vs distracting</th>
              <th className="py-2 font-medium">Mostly</th>
            </tr>
          </thead>
          <tbody>
            {devices.map((d) => {
              const both = Math.max(1, d.productiveSecs + d.distractingSecs);
              return (
                <tr
                  key={d.id}
                  onClick={() => onPick?.(d.id)}
                  className={cx("border-b border-slate-100 last:border-0 dark:border-slate-800/60", onPick && "cursor-pointer hover:bg-slate-50 dark:hover:bg-slate-800/50")}
                  title={onPick ? `Show the report for ${d.name}` : undefined}
                >
                  <td className="py-2.5 pr-3">
                    <div className="flex items-center gap-2">
                      <span className="text-slate-400">
                        <PlatformIcon platform={d.platform} />
                      </span>
                      <span className="whitespace-nowrap font-medium">{d.name}</span>
                    </div>
                  </td>
                  <td className="tabular whitespace-nowrap py-2.5 pr-3 text-right font-medium">{fmtDuration(d.activeSecs)}</td>
                  <td className="tabular whitespace-nowrap py-2.5 pr-3 text-right">{fmtDuration(d.dailyAvgSecs)}</td>
                  <td className="tabular whitespace-nowrap py-2.5 pr-3 text-right text-slate-500">
                    {fmtDuration(d.weekdayAvgSecs)} / {fmtDuration(d.weekendAvgSecs)}
                  </td>
                  <td className="py-2.5 pr-3">
                    <div className="flex h-2 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800" title={`Productive ${fmtDuration(d.productiveSecs)} · distracting ${fmtDuration(d.distractingSecs)}`}>
                      <div className="bg-emerald-500" style={{ width: `${(d.productiveSecs / both) * 100}%` }} />
                      <div className="bg-rose-400" style={{ width: `${(d.distractingSecs / both) * 100}%` }} />
                    </div>
                  </td>
                  <td className="py-2.5 text-xs text-slate-500">{[d.topCategory, d.topApp].filter(Boolean).join(" · ") || "—"}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      <p className="mt-2 text-xs text-slate-500">Select a device for its full report.</p>
    </Card>
  );
}
