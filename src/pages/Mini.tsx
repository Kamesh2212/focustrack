import { useEffect, useState } from "react";
import { Brain, Coffee, ExternalLink, Music, Pause, X } from "lucide-react";
import { api, isTauri } from "../lib/api";
import { useLoad, useNow } from "../lib/hooks";
import { cx } from "../components/ui";

function mmss(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

/** The small always-on-top timer that pops up during focus and study sessions. */
export default function Mini() {
  const { data } = useLoad(() => api.focus(), [], 3000);
  const now = useNow(1000);
  const [musicOn, setMusicOn] = useState(false);
  const f = data?.active ?? null;

  // Follow the app's light/dark setting.
  useEffect(() => {
    api.settings().then((s) => {
      const dark = s.theme === "dark" || (s.theme === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
      document.documentElement.classList.toggle("dark", dark);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let off: (() => void) | undefined;
    (async () => {
      const { listen, emit } = await import("@tauri-apps/api/event");
      off = await listen<{ playing: string | null }>("ft-music-state", ({ payload }) => setMusicOn(!!payload.playing));
      await emit("ft-music", { action: "state" });
    })();
    return () => off?.();
  }, []);

  const toggleMusic = async () => {
    if (!isTauri) return setMusicOn(!musicOn);
    const { emit } = await import("@tauri-apps/api/event");
    await emit("ft-music", { action: "toggle" });
  };
  const hide = async () => {
    if (!isTauri) return;
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().hide();
  };

  const left = f ? Math.max(0, (new Date(f.endsAt.replace(" ", "T")).getTime() - now) / 1000) : 0;
  const pct = f && f.totalSecs ? 1 - left / f.totalSecs : 0;
  const blocked = f?.preview ? (f.preview.mode === "allow_only" ? "Only approved apps allowed" : `${f.preview.blockedApps.length} apps · ${f.preview.blockedSites.length} sites blocked`) : "Nothing blocked";

  return (
    <div className="flex h-screen select-none flex-col overflow-hidden rounded-xl border border-slate-200 bg-white text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-100">
      <div data-tauri-drag-region className="flex cursor-move items-center gap-2 px-3 pt-2.5 text-xs text-slate-500">
        {f?.phase === "break" ? <Coffee size={13} className="text-emerald-500" /> : <Brain size={13} className="text-indigo-500" />}
        <span data-tauri-drag-region className="truncate font-medium">
          {f ? `${f.phase === "break" ? "Break" : f.mode === "study" ? "Studying" : "Focusing"} · ${f.subject || f.label}` : "No session running"}
        </span>
        <button onClick={hide} className="ml-auto rounded p-0.5 hover:bg-slate-100 dark:hover:bg-slate-800" aria-label="Hide timer" title="Hide (the session keeps going)">
          <X size={14} />
        </button>
      </div>
      <div data-tauri-drag-region className="flex flex-1 flex-col justify-center px-4">
        <div className="tabular text-4xl font-semibold tracking-tight">{f ? mmss(left) : "--:--"}</div>
        <div className="mt-1 truncate text-xs text-slate-500">
          {f && f.rounds > 1 ? `Round ${f.round} of ${f.rounds} · ` : ""}
          {f?.goalText ? `Goal: ${f.goalText}` : blocked}
        </div>
        <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
          <div className={cx("h-full rounded-full", f?.phase === "break" ? "bg-emerald-500" : "bg-indigo-500")} style={{ width: `${Math.round(pct * 100)}%`, transition: "width 1s linear" }} />
        </div>
      </div>
      <div className="flex items-center gap-2 border-t border-slate-100 px-3 py-2 dark:border-slate-800">
        <button
          onClick={toggleMusic}
          className={cx("inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs", musicOn ? "bg-indigo-600 text-white" : "bg-slate-100 dark:bg-slate-800")}
        >
          {musicOn ? <Pause size={12} /> : <Music size={12} />} {musicOn ? "Music on" : "Music"}
        </button>
        <button
          onClick={() => isTauri && api.showMainWindow()}
          className="ml-auto inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-xs hover:bg-slate-100 dark:hover:bg-slate-800"
        >
          <ExternalLink size={12} /> Open FocusTrack
        </button>
      </div>
    </div>
  );
}
