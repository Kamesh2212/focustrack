import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import type { CategoryUsage, DayTotal } from "../lib/types";
import { dayLabel, fmtDuration, hourLabel, toHours, weekdayShort } from "../lib/format";
import { Empty } from "./ui";

const AXIS = { fontSize: 12, fill: "#94a3b8" };
const ACTIVE = "#6366f1";
const IDLE = "#cbd5e1";

function TooltipBox({ title, rows }: { title: string; rows: { label: string; value: string; color?: string }[] }) {
  return (
    <div className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-xs shadow-lg dark:border-slate-700 dark:bg-slate-900">
      <div className="mb-1 font-semibold">{title}</div>
      {rows.map((r) => (
        <div key={r.label} className="flex items-center gap-2">
          {r.color && <span className="h-2 w-2 rounded-full" style={{ background: r.color }} />}
          <span className="text-slate-500 dark:text-slate-400">{r.label}</span>
          <span className="tabular ml-auto pl-3 font-medium">{r.value}</span>
        </div>
      ))}
    </div>
  );
}

/** Stacked active/idle bars per day. */
export function DailyChart({ days, height = 220 }: { days: DayTotal[]; height?: number }) {
  const short = days.length <= 7;
  const data = days.map((d) => ({
    ...d,
    label: short ? weekdayShort(d.day) : dayLabel(d.day),
    active: toHours(d.activeSecs),
    idle: toHours(d.idleSecs),
  }));
  return (
    <ResponsiveContainer width="100%" height={height}>
      <BarChart data={data} margin={{ top: 4, right: 4, left: -16, bottom: 0 }}>
        <CartesianGrid vertical={false} strokeOpacity={0.15} />
        <XAxis dataKey="label" tick={AXIS} tickLine={false} axisLine={false} interval="preserveStartEnd" minTickGap={8} />
        <YAxis tick={AXIS} tickLine={false} axisLine={false} unit="h" allowDecimals={false} />
        <Tooltip
          cursor={{ fill: "rgba(148,163,184,0.12)" }}
          content={({ active, payload }) => {
            if (!active || !payload?.length) return null;
            const d = payload[0].payload as DayTotal;
            return (
              <TooltipBox
                title={dayLabel(d.day)}
                rows={[
                  { label: "Active", value: fmtDuration(d.activeSecs), color: ACTIVE },
                  { label: "Idle", value: fmtDuration(d.idleSecs), color: IDLE },
                ]}
              />
            );
          }}
        />
        <Bar dataKey="active" stackId="t" fill={ACTIVE} radius={[0, 0, 0, 0]} maxBarSize={36} />
        <Bar dataKey="idle" stackId="t" fill={IDLE} radius={[4, 4, 0, 0]} maxBarSize={36} />
      </BarChart>
    </ResponsiveContainer>
  );
}

/** Active minutes per hour of day. */
export function HourlyChart({ hourly, height = 160 }: { hourly: number[]; height?: number }) {
  const data = hourly.map((secs, h) => ({ h, label: hourLabel(h), mins: Math.round(secs / 60), secs }));
  return (
    <ResponsiveContainer width="100%" height={height}>
      <BarChart data={data} margin={{ top: 4, right: 4, left: -20, bottom: 0 }}>
        <XAxis dataKey="label" tick={AXIS} tickLine={false} axisLine={false} interval={2} />
        <YAxis tick={AXIS} tickLine={false} axisLine={false} unit="m" allowDecimals={false} />
        <Tooltip
          cursor={{ fill: "rgba(148,163,184,0.12)" }}
          content={({ active, payload }) => {
            if (!active || !payload?.length) return null;
            const d = payload[0].payload as { h: number; secs: number };
            return (
              <TooltipBox
                title={`${String(d.h).padStart(2, "0")}:00 – ${String(d.h + 1).padStart(2, "0")}:00`}
                rows={[{ label: "Active", value: fmtDuration(d.secs), color: ACTIVE }]}
              />
            );
          }}
        />
        <Bar dataKey="mins" fill={ACTIVE} radius={[3, 3, 0, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

export function CategoryDonut({ categories }: { categories: CategoryUsage[] }) {
  const total = categories.reduce((s, c) => s + c.secs, 0);
  if (!total) return <Empty>No categorised usage yet.</Empty>;
  return (
    <div className="flex flex-col items-center gap-4 sm:flex-row lg:flex-col xl:flex-row">
      <div className="relative h-40 w-40 shrink-0">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie data={categories} dataKey="secs" nameKey="name" innerRadius={50} outerRadius={75} paddingAngle={1.5} stroke="none">
              {categories.map((c) => (
                <Cell key={c.name} fill={c.color} />
              ))}
            </Pie>
            <Tooltip
              content={({ active, payload }) => {
                if (!active || !payload?.length) return null;
                const c = payload[0].payload as CategoryUsage;
                return <TooltipBox title={c.name} rows={[{ label: "Time", value: fmtDuration(c.secs), color: c.color }]} />;
              }}
            />
          </PieChart>
        </ResponsiveContainer>
        <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
          <span className="tabular text-lg font-semibold">{fmtDuration(total)}</span>
          <span className="text-[11px] text-slate-500">tracked</span>
        </div>
      </div>
      <ul className="w-full space-y-1.5 text-sm">
        {categories.map((c) => (
          <li key={c.name} className="flex items-center gap-2">
            <span className="h-2.5 w-2.5 shrink-0 rounded-sm" style={{ background: c.color }} />
            <span className="truncate">{c.name}</span>
            <span className="tabular ml-auto text-slate-500 dark:text-slate-400">
              {fmtDuration(c.secs)} · {Math.round((c.secs / total) * 100)}%
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** 7 x 24 grid of active time; darker = busier. */
export function Heatmap({ grid }: { grid: number[][] }) {
  const max = Math.max(1, ...grid.flat());
  return (
    <div className="overflow-x-auto">
      <div className="inline-grid min-w-full gap-[3px]" style={{ gridTemplateColumns: "36px repeat(24, minmax(14px, 1fr))" }}>
        <span />
        {Array.from({ length: 24 }, (_, h) => (
          <span key={h} className="text-center text-[10px] text-slate-400">
            {h % 3 === 0 ? hourLabel(h) : ""}
          </span>
        ))}
        {grid.map((row, wd) => (
          <div key={wd} className="contents">
            <span className="pr-1 text-right text-[11px] leading-[18px] text-slate-500">{WEEKDAYS[wd]}</span>
            {row.map((secs, h) => (
              <span
                key={h}
                title={`${WEEKDAYS[wd]} ${String(h).padStart(2, "0")}:00 · ${fmtDuration(secs)}`}
                className="h-[18px] rounded-[3px] bg-slate-100 dark:bg-slate-800"
                style={secs > 0 ? { background: ACTIVE, opacity: 0.15 + 0.85 * (secs / max) } : undefined}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
