import { useEffect, useState } from "react";
import {
  AppWindow,
  BarChart3,
  Globe,
  History as HistoryIcon,
  LayoutDashboard,
  Settings as SettingsIcon,
  Target,
  Timer,
  type LucideIcon,
} from "lucide-react";
import { api, isTauri } from "./lib/api";
import type { Theme } from "./lib/types";
import { cx } from "./components/ui";
import Dashboard from "./pages/Dashboard";
import Applications from "./pages/Applications";
import Reports from "./pages/Reports";
import History from "./pages/History";
import Websites from "./pages/Websites";
import Limits from "./pages/Limits";
import Settings from "./pages/Settings";
import ComingSoon from "./pages/ComingSoon";

type PageId = "dashboard" | "applications" | "websites" | "reports" | "focus" | "limits" | "history" | "settings";

const NAV: { id: PageId; label: string; icon: LucideIcon }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "applications", label: "Applications", icon: AppWindow },
  { id: "websites", label: "Websites", icon: Globe },
  { id: "reports", label: "Reports", icon: BarChart3 },
  { id: "focus", label: "Focus Mode", icon: Timer },
  { id: "limits", label: "Usage Limits", icon: Target },
  { id: "history", label: "History", icon: HistoryIcon },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setTheme] = useState<Theme>("system");

  useEffect(() => {
    api.settings().then((s) => setTheme(s.theme)).catch(() => {});
  }, []);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = theme === "dark" || (theme === "system" && media.matches);
      document.documentElement.classList.toggle("dark", dark);
    };
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme]);

  return [theme, setTheme];
}

function initialPage(): PageId {
  const hash = window.location.hash.slice(1) as PageId;
  return NAV.some((n) => n.id === hash) ? hash : "dashboard";
}

export default function App() {
  const [page, setPage] = useState<PageId>(initialPage);
  const [, setTheme] = useTheme();

  const go = (id: PageId) => {
    setPage(id);
    window.history.replaceState(null, "", `#${id}`);
    document.querySelector("main")?.scrollTo({ top: 0 });
  };

  return (
    <div className="flex h-full">
      <nav className="flex w-56 shrink-0 flex-col border-r border-slate-200 bg-white px-3 py-5 dark:border-slate-800 dark:bg-slate-900">
        <div className="mb-6 flex items-center gap-2.5 px-2">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-600 text-white">
            <Timer size={18} />
          </div>
          <span className="text-lg font-semibold tracking-tight">FocusTrack</span>
        </div>
        <ul className="space-y-0.5">
          {NAV.map(({ id, label, icon: Icon }) => (
            <li key={id}>
              <button
                type="button"
                onClick={() => go(id)}
                aria-current={page === id ? "page" : undefined}
                className={cx(
                  "flex w-full items-center gap-3 rounded-lg px-3 py-2 text-sm transition-colors",
                  page === id
                    ? "bg-indigo-50 font-medium text-indigo-700 dark:bg-indigo-950/60 dark:text-indigo-300"
                    : "text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800",
                )}
              >
                <Icon size={17} />
                {label}
              </button>
            </li>
          ))}
        </ul>
        <div className="mt-auto px-2 text-xs text-slate-400">
          v0.1.0 · {isTauri ? "All data stays on this PC" : "Browser preview (sample data)"}
        </div>
      </nav>

      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-6xl px-8 py-7">
          {page === "dashboard" && <Dashboard />}
          {page === "applications" && <Applications />}
          {page === "reports" && <Reports />}
          {page === "history" && <History />}
          {page === "settings" && <Settings onThemeChange={setTheme} />}
          {page === "websites" && <Websites />}
          {page === "limits" && <Limits />}
          {page === "focus" && (
            <ComingSoon
              title="Focus Mode"
              phase="Phase 4 · Productivity"
              icon={Timer}
              summary="Timed focus sessions with breaks, history and optional blocking of distracting sites."
              features={[
                "25-minute Pomodoro",
                "50-minute deep work",
                "90-minute study session",
                "Custom durations and breaks",
                "Focus session history and totals",
                "Session-complete notifications",
              ]}
            />
          )}
        </div>
      </main>
    </div>
  );
}
