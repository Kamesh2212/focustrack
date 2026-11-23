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
  SiteUsage,
  LimitStatus,
  BrowserStatus,
  FocusPlan,
  FocusStatus,
  GoalStatus,
  InstalledApp,
  BlockPreview,
  Note,
  Profile,
  Resource,
  SyncStatus,
  DeviceTotal,
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

const SITES: [number, string, number, number][] = [
  // id, domain, categoryId, typical minutes/day
  [1, "youtube.com", 5, 80],
  [2, "github.com", 3, 70],
  [3, "instagram.com", 6, 45],
  [4, "reddit.com", 6, 35],
  [5, "linkedin.com", 6, 25],
  [6, "stackoverflow.com", 3, 20],
  [7, "mail.google.com", 4, 15],
];

let limits: LimitStatus[] = [
  { id: 1, targetType: "website", targetId: 1, name: "youtube.com", key: "youtube.com", color: "#ef4444", dailyLimitSecs: 3600, warnPct: 80, action: "block", enabled: true, usedSecs: 3010, extraSecs: 0, overridesUsed: 0, path: "", scope: null, reach: "all", othersSecs: 1240 },
  { id: 2, targetType: "website", targetId: 3, name: "instagram.com", key: "instagram.com", color: "#ec4899", dailyLimitSecs: 1800, warnPct: 80, action: "notify", enabled: true, usedSecs: 1950, extraSecs: 0, overridesUsed: 0, path: "", scope: null, reach: "device", othersSecs: 0 },
  { id: 3, targetType: "app", targetId: 6, name: "Steam", key: "launcher.exe", color: "#8b5cf6", dailyLimitSecs: 2700, warnPct: 80, action: "warn_then_block", enabled: true, usedSecs: 2400, extraSecs: 0, overridesUsed: 0, path: "C:\\Program Files\\HoYoPlay\\launcher.exe", scope: "c:\\program files\\hoyoplay\\", reach: "device", othersSecs: 0 },
];
let nextLimitId = 4;

const INSTALLED: InstalledApp[] = [
  { exeName: "Code.exe", name: "Visual Studio Code", path: "C:\\Apps\\Code.exe" },
  { exeName: "Discord.exe", name: "Discord", path: "C:\\Apps\\Discord.exe" },
  { exeName: "steam.exe", name: "Steam", path: "C:\\Apps\\steam.exe" },
  { exeName: "blender.exe", name: "Blender", path: "C:\\Apps\\blender.exe" },
  { exeName: "Zoom.exe", name: "Zoom Workplace", path: "C:\\Apps\\Zoom.exe" },
  { exeName: "obs64.exe", name: "OBS Studio", path: "C:\\Apps\\obs64.exe" },
];
let nextAppId = 100;

let focusActive: FocusStatus | null = null;
const focusHistory = [
  { id: 3, label: "Deep work", mode: "focus" as const, subject: null, goalText: "Finish chapter 3", goalId: null, day: today(), start: `${today()} 09:10:00`, end: `${today()} 10:00:00`, plannedSecs: 3000, actualSecs: 3000, completed: true },
  { id: 2, label: "Pomodoro", mode: "focus" as const, subject: null, goalText: null, goalId: null, day: addDays(today(), -1), start: `${addDays(today(), -1)} 14:00:00`, end: `${addDays(today(), -1)} 14:25:00`, plannedSecs: 1500, actualSecs: 1500, completed: true },
  { id: 1, label: "Maths", mode: "study" as const, subject: "Maths", goalText: "Problem set 4", goalId: null, day: addDays(today(), -1), start: `${addDays(today(), -1)} 19:00:00`, end: `${addDays(today(), -1)} 19:40:00`, plannedSecs: 5400, actualSecs: 2400, completed: false },
];

let goals: GoalStatus[] = [
  { id: 1, kind: "max_screen", categoryId: null, category: null, color: "#6366f1", target: 6 * 3600, title: null, enabled: true, current: 4 * 3600 + 1200, state: "on_track", label: "Keep screen time under 6h" },
  { id: 2, kind: "min_category", categoryId: 2, category: "Education", color: "#0ea5e9", target: 7200, title: null, enabled: true, current: 2700, state: "in_progress", label: "Spend at least 2h on Education" },
  { id: 3, kind: "max_category", categoryId: 6, category: "Social Media", color: "#ec4899", target: 1800, title: null, enabled: true, current: 2400, state: "exceeded", label: "Keep Social Media under 30m" },
  { id: 4, kind: "min_focus_sessions", categoryId: null, category: null, color: "#6366f1", target: 3, title: null, enabled: true, current: 1, state: "in_progress", label: "Complete 3 focus sessions" },
  { id: 5, kind: "custom", categoryId: null, category: null, color: "#6366f1", target: 1, title: "Read 20 pages", enabled: true, current: 0, state: "in_progress", label: "Read 20 pages" },
  { id: 6, kind: "custom", categoryId: null, category: null, color: "#6366f1", target: 1, title: "Exercise for 30 minutes", enabled: true, current: 1, state: "achieved", label: "Exercise for 30 minutes" },
];
let nextGoalId = 7;

let profiles: Profile[] = [
  { id: 1, name: "Study Mode", mode: "block", categories: ["Gaming", "Entertainment", "Social Media"], apps: [], sites: [], unblockOnBreaks: false, endEarly: "confirm", builtin: true },
  { id: 2, name: "Work Mode", mode: "block", categories: ["Gaming", "Entertainment"], apps: [], sites: ["reddit.com", "instagram.com", "x.com"], unblockOnBreaks: false, endEarly: "allow", builtin: true },
  { id: 3, name: "Deep Focus", mode: "allow_only", categories: ["Study", "Education", "Development", "Productivity"], apps: [], sites: ["google.com", "github.com", "stackoverflow.com"], unblockOnBreaks: false, endEarly: "strict", builtin: true },
  { id: 4, name: "Personal", mode: "block", categories: [], apps: [], sites: [], unblockOnBreaks: false, endEarly: "confirm", builtin: true },
];
let nextProfileId = 5;

function previewFor(id: number): BlockPreview {
  const p = profiles.find((x) => x.id === id)!;
  const all = [
    { name: "Steam", exeName: "steam.exe", category: "Gaming" },
    { name: "Epic Games Launcher", exeName: "EpicGamesLauncher.exe", category: "Gaming" },
    { name: "Spotify", exeName: "Spotify.exe", category: "Entertainment" },
    { name: "Netflix", exeName: "Netflix.exe", category: "Entertainment" },
    { name: "Discord", exeName: "Discord.exe", category: "Communication" },
    { name: "Visual Studio Code", exeName: "Code.exe", category: "Development" },
    { name: "Microsoft Word", exeName: "WINWORD.EXE", category: "Productivity" },
    { name: "Anki", exeName: "anki.exe", category: "Study" },
  ];
  const sites: Record<string, string> = {
    "youtube.com": "Entertainment", "netflix.com": "Entertainment", "twitch.tv": "Entertainment",
    "instagram.com": "Social Media", "reddit.com": "Social Media", "tiktok.com": "Social Media",
    "coursera.org": "Study", "khanacademy.org": "Study", "wikipedia.org": "Education", "github.com": "Development",
  };
  const inCats = (c: string) => p.categories.includes(c);
  const chosen = all.filter((a) => inCats(a.category) || p.apps.includes(a.exeName.toLowerCase()));
  const siteList = [...Object.entries(sites).filter(([, c]) => inCats(c)).map(([d]) => d), ...p.sites].sort();
  return p.mode === "allow_only"
    ? { mode: p.mode, blockedApps: all.filter((a) => !chosen.includes(a)), blockedSites: [], allowedApps: chosen, allowedSites: siteList }
    : { mode: p.mode, blockedApps: chosen, blockedSites: siteList, allowedApps: [], allowedSites: [] };
}

let notes: Note[] = [
  { id: 1, notebook: "Maths", title: "Limits", body: "lim x→0 sin(x)/x = 1\n\nSqueeze theorem proof: …", focusSessionId: 3, sessionLabel: "Study session", createdAt: `${today()} 09:20:00`, updatedAt: `${today()} 09:40:00` },
  { id: 2, notebook: "Physics", title: "Newton's laws", body: "1. Inertia\n2. F = ma\n3. Action–reaction", focusSessionId: null, sessionLabel: null, createdAt: `${today()} 08:00:00`, updatedAt: `${today()} 08:05:00` },
  { id: 3, notebook: "Quick notes", title: "Email Prof. about deadline", body: "Email Prof. about deadline", focusSessionId: null, sessionLabel: null, createdAt: `${today()} 11:00:00`, updatedAt: `${today()} 11:00:00` },
];
let nextNoteId = 4;

let phoneAccess = false;
let retention: number | null = null;
const mockPaused: Record<string, boolean> = {};

const sync: SyncStatus = {
  signedIn: false,
  enabled: false,
  email: null,
  server: null,
  deviceId: "d1",
  deviceName: "Home PC",
  state: "off",
  lastSync: null,
  lastError: null,
  pending: 0,
  otherDevices: 0,
};

let resources: Resource[] = [
  { id: 1, title: "Google Scholar", url: "https://scholar.google.com", subject: null },
  { id: 2, title: "Khan Academy", url: "https://www.khanacademy.org", subject: null },
  { id: 3, title: "Coursera", url: "https://www.coursera.org", subject: null },
  { id: 4, title: "MIT OCW Calculus", url: "https://ocw.mit.edu/courses/18-01sc", subject: "Maths" },
];
let nextResourceId = 5;

function stamp(d: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function startFocus(plan: FocusPlan, devices: string[] = []): FocusStatus {
  const start = new Date();
  const end = new Date(start.getTime() + plan.focusMins * 60000);
  focusActive = {
    label: plan.label, phase: "focus", round: 1, rounds: plan.rounds, startedAt: stamp(start), endsAt: stamp(end),
    remainingSecs: plan.focusMins * 60, totalSecs: plan.focusMins * 60, plan,
    mode: plan.mode, subject: plan.subject, goalText: plan.goalText,
    planEndsAt: stamp(new Date(start.getTime() + (plan.focusMins * plan.rounds + plan.breakMins * (plan.rounds - 1)) * 60000)),
    profileName: profiles.find((p) => p.id === plan.profileId)?.name ?? null,
    blocking: plan.profileId != null,
    preview: plan.profileId != null ? previewFor(plan.profileId) : null,
    endEarly: profiles.find((p) => p.id === plan.profileId)?.endEarly ?? "allow",
    stopWaitSecs: null,
    run: devices.length ? { id: "run1", joined: false, origin: "This PC", devices } : null,
  };
  return focusActive;
}

function goalLabel(kind: string, cat: string | null, target: number): string {
  const f = (s: number) => (s >= 3600 ? `${Math.floor(s / 3600)}h${s % 3600 ? ` ${Math.round((s % 3600) / 60)}m` : ""}` : `${Math.round(s / 60)}m`);
  if (kind === "max_screen") return `Keep screen time under ${f(target)}`;
  if (kind === "min_category") return `Spend at least ${f(target)} on ${cat}`;
  if (kind === "max_category") return `Keep ${cat} under ${f(target)}`;
  return `Complete ${target} focus session${target === 1 ? "" : "s"}`;
}

const settings: Settings = {
  paused: false,
  idleThresholdSecs: 300,
  autostart: true,
  theme: "system",
  overridesPerDay: 1,
  goalReminder: "20:00",
  dashboardLayout: "",
  notificationsEnabled: true,
  breakReminderMins: 120,
  websiteTracking: true,
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

function sitesFor(from: string, to: string): SiteUsage[] {
  const out = SITES.map(([id, domain, categoryId, mins]) => {
    const cat = CATEGORIES.find((c) => c.id === categoryId)!;
    let secs = 0;
    let sessions = 0;
    for (let d = from; d <= to; d = addDays(d, 1)) {
      const isToday = d === today();
      const portion = isToday ? Math.min(1, new Date().getHours() / 20) : 1;
      secs += Math.round(mins * 60 * (0.4 + hash(d + domain) * 1.2) * portion);
      sessions += 2 + Math.floor(hash(domain + d) * 8);
    }
    return { id, domain, categoryId, category: cat.name, color: cat.color, kind: cat.kind, secs, sessions };
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

const DEVICE_SHARES: [Omit<DeviceTotal, "activeSecs" | "idleSecs" | "lastActivity">, number][] = [
  [{ id: "d1", name: "Home PC", platform: "windows", current: true }, 0.55],
  [{ id: "d2", name: "Study laptop", platform: "windows", current: false }, 0.3],
  [{ id: "d3", name: "Pixel 8", platform: "android", current: false }, 0.15],
];

/** Sample data as if three devices were synced; a filter scales the headline numbers. */
function withDevices<T extends { activeSecs: number; idleSecs: number; screenSecs?: number }>(x: T, device?: string): T & { devices: DeviceTotal[]; uniqueActiveSecs: number; devicesUniqueSecs: number } {
  const total = x.activeSecs;
  const devices = DEVICE_SHARES.map(([d, share], i) => ({
    ...d,
    activeSecs: Math.round(total * share),
    idleSecs: Math.round(x.idleSecs * share),
    lastActivity: d.current ? null : new Date(Date.now() - (i * 4 + 2) * 60_000).toISOString().slice(0, 19).replace("T", " "),
  }));
  const share =
    !device || device === "all" ? 1 : device === "desktop" ? 0.85 : device === "mobile" ? 0.15 : (DEVICE_SHARES.find(([d]) => d.id === device)?.[1] ?? 1);
  const scaled = { ...x, activeSecs: Math.round(x.activeSecs * share), idleSecs: Math.round(x.idleSecs * share) };
  if (x.screenSecs !== undefined) scaled.screenSecs = Math.round(x.screenSecs * share);
  return { ...scaled, devices, uniqueActiveSecs: Math.round(scaled.activeSecs * (share === 1 || share === 0.85 ? 0.88 : 1)), devicesUniqueSecs: Math.round(total * 0.88) };
}

function dashboard(): Omit<Dashboard, "uniqueActiveSecs" | "devices" | "devicesUniqueSecs"> {
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
    topSites: sitesFor(t, t).slice(0, 8),
    siteSecs: sitesFor(t, t).reduce((s, x) => s + x.secs, 0),
    categories: cats,
    week,
    hourly: hourly(day.activeSecs).map((v, h) => (h > new Date().getHours() ? 0 : v)),
    live: {
      state: settings.paused ? "paused" : "active",
      appName: settings.paused ? null : "Visual Studio Code",
      exeName: settings.paused ? null : "Code.exe",
      domain: null,
      since: settings.paused
        ? null
        : `${t} ${p(sinceDate.getHours())}:${p(sinceDate.getMinutes())}:${p(sinceDate.getSeconds())}`,
      idleSecs: 2,
    },
  };
}

function report(from: string, to: string): Omit<Report, "uniqueActiveSecs" | "devices" | "devicesUniqueSecs"> {
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
    productivity: days.map((d) => ({
      day: d.day,
      productiveSecs: Math.round(d.activeSecs * (0.35 + hash(d.day + "p") * 0.2)),
      neutralSecs: Math.round(d.activeSecs * 0.3),
      distractingSecs: Math.round(d.activeSecs * (0.15 + hash(d.day + "d") * 0.15)),
    })),
    focusSecs: 5400 * Math.min(days.length, 7) / 3,
    focusSessions: Math.round(Math.min(days.length, 7) * 1.4),
    days,
    apps,
    sites: sitesFor(from, to),
    siteSecs: sitesFor(from, to).reduce((s, x) => s + x.secs, 0),
    categories: cats,
    hourly: hours,
    heatmap: Array.from({ length: 7 }, (_, wd) =>
      hourly(active / 7).map((v, h) => Math.round(v * (wd >= 5 ? 0.5 : 1) * (0.6 + hash(`${wd}-${h}`) * 0.8))),
    ),
  };
}

function siteHistory(from: string, to: string, search: string): SessionRow[] {
  const q = search.trim().toLowerCase();
  return history(from, to, "")
    .map((r, i) => {
      const [, domain, catId] = SITES[Math.floor(hash(r.start + i) * SITES.length)];
      const cat = CATEGORIES.find((c) => c.id === catId)!;
      return { ...r, appName: domain, exeName: "chrome", category: cat.name, color: cat.color };
    })
    .filter((r) => !q || [r.appName, r.category].some((x) => x.toLowerCase().includes(q)));
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
        return withDevices(dashboard(), a.device);
      case "get_live_status":
        return dashboard().live;
      case "get_apps":
        return appsFor(a.from, a.to);
      case "get_categories":
        return CATEGORIES;
      case "get_report":
        return withDevices(report(a.from, a.to), a.device);
      case "get_history":
        return a.kind === "websites" ? siteHistory(a.from, a.to, a.search ?? "") : history(a.from, a.to, a.search ?? "");
      case "get_websites":
        return sitesFor(a.from, a.to);
      case "get_browser_status":
        return { serverRunning: true, port: 17865, browsers: [{ browser: "chrome", lastSeenSecs: 4, connected: true }] } satisfies BrowserStatus;
      case "get_limits":
        return limits.map((l) => ({ ...l }));
      case "set_limit": {
        const l = args as unknown as LimitStatus;
        const existing = limits.find((x) => x.targetType === l.targetType && x.targetId === l.targetId);
        const name =
          l.targetType === "app"
            ? APPS.find((x) => x[0] === l.targetId)?.[1] ?? "App"
            : SITES.find((x) => x[0] === l.targetId)?.[1] ?? "site";
        if (existing) Object.assign(existing, l, l.reach === "all" ? {} : { othersSecs: 0 });
        else limits.push({ ...l, id: nextLimitId++, name, color: "#6366f1", usedSecs: 600, othersSecs: l.reach === "all" ? 240 : 0 });
        return null;
      }
      case "delete_limit":
        limits = limits.filter((l) => l.id !== a.id);
        return null;
      case "set_notifications_enabled":
        settings.notificationsEnabled = a.enabled;
        return null;
      case "set_break_reminder":
        settings.breakReminderMins = a.mins;
        return null;
      case "get_installed_apps":
        return INSTALLED;
      case "ensure_app":
        return APPS.find((x) => x[2].toLowerCase() === String(a.exeName).toLowerCase())?.[0] ?? nextAppId++;
      case "get_focus": {
        if (focusActive) {
          const left = Math.max(0, Math.round((new Date(focusActive.endsAt.replace(" ", "T")).getTime() - Date.now()) / 1000));
          focusActive = { ...focusActive, remainingSecs: left };
        }
        const hist = a.mode ? focusHistory.filter((h) => h.mode === a.mode) : focusHistory;
        return { active: focusActive, todaySecs: 3000, todaySessions: 1, weekSecs: 6900, weekSessions: 2, streakDays: 2, history: hist, stopPhrase: "I want to stop" };
      }
      case "start_focus":
        return startFocus(a.plan, (a.devices as string[] | undefined) ?? []);
      case "stop_focus": {
        const rule = focusActive?.endEarly ?? "allow";
        if (rule !== "allow" && String(a.phrase ?? "").trim().toLowerCase() !== "i want to stop")
          throw new Error('Type "I want to stop" to end the session early');
        focusActive = null;
        return null;
      }
      case "request_stop_focus":
        if (focusActive) focusActive = { ...focusActive, stopWaitSecs: 0 };
        return 0;
      case "preview_profile":
        return previewFor(a.profileId);
      case "get_profiles":
        return profiles.map((p) => ({ ...p }));
      case "save_profile": {
        const p = a.profile as unknown as Profile;
        if (p.id) profiles = profiles.map((x) => (x.id === p.id ? { ...p, builtin: x.builtin } : x));
        else profiles.push({ ...p, id: nextProfileId++, builtin: false });
        return p.id ?? nextProfileId - 1;
      }
      case "delete_profile":
        profiles = profiles.filter((p) => p.id !== a.id || p.builtin);
        return null;
      case "toggle_goal": {
        const g = goals.find((x) => x.id === a.id);
        if (g) {
          g.current = g.current ? 0 : 1;
          g.state = g.current ? "achieved" : "in_progress";
        }
        return !!g?.current;
      }
      case "get_notebooks": {
        const map = new Map<string, { name: string; count: number; updatedAt: string }>();
        for (const n of notes) {
          const nb = map.get(n.notebook) ?? { name: n.notebook, count: 0, updatedAt: n.updatedAt };
          nb.count++;
          if (n.updatedAt > nb.updatedAt) nb.updatedAt = n.updatedAt;
          map.set(n.notebook, nb);
        }
        return [...map.values()].sort((x, y) => y.updatedAt.localeCompare(x.updatedAt));
      }
      case "get_notes": {
        const q = String(a.search ?? "").toLowerCase();
        return notes
          .filter((n) => (!a.notebook || n.notebook === a.notebook) && (!q || (n.title + n.body).toLowerCase().includes(q)))
          .sort((x, y) => y.updatedAt.localeCompare(x.updatedAt));
      }
      case "create_note": {
        const n: Note = {
          id: nextNoteId++, notebook: String(a.notebook || "General"), title: String(a.title), body: String(a.body),
          focusSessionId: null, sessionLabel: focusActive?.label ?? null, createdAt: stamp(new Date()), updatedAt: stamp(new Date()),
        };
        notes.push(n);
        return n;
      }
      case "update_note": {
        const n = notes.find((x) => x.id === a.id)!;
        Object.assign(n, { notebook: a.notebook || "General", title: a.title, body: a.body, updatedAt: stamp(new Date()) });
        return { ...n };
      }
      case "delete_note":
        notes = notes.filter((n) => n.id !== a.id);
        return null;
      case "get_resources":
        return resources;
      case "add_resource":
        if (!/^https?:\/\//.test(String(a.url))) throw new Error("Enter a full web address starting with https://");
        resources.push({ id: nextResourceId++, title: a.title, url: a.url, subject: a.subject });
        return null;
      case "delete_resource":
        resources = resources.filter((r) => r.id !== a.id);
        return null;
      case "open_url":
        window.open(String(a.url), "_blank", "noopener");
        return null;
      case "get_platform":
        // Preview the phone app with ?platform=android
        return new URLSearchParams(window.location.search).get("platform") ?? "windows";
      case "get_insets":
        return { top: 0, bottom: 0 };
      case "phone_usage_access":
        return phoneAccess;
      case "open_usage_access":
        phoneAccess = true;
        return null;
      case "import_phone_usage":
        return phoneAccess ? 3 : null;
      case "sync_status":
        return { ...sync };
      case "sync_sign_up":
      case "sync_sign_in": {
        if (!String(a.email).includes("@")) throw new Error("Enter a valid email address");
        if (cmd === "sync_sign_up" && String(a.password).length < 10) throw new Error("Use a password of at least 10 characters");
        Object.assign(sync, { signedIn: true, enabled: true, email: a.email, server: a.server, deviceName: a.deviceName, state: "idle", lastSync: new Date().toISOString().slice(0, 19).replace("T", " "), otherDevices: 1 });
        return cmd === "sync_sign_up" ? "FT1-7K3M-QX9P-2HDW-V8RT-NC4J-6YBF-ZA1E-5GKS-0PWM-3TQV-H7XD-9RNC-JB2-4F8" : null;
      }
      case "sync_sign_out":
        Object.assign(sync, { signedIn: false, enabled: false, email: null, state: "off", otherDevices: 0 });
        return null;
      case "sync_now":
        sync.lastSync = new Date().toISOString().slice(0, 19).replace("T", " ");
        return null;
      case "sync_set_enabled":
        Object.assign(sync, { enabled: a.enabled, state: a.enabled ? "idle" : "off" });
        return null;
      case "sync_devices":
        return [
          { id: sync.deviceId, name: sync.deviceName, platform: "windows", appVersion: "1.2.0", lastSeen: new Date().toISOString(), online: true, lastActivity: null, paused: false, syncError: null, syncDisabled: false, current: true },
          { id: "d2", name: "Study laptop", platform: "windows", appVersion: "1.2.0", lastSeen: new Date(Date.now() - 3600_000).toISOString(), online: false, lastActivity: "2026-11-23 09:41:00", paused: mockPaused.d2 ?? false, syncError: null, syncDisabled: false, current: false },
          { id: "d3", name: "Pixel 8", platform: "android", appVersion: "1.2.0", lastSeen: new Date(Date.now() - 60_000).toISOString(), online: true, lastActivity: "2026-11-23 10:12:00", paused: mockPaused.d3 ?? false, syncError: "can't reach the sync server: timed out", syncDisabled: false, current: false },
        ];
      case "sync_device_control":
        if (a.pauseTracking !== null) mockPaused[String(a.id)] = Boolean(a.pauseTracking);
        return null;
      case "sync_remove_device":
        sync.otherDevices = 0;
        return null;
      case "sync_delete_history":
        return 42;
      case "sync_delete_account":
        if (String(a.password).length < 10) throw new Error("Wrong password");
        Object.assign(sync, { signedIn: false, enabled: false, email: null, state: "off", otherDevices: 0 });
        return null;
      case "sync_retention":
        return retention;
      case "sync_set_retention":
        retention = (a.days as number | null) ?? null;
        return retention;
      case "sync_export":
        return "C:/Users/you/Downloads/FocusTrack-export-2026-11-23-1015.json";
      case "sync_rename_device":
        sync.deviceName = String(a.name);
        return null;
      case "get_admin_mode":
        return { elevated: false, enabled: false, needsAdmin: ["Genshin Impact"] };
      case "enable_admin_mode":
      case "disable_admin_mode":
        return null;
      case "get_study_time":
        return [5400, 21600];
      case "override_limit": {
        const l = limits.find((x) => x.id === a.id);
        if (l) {
          if (l.overridesUsed >= settings.overridesPerDay) throw new Error("You've used all your extensions for today");
          l.extraSecs += 900;
          l.overridesUsed += 1;
        }
        return null;
      }
      case "set_overrides_per_day":
        settings.overridesPerDay = a.count;
        return null;
      case "set_goal_reminder":
        settings.goalReminder = a.time;
        return null;
      case "set_dashboard_layout":
        settings.dashboardLayout = a.layout;
        return null;
      case "get_goals":
        return goals.map((g) => ({ ...g }));
      case "set_goal": {
        const g = args as unknown as GoalStatus;
        const cat = CATEGORIES.find((c) => c.id === g.categoryId) ?? null;
        const base = { ...g, category: cat?.name ?? null, color: cat?.color ?? "#6366f1", label: g.kind === "custom" ? g.title ?? "" : g.title ? `${g.title}` : goalLabel(g.kind, cat?.name ?? null, g.target) };
        if (g.id) goals = goals.map((x) => (x.id === g.id ? { ...x, ...base } : x));
        else goals.push({ ...base, id: nextGoalId++, current: 0, state: g.kind.startsWith("max") ? "on_track" : "in_progress" });
        return null;
      }
      case "delete_goal":
        goals = goals.filter((g) => g.id !== a.id);
        return null;
      case "get_insights":
        return [
          { tone: "warn", text: "Your youtube.com usage increased by 35% this week (6h 10m vs 4h 34m)." },
          { tone: "good", text: "Your Visual Studio Code usage increased by 28% this week (14h 5m vs 11h)." },
          { tone: "info", text: "You spend most of your computer time between 2 PM and 6 PM." },
          { tone: "info", text: "You spent 8h 10m on entertainment, social media and gaming apps this week." },
          { tone: "good", text: "You completed 6 focus sessions (4h 10m focused) this week. You're on a 2-day focus streak." },
        ];
      case "set_website_tracking":
        settings.websiteTracking = a.enabled;
        return null;
      case "export_csv":
        return `C:\\Users\\you\\Downloads\\focustrack-${a.kind}-${a.from}-to-${a.to}.csv`;
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
