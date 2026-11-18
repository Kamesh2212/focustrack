import type { AppUsage } from "../lib/types";
import { fmtDuration } from "../lib/format";
import { AppBadge, Bar, Empty } from "./ui";

export function AppList({ apps, limit, empty = "No app usage recorded yet." }: { apps: AppUsage[]; limit?: number; empty?: string }) {
  const shown = (limit ? apps.slice(0, limit) : apps).filter((a) => a.secs > 0);
  if (!shown.length) return <Empty>{empty}</Empty>;
  const max = shown[0].secs;
  return (
    <ul className="space-y-3">
      {shown.map((a) => (
        <li key={a.id} className="flex items-center gap-3">
          <AppBadge name={a.name} color={a.color} />
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline gap-2">
              <span className="truncate text-sm font-medium">{a.name}</span>
              <span className="truncate text-xs text-slate-400">{a.category}</span>
              <span className="tabular ml-auto shrink-0 text-sm text-slate-600 dark:text-slate-300">{fmtDuration(a.secs)}</span>
            </div>
            <div className="mt-1.5">
              <Bar value={a.secs} max={max} color={a.color} />
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}
