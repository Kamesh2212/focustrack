import { useState } from "react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { dayLabel, fmtDuration, pctChange, rangeFor, today, type RangeKey } from "../lib/format";
import { Card, DateInput, ErrorNote, PageHeader, RangePicker, Stat } from "../components/ui";
import { CategoryDonut, DailyChart, Heatmap, HourlyChart } from "../components/charts";
import { AppList } from "../components/AppList";

export default function Reports() {
  const [range, setRange] = useState<RangeKey>("7d");
  const [custom, setCustom] = useState(rangeFor("7d"));
  const { from, to } = rangeFor(range, custom);
  const { data: r, error } = useLoad(() => api.report(from, to), [from, to]);

  const change = r ? pctChange(r.activeSecs, r.prevActiveSecs) : null;
  const productivePct = r && r.activeSecs ? Math.round((r.productiveSecs / r.activeSecs) * 100) : 0;
  const multiDay = from !== to;

  return (
    <>
      <PageHeader
        title="Reports"
        subtitle={multiDay ? `${dayLabel(from)} – ${dayLabel(to)}` : dayLabel(from)}
        actions={
          <>
            <RangePicker value={range} onChange={setRange} keys={["today", "yesterday", "7d", "30d", "custom"]} />
            {range === "custom" && (
              <div className="flex items-center gap-1.5 text-sm">
                <DateInput value={custom.from} max={custom.to} onChange={(v) => setCustom({ ...custom, from: v })} />
                <span className="text-slate-400">to</span>
                <DateInput value={custom.to} max={today()} onChange={(v) => setCustom({ ...custom, to: v })} />
              </div>
            )}
          </>
        }
      />
      <ErrorNote error={error} />
      {r && (
        <>
          <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
            <Stat
              label="Active time"
              value={fmtDuration(r.activeSecs)}
              hint={change === null ? "No previous period data" : `${change > 0 ? "▲" : change < 0 ? "▼" : ""} ${Math.abs(change)}% vs previous period`}
              tone={change === null ? "default" : change > 0 ? "bad" : "good"}
            />
            <Stat label="Idle time" value={fmtDuration(r.idleSecs)} hint={`Screen time ${fmtDuration(r.activeSecs + r.idleSecs)}`} />
            <Stat
              label="Daily average"
              value={fmtDuration(r.dailyAvgSecs)}
              hint={`Weekdays ${fmtDuration(r.weekdayAvgSecs)} · Weekends ${fmtDuration(r.weekendAvgSecs)}`}
            />
            <Stat
              label="Productive share"
              value={`${productivePct}%`}
              hint={r.peakHour === null ? "—" : `Most active ${String(r.peakHour).padStart(2, "0")}:00–${String(r.peakHour + 1).padStart(2, "0")}:00`}
            />
          </div>

          <div className="mt-4 grid gap-4 lg:grid-cols-3">
            <Card className="lg:col-span-2" title={multiDay ? "Daily usage" : "Usage by hour"}>
              {multiDay ? <DailyChart days={r.days} height={240} /> : <HourlyChart hourly={r.hourly} height={240} />}
            </Card>
            <Card title="Categories">
              <CategoryDonut categories={r.categories} />
            </Card>
          </div>

          <div className="mt-4 grid gap-4 lg:grid-cols-2">
            <Card title="Applications">
              <AppList apps={r.apps} limit={10} empty="No application usage in this period." />
            </Card>
            <div className="grid content-start gap-4">
              {multiDay && (
                <Card title="Activity by hour">
                  <HourlyChart hourly={r.hourly} height={150} />
                </Card>
              )}
              <Card title="Activity heatmap">
                <Heatmap grid={r.heatmap} />
              </Card>
            </div>
          </div>
        </>
      )}
    </>
  );
}
