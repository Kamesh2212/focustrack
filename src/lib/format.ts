/** 8100 -> "2h 15m", 2400 -> "40m", 42 -> "42s" */
export function fmtDuration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

/** Hours with one decimal, for chart axes. */
export function toHours(secs: number): number {
  return Math.round((secs / 3600) * 10) / 10;
}

/** Percentage change, or null when there is no baseline. */
export function pctChange(current: number, previous: number): number | null {
  if (previous <= 0) return null;
  return Math.round(((current - previous) / previous) * 100);
}

export function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export function toDayString(d: Date): string {
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export function addDays(day: string, n: number): string {
  const [y, m, d] = day.split("-").map(Number);
  return toDayString(new Date(y, m - 1, d + n));
}

export function today(): string {
  return toDayString(new Date());
}

/** "2026-11-18" -> "Wed" */
export function weekdayShort(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, { weekday: "short" });
}

/** "2026-11-18" -> "18 Nov" */
export function dayLabel(day: string): string {
  const [y, m, d] = day.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(undefined, { day: "numeric", month: "short" });
}

/** "2026-11-18 09:05:00" -> "09:05" */
export function clock(ts: string): string {
  return ts.slice(11, 16);
}

export function hourLabel(h: number): string {
  if (h === 0) return "12a";
  if (h === 12) return "12p";
  return h < 12 ? `${h}a` : `${h - 12}p`;
}

export type RangeKey = "today" | "yesterday" | "7d" | "30d" | "custom";

export const RANGE_LABELS: Record<RangeKey, string> = {
  today: "Today",
  yesterday: "Yesterday",
  "7d": "Last 7 days",
  "30d": "Last 30 days",
  custom: "Custom",
};

export function rangeFor(key: RangeKey, custom?: { from: string; to: string }): { from: string; to: string } {
  const t = today();
  switch (key) {
    case "today":
      return { from: t, to: t };
    case "yesterday":
      return { from: addDays(t, -1), to: addDays(t, -1) };
    case "7d":
      return { from: addDays(t, -6), to: t };
    case "30d":
      return { from: addDays(t, -29), to: t };
    case "custom":
      return custom ?? { from: addDays(t, -6), to: t };
  }
}
