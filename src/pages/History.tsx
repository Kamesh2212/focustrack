import { useState } from "react";
import { Download, Search } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { addDays, clock, dayLabel, fmtDuration, today } from "../lib/format";
import type { SessionRow } from "../lib/types";
import { Button, Card, DateInput, Empty, ErrorNote, PageHeader } from "../components/ui";

function groupByDay(rows: SessionRow[]): [string, SessionRow[]][] {
  const map = new Map<string, SessionRow[]>();
  for (const r of rows) map.set(r.day, [...(map.get(r.day) ?? []), r]);
  return [...map.entries()];
}

export default function History() {
  const [from, setFrom] = useState(addDays(today(), -6));
  const [to, setTo] = useState(today());
  const [query, setQuery] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const { data, error, loading } = useLoad(() => api.history(from, to, query), [from, to, query]);

  const exportCsv = async () => {
    try {
      const path = await api.exportCsv(from, to, query);
      setNotice(`Exported to ${path}`);
    } catch (e) {
      setNotice(`Export failed: ${e}`);
    }
  };

  return (
    <>
      <PageHeader
        title="History"
        subtitle="Every recorded application session. Filter by date, app or category."
        actions={
          <Button variant="primary" onClick={exportCsv}>
            <Download size={15} /> Export CSV
          </Button>
        }
      />
      <ErrorNote error={error} />
      {notice && (
        <div className="mb-4 rounded-lg border border-indigo-200 bg-indigo-50 px-4 py-2.5 text-sm text-indigo-800 dark:border-indigo-900 dark:bg-indigo-950 dark:text-indigo-200">
          {notice}
        </div>
      )}
      <Card>
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-1.5 text-sm">
            <DateInput value={from} max={to} onChange={setFrom} />
            <span className="text-slate-400">to</span>
            <DateInput value={to} max={today()} onChange={setTo} />
          </div>
          <label className="flex min-w-[220px] flex-1 items-center gap-2 rounded-lg border border-slate-200 px-3 py-1.5 dark:border-slate-700">
            <Search size={15} className="text-slate-400" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter by application or category"
              className="w-full bg-transparent text-sm outline-none"
            />
          </label>
        </div>

        {!data?.length ? (
          <Empty>{loading ? "Loading…" : "No sessions match these filters."}</Empty>
        ) : (
          <div className="space-y-6">
            {groupByDay(data).map(([day, rows]) => (
              <div key={day}>
                <div className="mb-2 flex items-baseline justify-between border-b border-slate-200 pb-1.5 dark:border-slate-800">
                  <h3 className="text-sm font-semibold">
                    {new Date(day + "T00:00").toLocaleDateString(undefined, { weekday: "long" })}, {dayLabel(day)}
                  </h3>
                  <span className="tabular text-xs text-slate-500">
                    {fmtDuration(rows.reduce((s, r) => s + r.secs, 0))} · {rows.length} sessions
                  </span>
                </div>
                <ul className="divide-y divide-slate-100 dark:divide-slate-800/60">
                  {rows.map((r) => (
                    <li key={r.id} className="flex items-center gap-3 py-1.5 text-sm">
                      <span className="tabular w-24 shrink-0 text-slate-500">
                        {clock(r.start)}–{clock(r.end)}
                      </span>
                      <span className="h-2 w-2 shrink-0 rounded-full" style={{ background: r.color }} />
                      <span className="truncate font-medium">{r.appName}</span>
                      <span className="hidden truncate text-xs text-slate-400 sm:inline">{r.category}</span>
                      <span className="tabular ml-auto shrink-0">{fmtDuration(r.secs)}</span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
            {data.length >= 500 && <p className="text-center text-xs text-slate-500">Showing the latest 500 sessions. Narrow the dates to see more.</p>}
          </div>
        )}
      </Card>
    </>
  );
}
