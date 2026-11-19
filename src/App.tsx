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
  GraduationCap,
  NotebookPen,
  Menu,
  X,
  type LucideIcon,
} from "lucide-react";
import { api, isTauri } from "./lib/api";
import type { Theme } from "./lib/types";
import { cx } from "./components/ui";
import { isPhone, usePlatform } from "./lib/platform";
import Dashboard from "./pages/Dashboard";
import Applications from "./pages/Applications";
import Reports from "./pages/Reports";
import History from "./pages/History";
import Websites from "./pages/Websites";
import Limits from "./pages/Limits";
import Focus from "./pages/Focus";
import Study from "./pages/Study";
import Notes from "./pages/Notes";
import { useMusicController } from "./components/FocusMusic";
import Settings from "./pages/Settings";

type PageId = "dashboard" | "applications" | "websites" | "reports" | "focus" | "study" | "notes" | "limits" | "history" | "settings";

const NAV: { id: PageId; label: string; icon: LucideIcon }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "applications", label: "Applications", icon: AppWindow },
  { id: "websites", label: "Websites", icon: Globe },
  { id: "reports", label: "Reports", icon: BarChart3 },
  { id: "focus", label: "Focus Mode", icon: Timer },
  { id: "study", label: "Study", icon: GraduationCap },
  { id: "notes", label: "Notes", icon: NotebookPen },
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
  const [menuOpen, setMenuOpen] = useState(false);
  const phone = isPhone(usePlatform());
  useMusicController();

  // Keep clear of the phone's status and navigation bars.
  useEffect(() => {
    if (!phone) return;
    const apply = () =>
      api
        .insets()
        .then(({ top, bottom }) => {
          document.documentElement.style.setProperty("--ft-inset-top", `${top}px`);
          document.documentElement.style.setProperty("--ft-inset-bottom", `${bottom}px`);
        })
        .catch(() => {});
    apply();
    // The first measurement can arrive just after start-up.
    const again = window.setTimeout(apply, 800);
    window.addEventListener("resize", apply);
    return () => {
      window.clearTimeout(again);
      window.removeEventListener("resize", apply);
    };
  }, [phone]);
  const [, setTheme] = useTheme();

  const go = (id: PageId) => {
    setPage(id);
    setMenuOpen(false);
    window.history.replaceState(null, "", `#${id}`);
    document.querySelector("main")?.scrollTo({ top: 0 });
  };

  // Dashboard widgets can open other pages.
  useEffect(() => {
    const onNav = (e: Event) => {
      const id = (e as CustomEvent<string>).detail as PageId;
      if (NAV.some((n) => n.id === id)) go(id);
    };
    window.addEventListener("ft-navigate", onNav);
    return () => window.removeEventListener("ft-navigate", onNav);
  }, []);

  const navList = (
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
  );
  const logo = (
    <div className="flex items-center gap-2.5">
      <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-indigo-600 text-white">
        <Timer size={18} />
      </div>
      <span className="text-lg font-semibold tracking-tight">FocusTrack</span>
    </div>
  );
  const footer = `v${__APP_VERSION__} · ${isTauri ? (phone ? "All data stays on this phone" : "All data stays on this PC") : "Browser preview (sample data)"}`;

  return (
    <div className="flex h-full flex-col md:flex-row">
      {/* Phones and narrow windows: a top bar and a slide-in menu. */}
      <header className="flex items-center justify-between border-b border-slate-200 bg-white px-4 pb-2.5 pt-[max(0.625rem,var(--ft-inset-top,env(safe-area-inset-top)))] md:hidden dark:border-slate-800 dark:bg-slate-900">
        {logo}
        <button
          type="button"
          onClick={() => setMenuOpen(true)}
          className="rounded-lg p-2 text-slate-600 hover:bg-slate-100 dark:text-slate-300 dark:hover:bg-slate-800"
          aria-label="Open menu"
          aria-expanded={menuOpen}
        >
          <Menu size={20} />
        </button>
      </header>
      {menuOpen && (
        <div className="fixed inset-0 z-40 md:hidden" role="dialog" aria-modal="true" aria-label="Menu">
          <div className="absolute inset-0 bg-slate-900/40" onClick={() => setMenuOpen(false)} />
          <nav className="absolute inset-y-0 left-0 flex w-64 max-w-[80%] flex-col bg-white px-3 pb-[max(1rem,var(--ft-inset-bottom,env(safe-area-inset-bottom)))] pt-[max(1rem,var(--ft-inset-top,env(safe-area-inset-top)))] shadow-xl dark:bg-slate-900">
            <div className="mb-5 flex items-center justify-between px-2">
              {logo}
              <button type="button" onClick={() => setMenuOpen(false)} className="rounded-lg p-1.5 text-slate-500 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label="Close menu">
                <X size={18} />
              </button>
            </div>
            {navList}
            <div className="mt-auto px-2 pt-4 text-xs text-slate-400">{footer}</div>
          </nav>
        </div>
      )}
      <nav className="hidden w-56 shrink-0 flex-col border-r border-slate-200 bg-white px-3 py-5 md:flex dark:border-slate-800 dark:bg-slate-900">
        <div className="mb-6 px-2">{logo}</div>
        {navList}
        <div className="mt-auto px-2 text-xs text-slate-400">{footer}</div>
      </nav>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-6xl px-4 pt-5 pb-[max(1.25rem,var(--ft-inset-bottom,0px))] md:px-8 md:py-7">
          {page === "dashboard" && <Dashboard />}
          {page === "applications" && <Applications />}
          {page === "reports" && <Reports />}
          {page === "history" && <History />}
          {page === "settings" && <Settings onThemeChange={setTheme} />}
          {page === "websites" && <Websites />}
          {page === "limits" && <Limits />}
          {page === "focus" && <Focus />}
          {page === "study" && <Study />}
          {page === "notes" && <Notes />}
        </div>
      </main>
    </div>
  );
}
