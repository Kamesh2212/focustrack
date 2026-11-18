import { invoke } from "@tauri-apps/api/core";
import type {
  AppUsage,
  BrowserStatus,
  Category,
  Dashboard,
  HistoryKind,
  LimitStatus,
  LiveStatus,
  Report,
  SessionRow,
  Settings,
  SiteUsage,
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
  dashboard: () => call<Dashboard>("get_dashboard"),
  liveStatus: () => call<LiveStatus>("get_live_status"),
  apps: (from: string, to: string) => call<AppUsage[]>("get_apps", { from, to }),
  categories: () => call<Category[]>("get_categories"),
  setAppCategory: (appId: number, categoryId: number | null) =>
    call<void>("set_app_category", { appId, categoryId }),
  renameApp: (appId: number, name: string) => call<void>("rename_app", { appId, name }),
  report: (from: string, to: string) => call<Report>("get_report", { from, to }),
  history: (from: string, to: string, search: string, kind: HistoryKind, limit = 500) =>
    call<SessionRow[]>("get_history", { from, to, search, limit, kind }),
  exportCsv: (from: string, to: string, search: string, kind: HistoryKind) =>
    call<string>("export_csv", { from, to, search, kind }),
  websites: (from: string, to: string) => call<SiteUsage[]>("get_websites", { from, to }),
  setWebsiteCategory: (websiteId: number, categoryId: number | null) =>
    call<void>("set_website_category", { websiteId, categoryId }),
  browserStatus: () => call<BrowserStatus>("get_browser_status"),
  limits: () => call<LimitStatus[]>("get_limits"),
  setLimit: (l: { targetType: "app" | "website"; targetId: number; dailyLimitSecs: number; warnPct: number; block: boolean; enabled: boolean }) =>
    call<void>("set_limit", l),
  deleteLimit: (id: number) => call<void>("delete_limit", { id }),
  setNotificationsEnabled: (enabled: boolean) => call<void>("set_notifications_enabled", { enabled }),
  setBreakReminder: (mins: number) => call<void>("set_break_reminder", { mins }),
  setWebsiteTracking: (enabled: boolean) => call<void>("set_website_tracking", { enabled }),
  testNotification: () => call<void>("send_test_notification"),
  settings: () => call<Settings>("get_settings"),
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
  setIdleThreshold: (secs: number) => call<void>("set_idle_threshold", { secs }),
  setAutostart: (enabled: boolean) => call<void>("set_autostart", { enabled }),
  setTheme: (theme: Theme) => call<void>("set_theme", { theme }),
  deleteAllData: () => call<void>("delete_all_data"),
  deleteOlderThan: (days: number) => call<number>("delete_data_older_than", { days }),
};
