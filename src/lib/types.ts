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

export type LimitAction = "notify" | "block" | "warn_then_block";

export interface LimitStatus {
  id: number;
  targetType: "app" | "website";
  targetId: number;
  name: string;
  key: string;
  color: string;
  dailyLimitSecs: number;
  warnPct: number;
  action: LimitAction;
  enabled: boolean;
  usedSecs: number;
  extraSecs: number;
  overridesUsed: number;
  /** App limits: exe path ("" if unknown). */
  path: string;
  /** Launcher limits: the folder whose programs this limit also covers. */
  scope: string | null;
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
  uniqueActiveSecs: number;
  devices: DeviceTotal[];
  devicesUniqueSecs: number;
}

/** One device's screen time (Account & Sync). */
export interface DeviceTotal {
  id: string;
  name: string;
  platform: string;
  activeSecs: number;
  idleSecs: number;
  lastActivity: string | null;
  current: boolean;
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
  uniqueActiveSecs: number;
  devices: DeviceTotal[];
  devicesUniqueSecs: number;
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
  overridesPerDay: number;
  goalReminder: string;
  dashboardLayout: string;
}

/** Multi-device sync status (Settings → Account & Sync). */
export interface SyncStatus {
  signedIn: boolean;
  enabled: boolean;
  email: string | null;
  server: string | null;
  deviceId: string;
  deviceName: string;
  state: "off" | "idle" | "syncing" | "error" | "signed_out";
  lastSync: string | null;
  lastError: string | null;
  pending: number;
  otherDevices: number;
}

export interface SyncDevice {
  id: string;
  name: string;
  platform: string;
  appVersion: string;
  /** Last time it talked to the server (≈ last sync). */
  lastSeen: string | null;
  online: boolean;
  lastActivity: string | null;
  paused: boolean;
  syncError: string | null;
  /** Sync was turned off for it from another device. */
  syncDisabled: boolean;
  current: boolean;
}

export interface AdminMode {
  elevated: boolean;
  enabled: boolean;
  needsAdmin: string[];
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
  profileId: number | null;
  mode: "focus" | "study";
  subject: string | null;
  goalText: string | null;
  goalId: number | null;
}

export interface FocusStatus {
  label: string;
  mode: "focus" | "study";
  subject: string | null;
  goalText: string | null;
  phase: "focus" | "break";
  round: number;
  rounds: number;
  startedAt: string;
  endsAt: string;
  planEndsAt: string;
  remainingSecs: number;
  totalSecs: number;
  profileName: string | null;
  blocking: boolean;
  preview: BlockPreview | null;
  endEarly: EndEarly;
  stopWaitSecs: number | null;
  plan: FocusPlan;
}

export interface FocusRecord {
  id: number;
  label: string;
  mode: "focus" | "study";
  subject: string | null;
  goalText: string | null;
  goalId: number | null;
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
  stopPhrase: string;
}

export type GoalKind = "max_screen" | "min_category" | "max_category" | "min_focus_sessions" | "custom";

export interface GoalStatus {
  id: number;
  kind: GoalKind;
  categoryId: number | null;
  category: string | null;
  color: string;
  target: number;
  title: string | null;
  enabled: boolean;
  current: number;
  state: "achieved" | "in_progress" | "on_track" | "exceeded";
  label: string;
}

export interface Insight {
  tone: "good" | "warn" | "info";
  text: string;
}

export type EndEarly = "allow" | "confirm" | "strict";

export interface Profile {
  id: number | null;
  name: string;
  mode: "block" | "allow_only";
  categories: string[];
  apps: string[];
  sites: string[];
  unblockOnBreaks: boolean;
  endEarly: EndEarly;
  builtin: boolean;
}

export interface AppRef {
  name: string;
  exeName: string;
  category: string;
}

export interface BlockPreview {
  mode: "block" | "allow_only";
  blockedApps: AppRef[];
  blockedSites: string[];
  allowedApps: AppRef[];
  allowedSites: string[];
}

export interface Note {
  id: number;
  notebook: string;
  title: string;
  body: string;
  focusSessionId: number | null;
  sessionLabel: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface Notebook {
  name: string;
  count: number;
  updatedAt: string;
}

export interface Resource {
  id: number;
  title: string;
  url: string;
  subject: string | null;
}
