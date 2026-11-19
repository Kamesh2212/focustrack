import { useState } from "react";
import { Laptop, Smartphone } from "lucide-react";
import type { DeviceTotal } from "../lib/types";
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
