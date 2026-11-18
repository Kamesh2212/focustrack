// Mirrors the serde structs in src-tauri/src/{commands,analytics,tracker}.rs

export type TrackingState = "starting" | "active" | "idle" | "locked" | "paused";

export interface LiveStatus {
  state: TrackingState;
  appName: string | null;
  exeName: string | null;
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
  categories: CategoryUsage[];
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
}
