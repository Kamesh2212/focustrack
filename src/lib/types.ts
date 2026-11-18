// Mirrors the serde structs in src-tauri/src/{commands,analytics,tracker}.rs

export type TrackingState = "starting" | "active" | "idle" | "locked" | "paused";

export interface LiveStatus {
  state: TrackingState;
  appName: string | null;
  exeName: string | null;
  domain: string | null;
  since: string | null;
  idleSecs: number;
}

export interface DayTotal {
  day: string;
  activeSecs: number;
  idleSecs: number;
}

export type CategoryKind = "productive" | "neutral" | "distracting";

export interface AppUsage {
  id: number;
  name: string;
  exeName: string;
  categoryId: number | null;
  category: string;
  color: string;
  kind: CategoryKind;
  secs: number;
  sessions: number;
}

export interface SiteUsage {
  id: number;
  domain: string;
  categoryId: number | null;
  category: string;
  color: string;
  kind: CategoryKind;
  secs: number;
  sessions: number;
}

export interface LimitStatus {
  id: number;
  targetType: "app" | "website";
  targetId: number;
  name: string;
  color: string;
  dailyLimitSecs: number;
  warnPct: number;
  block: boolean;
  enabled: boolean;
  usedSecs: number;
}

export interface BrowserStatus {
  serverRunning: boolean;
  port: number;
  browsers: { browser: string; lastSeenSecs: number; connected: boolean }[];
}

export type HistoryKind = "apps" | "websites";

export interface CategoryUsage {
  name: string;
  color: string;
  kind: CategoryKind;
  secs: number;
}

export interface Category {
  id: number;
  name: string;
  color: string;
  kind: CategoryKind;
}

export interface Dashboard {
  today: string;
  screenSecs: number;
  activeSecs: number;
  idleSecs: number;
  prevDayActiveSecs: number;
  dailyAvgSecs: number;
  weeklyAvgSecs: number;
  sessionCount: number;
  productiveSecs: number;
  distractingSecs: number;
  topApps: AppUsage[];
  topSites: SiteUsage[];
  siteSecs: number;
  categories: CategoryUsage[];
  week: DayTotal[];
  hourly: number[];
  live: LiveStatus;
}

export interface Report {
  from: string;
  to: string;
  activeSecs: number;
  idleSecs: number;
  dailyAvgSecs: number;
  prevActiveSecs: number;
  weekdayAvgSecs: number;
  weekendAvgSecs: number;
  productiveSecs: number;
  distractingSecs: number;
  peakHour: number | null;
  days: DayTotal[];
  apps: AppUsage[];
  sites: SiteUsage[];
  siteSecs: number;
  categories: CategoryUsage[];
  productivity: DayKinds[];
  focusSecs: number;
  focusSessions: number;
  hourly: number[];
  heatmap: number[][];
}

export interface SessionRow {
  id: number;
  appName: string;
  exeName: string;
  category: string;
  color: string;
  day: string;
  start: string;
  end: string;
  secs: number;
}

export type Theme = "system" | "light" | "dark";

export interface Settings {
  paused: boolean;
  idleThresholdSecs: number;
  autostart: boolean;
  theme: Theme;
  dataDir: string;
  notificationsEnabled: boolean;
  breakReminderMins: number;
  websiteTracking: boolean;
}

export interface DayKinds {
  day: string;
  productiveSecs: number;
  neutralSecs: number;
  distractingSecs: number;
}

export interface InstalledApp {
  exeName: string;
  name: string;
  path: string;
}

export interface FocusPlan {
  label: string;
  focusMins: number;
  breakMins: number;
  longBreakMins: number;
  rounds: number;
  blockDistracting: boolean;
}

export interface FocusStatus {
  label: string;
  phase: "focus" | "break";
  round: number;
  rounds: number;
  startedAt: string;
  endsAt: string;
  remainingSecs: number;
  totalSecs: number;
  blockDistracting: boolean;
  plan: FocusPlan;
}

export interface FocusRecord {
  id: number;
  label: string;
  day: string;
  start: string;
  end: string | null;
  plannedSecs: number;
  actualSecs: number;
  completed: boolean;
}

export interface FocusOverview {
  active: FocusStatus | null;
  todaySecs: number;
  todaySessions: number;
  weekSecs: number;
  weekSessions: number;
  streakDays: number;
  history: FocusRecord[];
}

export type GoalKind = "max_screen" | "min_category" | "max_category" | "min_focus_sessions";

export interface GoalStatus {
  id: number;
  kind: GoalKind;
  categoryId: number | null;
  category: string | null;
  color: string;
  target: number;
  enabled: boolean;
  current: number;
  state: "achieved" | "in_progress" | "on_track" | "exceeded";
  label: string;
}

export interface Insight {
  tone: "good" | "warn" | "info";
  text: string;
}
