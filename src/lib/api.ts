import { invoke } from "@tauri-apps/api/core";
import type {
  AdminMode,
  AppUsage,
  BrowserStatus,
  Category,
  Dashboard,
  FocusOverview,
  FocusPlan,
  FocusStatus,
  GoalKind,
  GoalStatus,
  HistoryKind,
  Insight,
  InstalledApp,
  BlockPreview,
  LimitAction,
  Note,
  Notebook,
  Profile,
  Resource,
  LimitStatus,
  LiveStatus,
  Report,
  SessionRow,
  Settings,
  SiteUsage,
  SyncDevice,
  SyncStatus,
  Theme,
} from "./types";
import { mock } from "./mock";

/** True inside the Tauri webview; false in a plain browser (`npm run dev`). */
export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) return mock<T>(cmd, args ?? {});
  return invoke<T>(cmd, args);
}

export const api = {
  /** `device`: "all", "desktop", "mobile" or a device id. */
  dashboard: (device = "all") => call<Dashboard>("get_dashboard", { device }),
  liveStatus: () => call<LiveStatus>("get_live_status"),
  apps: (from: string, to: string) => call<AppUsage[]>("get_apps", { from, to }),
  categories: () => call<Category[]>("get_categories"),
  setAppCategory: (appId: number, categoryId: number | null) =>
    call<void>("set_app_category", { appId, categoryId }),
  renameApp: (appId: number, name: string) => call<void>("rename_app", { appId, name }),
  report: (from: string, to: string, device = "all") => call<Report>("get_report", { from, to, device }),
  history: (from: string, to: string, search: string, kind: HistoryKind, limit = 500) =>
    call<SessionRow[]>("get_history", { from, to, search, limit, kind }),
  exportCsv: (from: string, to: string, search: string, kind: HistoryKind) =>
    call<string>("export_csv", { from, to, search, kind }),
  websites: (from: string, to: string) => call<SiteUsage[]>("get_websites", { from, to }),
  setWebsiteCategory: (websiteId: number, categoryId: number | null) =>
    call<void>("set_website_category", { websiteId, categoryId }),
  browserStatus: () => call<BrowserStatus>("get_browser_status"),
  limits: () => call<LimitStatus[]>("get_limits"),
  setLimit: (l: { targetType: "app" | "website"; targetId: number; dailyLimitSecs: number; warnPct: number; action: LimitAction; enabled: boolean }) =>
    call<void>("set_limit", l),
  overrideLimit: (id: number) => call<void>("override_limit", { id }),
  setOverridesPerDay: (count: number) => call<void>("set_overrides_per_day", { count }),
  setGoalReminder: (time: string) => call<void>("set_goal_reminder", { time }),
  setDashboardLayout: (layout: string) => call<void>("set_dashboard_layout", { layout }),
  deleteLimit: (id: number) => call<void>("delete_limit", { id }),
  setNotificationsEnabled: (enabled: boolean) => call<void>("set_notifications_enabled", { enabled }),
  setBreakReminder: (mins: number) => call<void>("set_break_reminder", { mins }),
  setWebsiteTracking: (enabled: boolean) => call<void>("set_website_tracking", { enabled }),
  testNotification: () => call<void>("send_test_notification"),
  installedApps: () => call<InstalledApp[]>("get_installed_apps"),
  ensureApp: (a: InstalledApp) => call<number>("ensure_app", { exeName: a.exeName, name: a.name, path: a.path }),
  focus: (mode?: "focus" | "study") => call<FocusOverview>("get_focus", { mode: mode ?? null }),
  startFocus: (plan: FocusPlan) => call<FocusStatus>("start_focus", { plan }),
  stopFocus: (phrase?: string) => call<void>("stop_focus", { phrase: phrase ?? null }),
  requestStopFocus: () => call<number>("request_stop_focus"),
  previewProfile: (profileId: number) => call<BlockPreview>("preview_profile", { profileId }),
  profiles: () => call<Profile[]>("get_profiles"),
  saveProfile: (profile: Profile) => call<number>("save_profile", { profile }),
  deleteProfile: (id: number) => call<void>("delete_profile", { id }),
  notebooks: () => call<Notebook[]>("get_notebooks"),
  notes: (notebook?: string | null, search?: string) => call<Note[]>("get_notes", { notebook: notebook ?? null, search: search ?? null }),
  createNote: (notebook: string, title: string, body: string) => call<Note>("create_note", { notebook, title, body }),
  updateNote: (id: number, notebook: string, title: string, body: string) => call<Note>("update_note", { id, notebook, title, body }),
  deleteNote: (id: number) => call<void>("delete_note", { id }),
  resources: () => call<Resource[]>("get_resources"),
  addResource: (title: string, url: string, subject: string | null) => call<void>("add_resource", { title, url, subject }),
  deleteResource: (id: number) => call<void>("delete_resource", { id }),
  openUrl: (url: string) => call<void>("open_url", { url }),
  studyTime: () => call<[number, number]>("get_study_time"),
  adminMode: () => call<AdminMode>("get_admin_mode"),
  showMainWindow: () => call<void>("show_main_window"),
  enableAdminMode: () => call<void>("enable_admin_mode"),
  disableAdminMode: () => call<void>("disable_admin_mode"),
  /** "windows" | "android" | … */
  platform: () => call<string>("get_platform"),
  insets: () => call<{ top: number; bottom: number }>("get_insets"),
  phoneUsageAccess: () => call<boolean>("phone_usage_access"),
  openUsageAccess: () => call<void>("open_usage_access"),
  /** New sessions imported, or null without usage access. */
  importPhoneUsage: () => call<number | null>("import_phone_usage"),
  syncStatus: () => call<SyncStatus>("sync_status"),
  /** Returns the recovery key, to show once. */
  syncSignUp: (server: string, email: string, password: string, deviceName: string) =>
    call<string>("sync_sign_up", { server, email, password, deviceName }),
  syncSignIn: (server: string, email: string, password: string, deviceName: string) =>
    call<void>("sync_sign_in", { server, email, password, deviceName }),
  syncSignOut: () => call<void>("sync_sign_out"),
  syncNow: () => call<void>("sync_now"),
  syncSetEnabled: (enabled: boolean) => call<void>("sync_set_enabled", { enabled }),
  syncDevices: () => call<SyncDevice[]>("sync_devices"),
  syncRemoveDevice: (id: string) => call<void>("sync_remove_device", { id }),
  syncRenameDevice: (name: string) => call<void>("sync_rename_device", { name }),
  /** Deletes synced history everywhere; `before` = "YYYY-MM-DD" or null for all. Returns records removed. */
  syncDeleteHistory: (before: string | null, device: string | null) => call<number>("sync_delete_history", { before, device }),
  syncDeleteAccount: (password: string) => call<void>("sync_delete_account", { password }),
  syncRetention: () => call<number | null>("sync_retention"),
  syncSetRetention: (days: number | null) => call<number | null>("sync_set_retention", { days }),
  /** Writes a decrypted JSON export to Downloads; returns its path. */
  syncExport: () => call<string>("sync_export"),
  toggleGoal: (id: number) => call<boolean>("toggle_goal", { id }),
  goals: () => call<GoalStatus[]>("get_goals"),
  setGoal: (g: { id: number | null; kind: GoalKind; categoryId: number | null; target: number; enabled: boolean; title: string | null }) =>
    call<void>("set_goal", g),
  deleteGoal: (id: number) => call<void>("delete_goal", { id }),
  insights: () => call<Insight[]>("get_insights"),
  settings: () => call<Settings>("get_settings"),
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
  setIdleThreshold: (secs: number) => call<void>("set_idle_threshold", { secs }),
  setAutostart: (enabled: boolean) => call<void>("set_autostart", { enabled }),
  setTheme: (theme: Theme) => call<void>("set_theme", { theme }),
  deleteAllData: () => call<void>("delete_all_data"),
  deleteOlderThan: (days: number) => call<number>("delete_data_older_than", { days }),
};
