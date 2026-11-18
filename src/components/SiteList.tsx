import type { SiteUsage } from "../lib/types";
import { fmtDuration } from "../lib/format";
import { AppBadge, Bar, Empty } from "./ui";

export function SiteList({ sites, limit, empty = "No website usage recorded yet." }: { sites: SiteUsage[]; limit?: number; empty?: string }) {
  const shown = (limit ? sites.slice(0, limit) : sites).filter((s) => s.secs > 0);
  if (!shown.length) return <Empty>{empty}</Empty>;
  const max = shown[0].secs;
  return (
    <ul className="space-y-3">
      {shown.map((s) => (
        <li key={s.id} className="flex items-center gap-3">
          <AppBadge name={s.domain} color={s.color} />
          <div className="min-w-0 flex-1">
            <div className="flex items-baseline gap-2">
              <span className="truncate text-sm font-medium">{s.domain}</span>
              <span className="truncate text-xs text-slate-400">{s.category}</span>
              <span className="tabular ml-auto shrink-0 text-sm text-slate-600 dark:text-slate-300">{fmtDuration(s.secs)}</span>
            </div>
            <div className="mt-1.5">
              <Bar value={s.secs} max={max} color={s.color} />
            </div>
          </div>
        </li>
      ))}
    </ul>
  );
}
