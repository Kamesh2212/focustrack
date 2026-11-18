// Sample data so the UI can be developed in a normal browser (`npm run dev`)
// without the Rust backend. Never used inside the desktop app.

import type {
  AppUsage,
  Category,
  CategoryUsage,
  Dashboard,
  DayTotal,
  Report,
  SessionRow,
  Settings,
} from "./types";
import { addDays, today } from "./format";

const CATEGORIES: Category[] = [
  { id: 1, name: "Productivity", color: "#6366f1", kind: "productive" },
  { id: 2, name: "Education", color: "#0ea5e9", kind: "productive" },
  { id: 3, name: "Development", color: "#10b981", kind: "productive" },
  { id: 4, name: "Communication", color: "#f59e0b", kind: "neutral" },
  { id: 5, name: "Entertainment", color: "#ef4444", kind: "distracting" },
  { id: 6, name: "Social Media", color: "#ec4899", kind: "distracting" },
  { id: 7, name: "Gaming", color: "#8b5cf6", kind: "distracting" },
  { id: 8, name: "Other", color: "#94a3b8", kind: "neutral" },
];

const APPS: [number, string, string, number, number][] = [
  // id, name, exe, categoryId, typical minutes/day
  [1, "Google Chrome", "chrome.exe", 8, 135],
  [2, "Visual Studio Code", "Code.exe", 3, 105],
  [3, "Microsoft Word", "WINWORD.EXE", 1, 55],
  [4, "Spotify", "Spotify.exe", 5, 40],
  [5, "Discord", "Discord.exe", 4, 35],
  [6, "Steam", "steam.exe", 7, 70],
  [7, "Windows Explorer", "explorer.exe", 1, 12],
  [8, "Windows Terminal", "WindowsTerminal.exe", 3, 25],
];

const settings: Settings = {
  paused: false,
  idleThresholdSecs: 300,
  autostart: true,
  theme: "system",
  dataDir: "C:\\Users\\you\\AppData\\Roaming\\com.focustrack.app",
};

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  // murmur3 finaliser so similar strings give unrelated values
  h ^= h >>> 16;
  h = Math.imul(h, 0x85ebca6b);
  h ^= h >>> 13;
  h = Math.imul(h, 0xc2b2ae35);
  h ^= h >>> 16;
  return (h >>> 0) / 4294967295;
}

function appSecs(appIdx: number, day: string): number {
  const [, , , , mins] = APPS[appIdx];
  const weekend = [0, 6].includes(new Date(day).getDay());
  const factor = 0.4 + hash(day + appIdx) * 1.2;
  const isToday = day === today();
  const portion = isToday ? Math.min(1, new Date().getHours() / 20) : 1;
  return Math.round(mins * 60 * factor * portion * (weekend && appIdx === 1 ? 0.3 : 1));
}

function appsFor(from: string, to: string): AppUsage[] {
  const out = APPS.map(([id, name, exeName, categoryId], idx) => {
    const cat = CATEGORIES.find((c) => c.id === categoryId)!;
    let secs = 0;
    let sessions = 0;
    for (let d = from; d <= to; d = addDays(d, 1)) {
      secs += appSecs(idx, d);
      sessions += 1 + Math.floor(hash(d + name) * 6);
    }
    return { id, name, exeName, categoryId, category: cat.name, color: cat.color, kind: cat.kind, secs, sessions };
  });
  return out.sort((a, b) => b.secs - a.secs);
}

function categoriesFor(apps: AppUsage[]): CategoryUsage[] {
  const map = new Map<string, CategoryUsage>();
  for (const a of apps) {
    const c = map.get(a.category) ?? { name: a.category, color: a.color, kind: a.kind, secs: 0 };
    c.secs += a.secs;
    map.set(a.category, c);
  }
  return [...map.values()].filter((c) => c.secs > 0).sort((a, b) => b.secs - a.secs);
}

function daysFor(from: string, to: string): DayTotal[] {
  const out: DayTotal[] = [];
  for (let d = from; d <= to; d = addDays(d, 1)) {
    const active = APPS.reduce((s, _, i) => s + appSecs(i, d), 0);
    out.push({ day: d, activeSecs: active, idleSecs: Math.round(active * (0.08 + hash(d) * 0.1)) });
  }
  return out;
}

function hourly(total: number): number[] {
  const shape = [0, 0, 0, 0, 0, 0, 0, 0.2, 0.6, 1, 1, 0.9, 0.5, 0.8, 1, 1, 0.9, 0.7, 0.5, 0.7, 0.9, 0.8, 0.4, 0.1];
  const sum = shape.reduce((a, b) => a + b, 0);
  return shape.map((v) => Math.round((v / sum) * total));
}

function avg(days: DayTotal[]): number {
  const used = days.filter((d) => d.activeSecs > 0);
  return used.length ? Math.round(used.reduce((s, d) => s + d.activeSecs, 0) / used.length) : 0;
}

function split(cats: CategoryUsage[]) {
  return {
    productiveSecs: cats.filter((c) => c.kind === "productive").reduce((s, c) => s + c.secs, 0),
    distractingSecs: cats.filter((c) => c.kind === "distracting").reduce((s, c) => s + c.secs, 0),
  };
}

function dashboard(): Dashboard {
  const t = today();
  const apps = appsFor(t, t);
  const cats = categoriesFor(apps);
  const [day] = daysFor(t, t);
  const week = daysFor(addDays(t, -6), t);
  const sinceDate = new Date(Date.now() - 23 * 60 * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return {
    today: t,
    screenSecs: day.activeSecs + day.idleSecs,
    activeSecs: day.activeSecs,
    idleSecs: day.idleSecs,
    prevDayActiveSecs: daysFor(addDays(t, -1), addDays(t, -1))[0].activeSecs,
    dailyAvgSecs: avg(daysFor(addDays(t, -29), t)),
    weeklyAvgSecs: Math.round(week.reduce((s, d) => s + d.activeSecs, 0) / 7),
    sessionCount: 4,
    ...split(cats),
    topApps: apps.slice(0, 8),
    categories: cats,
    week,
    hourly: hourly(day.activeSecs).map((v, h) => (h > new Date().getHours() ? 0 : v)),
    live: {
      state: settings.paused ? "paused" : "active",
      appName: settings.paused ? null : "Visual Studio Code",
      exeName: settings.paused ? null : "Code.exe",
      since: settings.paused
        ? null
        : `${t} ${p(sinceDate.getHours())}:${p(sinceDate.getMinutes())}:${p(sinceDate.getSeconds())}`,
      idleSecs: 2,
    },
  };
}

function report(from: string, to: string): Report {
  const apps = appsFor(from, to);
  const cats = categoriesFor(apps);
  const days = daysFor(from, to);
  const len = days.length;
  const prev = daysFor(addDays(from, -len), addDays(from, -1));
  const active = days.reduce((s, d) => s + d.activeSecs, 0);
  const isWeekend = (d: DayTotal) => [0, 6].includes(new Date(d.day).getDay());
  const hours = hourly(active);
  return {
    from,
    to,
    activeSecs: active,
    idleSecs: days.reduce((s, d) => s + d.idleSecs, 0),
    dailyAvgSecs: avg(days),
    prevActiveSecs: prev.reduce((s, d) => s + d.activeSecs, 0),
    weekdayAvgSecs: avg(days.filter((d) => !isWeekend(d))),
    weekendAvgSecs: avg(days.filter(isWeekend)),
    ...split(cats),
    peakHour: hours.indexOf(Math.max(...hours)),
    days,
    apps,
    categories: cats,
    hourly: hours,
    heatmap: Array.from({ length: 7 }, (_, wd) =>
      hourly(active / 7).map((v, h) => Math.round(v * (wd >= 5 ? 0.5 : 1) * (0.6 + hash(`${wd}-${h}`) * 0.8))),
    ),
  };
}

function history(from: string, to: string, search: string): SessionRow[] {
  const rows: SessionRow[] = [];
  let id = 1;
  const q = search.trim().toLowerCase();
  for (let d = to; d >= from && rows.length < 300; d = addDays(d, -1)) {
    let minute = 22 * 60;
    for (let i = 0; i < 14; i++) {
      const idx = Math.floor(hash(d + i) * APPS.length);
      const [, name, exe, catId] = APPS[idx];
      const cat = CATEGORIES.find((c) => c.id === catId)!;
      const len = 5 + Math.floor(hash(i + d) * 50);
      minute -= len + Math.floor(hash(d + i + "g") * 15);
      if (minute < 7 * 60) break;
      const fmt = (m: number) => `${d} ${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}:00`;
      const row = {
        id: id++, appName: name, exeName: exe, category: cat.name, color: cat.color,
        day: d, start: fmt(minute), end: fmt(minute + len), secs: len * 60,
      };
      if (!q || [name, exe, cat.name].some((s) => s.toLowerCase().includes(q))) rows.push(row);
    }
  }
  return rows;
}

export async function mock<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  await new Promise((r) => setTimeout(r, 120));
  const a = args as Record<string, never>;
  const result: unknown = (() => {
    switch (cmd) {
      case "get_dashboard":
        return dashboard();
      case "get_live_status":
        return dashboard().live;
      case "get_apps":
        return appsFor(a.from, a.to);
      case "get_categories":
        return CATEGORIES;
      case "get_report":
        return report(a.from, a.to);
      case "get_history":
        return history(a.from, a.to, a.search ?? "");
      case "export_csv":
        return `C:\\Users\\you\\Downloads\\focustrack-${a.from}-to-${a.to}.csv`;
      case "get_settings":
        return { ...settings };
      case "set_paused":
        settings.paused = a.paused;
        return null;
      case "set_idle_threshold":
        settings.idleThresholdSecs = a.secs;
        return null;
      case "set_autostart":
        settings.autostart = a.enabled;
        return null;
      case "set_theme":
        settings.theme = a.theme;
        return null;
      case "delete_data_older_than":
        return 0;
      default:
        return null;
    }
  })();
  return result as T;
}
