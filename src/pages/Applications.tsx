import { useState } from "react";
import { Check, Pencil, Search, X } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { fmtDuration, rangeFor, type RangeKey } from "../lib/format";
import type { AppUsage, Category } from "../lib/types";
import { AppBadge, Bar, Card, Empty, ErrorNote, PageHeader, RangePicker } from "../components/ui";
import { DeviceParts, DevicePicker, hasSeveral, useDeviceFilter } from "../components/Devices";

function NameCell({ app, onSaved }: { app: AppUsage; onSaved: () => void }) {
  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(app.name);
  const save = async () => {
    if (name.trim() && name.trim() !== app.name) {
      await api.renameApp(app.id, name.trim());
      onSaved();
    }
    setEditing(false);
  };
  if (editing) {
    return (
      <div className="flex items-center gap-1">
        <input
          autoFocus
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") save();
            if (e.key === "Escape") setEditing(false);
          }}
          className="w-full rounded-md border border-slate-300 bg-white px-2 py-1 text-sm dark:border-slate-600 dark:bg-slate-800"
        />
        <button onClick={save} className="rounded p-1 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label="Save name">
          <Check size={15} />
        </button>
        <button onClick={() => setEditing(false)} className="rounded p-1 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label="Cancel">
          <X size={15} />
        </button>
      </div>
    );
  }
  return (
    <div className="group flex min-w-0 items-center gap-1">
      <div className="min-w-0">
        <div className="truncate font-medium">{app.name}</div>
        <div className="truncate text-xs text-slate-400">
          {app.members.length > 1 ? `${app.exeName} and ${app.members.length - 1} more (other platforms)` : app.exeName}
        </div>
        <DeviceParts parts={app.devices} />
      </div>
      {app.members.length === 1 && (
      <button
        onClick={() => {
          setName(app.name);
          setEditing(true);
        }}
        className="rounded p-1 text-slate-400 opacity-0 hover:bg-slate-100 group-hover:opacity-100 dark:hover:bg-slate-800"
        aria-label={`Rename ${app.name}`}
      >
        <Pencil size={13} />
      </button>
      )}
    </div>
  );
}

function CategorySelect({ app, categories, onChange }: { app: AppUsage; categories: Category[]; onChange: () => void }) {
  return (
    <select
      value={app.categoryId ?? ""}
      onChange={async (e) => {
        // The same app on every platform gets the category.
        for (const id of app.members) await api.setAppCategory(id, e.target.value ? Number(e.target.value) : null);
        onChange();
      }}
      className="rounded-md border border-slate-200 bg-white px-2 py-1 text-sm dark:border-slate-700 dark:bg-slate-900"
      style={{ borderLeft: `4px solid ${app.color}` }}
      aria-label={`Category for ${app.name}`}
    >
      {categories.map((c) => (
        <option key={c.id} value={c.id}>
          {c.name}
        </option>
      ))}
    </select>
  );
}

export default function Applications() {
  const [range, setRange] = useState<RangeKey>("today");
  const [query, setQuery] = useState("");
  const { from, to } = rangeFor(range);
  const [device, setDevice] = useDeviceFilter();
  const devices = useLoad(() => api.devices(from, to), [from, to]);
  const several = hasSeveral(devices.data ?? undefined);
  // With several devices: one row per app across platforms, each device's part under it (#63).
  const apps = useLoad(() => api.apps(from, to, several ? { combine: true, device } : {}), [from, to, several, device], 10000);
  const cats = useLoad(() => api.categories(), []);

  const q = query.trim().toLowerCase();
  const list = (apps.data ?? []).filter(
    (a) => (range === "30d" || a.secs > 0 || q) && (!q || `${a.name} ${a.exeName} ${a.category}`.toLowerCase().includes(q)),
  );
  const total = list.reduce((s, a) => s + a.secs, 0);
  const max = list[0]?.secs ?? 0;

  return (
    <>
      <PageHeader
        title="Applications"
        subtitle="Time in each app while it was in the foreground. Change a category to re-classify all of its history."
        actions={
          <>
            {devices.data && <DevicePicker devices={devices.data} value={device} onChange={setDevice} />}
            <RangePicker value={range} onChange={setRange} />
          </>
        }
      />
      <ErrorNote error={apps.error ?? cats.error} />
      <Card>
        <div className="mb-4 flex flex-wrap items-center gap-3">
          <label className="flex flex-1 items-center gap-2 rounded-lg border border-slate-200 px-3 py-1.5 dark:border-slate-700">
            <Search size={15} className="text-slate-400" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search applications or categories"
              className="w-full bg-transparent text-sm outline-none"
            />
          </label>
          <span className="tabular text-sm text-slate-500">
            {list.length} apps · {fmtDuration(total)}
          </span>
        </div>

        {list.length === 0 ? (
          <Empty>{apps.loading ? "Loading…" : "No application usage in this period."}</Empty>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-slate-200 text-left text-xs uppercase tracking-wide text-slate-500 dark:border-slate-800">
                  <th className="py-2 pr-3 font-medium">Application</th>
                  <th className="py-2 pr-3 font-medium">Category</th>
                  <th className="w-[28%] py-2 pr-3 font-medium">Share</th>
                  <th className="py-2 pr-3 text-right font-medium">Sessions</th>
                  <th className="py-2 text-right font-medium">Time</th>
                </tr>
              </thead>
              <tbody>
                {list.map((a) => (
                  <tr key={a.id} className="border-b border-slate-100 last:border-0 dark:border-slate-800/60">
                    <td className="max-w-[260px] py-2.5 pr-3">
                      <div className="flex items-center gap-3">
                        <AppBadge name={a.name} color={a.color} size={30} />
                        <NameCell app={a} onSaved={apps.reload} />
                      </div>
                    </td>
                    <td className="py-2.5 pr-3">
                      {cats.data && <CategorySelect app={a} categories={cats.data} onChange={apps.reload} />}
                    </td>
                    <td className="py-2.5 pr-3">
                      <div className="flex items-center gap-2">
                        <Bar value={a.secs} max={max} color={a.color} />
                        <span className="tabular w-10 text-right text-xs text-slate-500">
                          {total ? Math.round((a.secs / total) * 100) : 0}%
                        </span>
                      </div>
                    </td>
                    <td className="tabular py-2.5 pr-3 text-right text-slate-500">{a.sessions}</td>
                    <td className="tabular py-2.5 text-right font-medium">{fmtDuration(a.secs)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </>
  );
}
