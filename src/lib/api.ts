import { invoke } from "@tauri-apps/api/core";
import type { AppUsage, Category, Dashboard, LiveStatus, Report, SessionRow, Settings, Theme } from "./types";
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
  history: (from: string, to: string, search: string, limit = 500) =>
    call<SessionRow[]>("get_history", { from, to, search, limit }),
  exportCsv: (from: string, to: string, search: string) => call<string>("export_csv", { from, to, search }),
  settings: () => call<Settings>("get_settings"),
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
  setIdleThreshold: (secs: number) => call<void>("set_idle_threshold", { secs }),
  setAutostart: (enabled: boolean) => call<void>("set_autostart", { enabled }),
  setTheme: (theme: Theme) => call<void>("set_theme", { theme }),
  deleteAllData: () => call<void>("delete_all_data"),
  deleteOlderThan: (days: number) => call<number>("delete_data_older_than", { days }),
};
