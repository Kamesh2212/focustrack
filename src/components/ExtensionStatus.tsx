import { Puzzle } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import type { BrowserStatus } from "../lib/types";
import { cx } from "./ui";

const NAMES: Record<string, string> = {
  chrome: "Chrome",
  edge: "Edge",
  brave: "Brave",
  firefox: "Firefox",
  opera: "Opera",
  vivaldi: "Vivaldi",
};

export function useBrowserStatus() {
  return useLoad<BrowserStatus>(() => api.browserStatus(), [], 10000);
}

export function InstallSteps() {
  return (
    <ol className="list-decimal space-y-1 pl-5 text-sm text-slate-600 dark:text-slate-300">
      <li>
        Open <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">chrome://extensions</code> (Edge:{" "}
        <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">edge://extensions</code>, Brave:{" "}
        <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">brave://extensions</code>).
      </li>
      <li>Turn on <b>Developer mode</b>.</li>
      <li>
        Click <b>Load unpacked</b> and choose the <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">extension</code> folder
        in the FocusTrack project.
      </li>
      <li>
        Firefox: open <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">about:debugging</code> → This Firefox → Load
        Temporary Add-on → pick <code className="rounded bg-slate-100 px-1 dark:bg-slate-800">extension/manifest.json</code>.
      </li>
    </ol>
  );
}

/** Compact banner: which browsers are connected, or how to connect one. */
export function ExtensionStatus({ status }: { status: BrowserStatus | null }) {
  if (!status) return null;
  const connected = status.browsers.filter((b) => b.connected);
  if (!status.serverRunning) {
    return (
      <div className="rounded-lg border border-rose-200 bg-rose-50 px-4 py-3 text-sm text-rose-700 dark:border-rose-900 dark:bg-rose-950 dark:text-rose-300">
        The browser connection could not start: port {status.port} is in use by another program.
      </div>
    );
  }
  if (connected.length) {
    return (
      <div className="flex flex-wrap items-center gap-2 text-sm text-slate-600 dark:text-slate-300">
        <Puzzle size={16} className="text-emerald-600" />
        Connected:
        {connected.map((b) => (
          <span key={b.browser} className="rounded-full bg-emerald-50 px-2 py-0.5 text-xs font-medium text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
            {NAMES[b.browser] ?? b.browser}
          </span>
        ))}
      </div>
    );
  }
  return (
    <div className="rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950/50">
      <div className="mb-2 flex items-center gap-2 text-sm font-medium text-amber-800 dark:text-amber-200">
        <Puzzle size={16} /> No browser extension connected yet
      </div>
      <p className="mb-2 text-sm text-slate-600 dark:text-slate-300">
        Website tracking needs the FocusTrack extension in your browser. It shares only the active tab's domain, and only
        with this app.
      </p>
      <InstallSteps />
    </div>
  );
}

export function browserLabel(b: string): string {
  return NAMES[b] ?? b;
}

export function StatusDot({ on }: { on: boolean }) {
  return <span className={cx("inline-block h-2 w-2 rounded-full", on ? "bg-emerald-500" : "bg-slate-400")} />;
}
