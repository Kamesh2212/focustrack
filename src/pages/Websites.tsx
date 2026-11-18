import { useState } from "react";
import { Search } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { fmtDuration, rangeFor, type RangeKey } from "../lib/format";
import type { Category, SiteUsage } from "../lib/types";
import { AppBadge, Bar, Card, Empty, ErrorNote, PageHeader, RangePicker } from "../components/ui";
import { CategoryDonut } from "../components/charts";
import { ExtensionStatus, useBrowserStatus } from "../components/ExtensionStatus";

function CategorySelect({ site, categories, onChange }: { site: SiteUsage; categories: Category[]; onChange: () => void }) {
  return (
    <select
      value={site.categoryId ?? ""}
      onChange={async (e) => {
        await api.setWebsiteCategory(site.id, e.target.value ? Number(e.target.value) : null);
        onChange();
      }}
      className="rounded-md border border-slate-200 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
      style={{ borderLeft: `4px solid ${site.color}` }}
      aria-label={`Category for ${site.domain}`}
    >
      {categories.map((c) => (
        <option key={c.id} value={c.id}>
          {c.name}
        </option>
      ))}
    </select>
  );
}

export default function Websites() {
  const [range, setRange] = useState<RangeKey>("today");
  const [query, setQuery] = useState("");
  const { from, to } = rangeFor(range);
  const sites = useLoad(() => api.websites(from, to), [from, to], 10000);
  const cats = useLoad(() => api.categories(), []);
  const browser = useBrowserStatus();

  const q = query.trim().toLowerCase();
  const list = (sites.data ?? []).filter(
    (s) => (s.secs > 0 || q) && (!q || `${s.domain} ${s.category}`.toLowerCase().includes(q)),
  );
  const total = list.reduce((s, x) => s + x.secs, 0);
  const max = list[0]?.secs ?? 0;

  const byCategory = Object.values(
    list.reduce<Record<string, { name: string; color: string; kind: SiteUsage["kind"]; secs: number }>>((acc, s) => {
      const c = (acc[s.category] ??= { name: s.category, color: s.color, kind: s.kind, secs: 0 });
      c.secs += s.secs;
      return acc;
    }, {}),
  ).sort((a, b) => b.secs - a.secs);

  return (
    <>
      <PageHeader
        title="Websites"
        subtitle="Time on each website while its tab was active and the browser was in front. Domains only, never page content."
        actions={<RangePicker value={range} onChange={setRange} />}
      />
      <ErrorNote error={sites.error ?? cats.error} />
      <div className="mb-4">
        <ExtensionStatus status={browser.data} />
      </div>

      <div className="grid gap-4 lg:grid-cols-3">
        <Card className="lg:col-span-2">
          <div className="mb-4 flex flex-wrap items-center gap-3">
            <label className="flex flex-1 items-center gap-2 rounded-lg border border-slate-200 px-3 py-1.5 dark:border-slate-700">
              <Search size={15} className="text-slate-400" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="Search websites or categories"
                className="w-full bg-transparent text-sm outline-none"
              />
            </label>
            <span className="tabular text-sm text-slate-500">
              {list.length} sites · {fmtDuration(total)}
            </span>
          </div>
          {list.length === 0 ? (
            <Empty>{sites.loading ? "Loading…" : "No website usage in this period."}</Empty>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-slate-200 text-left text-xs uppercase tracking-wide text-slate-500 dark:border-slate-800">
                    <th className="py-2 pr-3 font-medium">Website</th>
                    <th className="py-2 pr-3 font-medium">Category</th>
                    <th className="w-[26%] py-2 pr-3 font-medium">Share</th>
                    <th className="py-2 pr-3 text-right font-medium">Visits</th>
                    <th className="py-2 text-right font-medium">Time</th>
                  </tr>
                </thead>
                <tbody>
                  {list.map((s) => (
                    <tr key={s.id} className="border-b border-slate-100 last:border-0 dark:border-slate-800/60">
                      <td className="max-w-[240px] py-2.5 pr-3">
                        <div className="flex items-center gap-3">
                          <AppBadge name={s.domain} color={s.color} size={30} />
                          <span className="truncate font-medium">{s.domain}</span>
                        </div>
                      </td>
                      <td className="py-2.5 pr-3">
                        {cats.data && <CategorySelect site={s} categories={cats.data} onChange={sites.reload} />}
                      </td>
                      <td className="py-2.5 pr-3">
                        <div className="flex items-center gap-2">
                          <Bar value={s.secs} max={max} color={s.color} />
                          <span className="tabular w-10 text-right text-xs text-slate-500">
                            {total ? Math.round((s.secs / total) * 100) : 0}%
                          </span>
                        </div>
                      </td>
                      <td className="tabular py-2.5 pr-3 text-right text-slate-500">{s.sessions}</td>
                      <td className="tabular py-2.5 text-right font-medium">{fmtDuration(s.secs)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Card>
        <Card title="Website categories">
          <CategoryDonut categories={byCategory} />
        </Card>
      </div>
    </>
  );
}
