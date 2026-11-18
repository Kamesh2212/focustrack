import { useEffect, useRef, useState } from "react";
import { ExternalLink, Music, Pause, Play } from "lucide-react";
import { api, isTauri } from "../lib/api";
import { loadPrefs, music, savePrefs, SOUNDS, type MusicPrefs, type Sound } from "../lib/music";
import { Button, Card, Toggle, cx } from "./ui";

/** Re-renders when the player starts or stops. */
export function useMusicState() {
  const [, force] = useState(0);
  useEffect(() => {
    const off = music.onChange(() => force((n) => n + 1));
    return () => {
      off();
    };
  }, []);
  return music.playing;
}

/** Starts the chosen sound if the user wants music with sessions (call from a click). */
export function startSessionMusic() {
  const p = loadPrefs();
  if (p.autoplay) void music.play(p.sound, p.volume);
}

/**
 * Lives in the main window: lets the timer pop-up control the music, pauses it
 * during breaks and stops it when the session ends.
 */
export function useMusicController() {
  const phase = useRef<string | null>(null);
  useEffect(() => {
    if (!isTauri) return;
    let unlisten: (() => void) | undefined;
    let timer: number | undefined;
    (async () => {
      const { listen, emit } = await import("@tauri-apps/api/event");
      const report = () => emit("ft-music-state", { playing: music.playing });
      const offChange = music.onChange(report);
      const off = await listen<{ action: "toggle" | "state" }>("ft-music", ({ payload }) => {
        if (payload.action === "toggle") {
          const p = loadPrefs();
          if (music.playing) music.stop();
          else void music.play(p.sound, p.volume);
        }
        report();
      });
      unlisten = () => {
        off();
        offChange();
      };
      // Follow the session: pause during breaks, stop when it ends.
      timer = window.setInterval(async () => {
        const f = (await api.focus().catch(() => null))?.active ?? null;
        const now = f ? f.phase : null;
        const prev = phase.current;
        phase.current = now;
        const p = loadPrefs();
        if (prev && !now && music.playing) music.stop();
        if (prev === "focus" && now === "break" && p.pauseOnBreaks && music.playing) music.stop();
        if (prev === "break" && now === "focus" && p.autoplay && !music.playing) void music.play(p.sound, p.volume);
      }, 4000);
    })();
    return () => {
      unlisten?.();
      window.clearInterval(timer);
    };
  }, []);
}

export function FocusMusicCard() {
  const [prefs, setPrefs] = useState<MusicPrefs>(loadPrefs);
  const playing = useMusicState();
  const [error, setError] = useState<string | null>(null);
  const update = (p: Partial<MusicPrefs>) => {
    const next = { ...prefs, ...p };
    setPrefs(next);
    savePrefs(next);
    if (p.volume !== undefined) music.setVolume(p.volume);
    if (p.sound && playing) void music.play(p.sound, next.volume);
  };

  return (
    <Card
      title={
        <span className="inline-flex items-center gap-2">
          <Music size={15} /> Focus music
        </span>
      }
      action={
        <Button variant={playing ? "secondary" : "primary"} onClick={() => (playing ? music.stop() : void music.play(prefs.sound, prefs.volume))}>
          {playing ? <Pause size={14} /> : <Play size={14} />} {playing ? "Stop" : "Play"}
        </Button>
      }
    >
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
        {SOUNDS.map((s) => (
          <button
            key={s.id}
            type="button"
            onClick={() => update({ sound: s.id as Sound })}
            className={cx(
              "rounded-lg border px-2.5 py-2 text-left text-sm transition-colors",
              prefs.sound === s.id
                ? "border-indigo-500 bg-indigo-50 dark:bg-indigo-950/50"
                : "border-slate-200 hover:bg-slate-50 dark:border-slate-700 dark:hover:bg-slate-800",
            )}
          >
            <div className="font-medium">
              {s.name}
              {playing === s.id && <span className="ml-1.5 inline-block h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-500 align-middle" />}
            </div>
            <div className="text-xs text-slate-500">{s.desc}</div>
          </button>
        ))}
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-3 text-sm">
        <label className="flex items-center gap-2">
          Volume
          <input type="range" min={0} max={1} step={0.05} value={prefs.volume} onChange={(e) => update({ volume: Number(e.target.value) })} aria-label="Volume" />
        </label>
        <label className="flex items-center gap-2">
          <Toggle label="Play when a session starts" checked={prefs.autoplay} onChange={(v) => update({ autoplay: v })} />
          Play when a session starts
        </label>
        <label className="flex items-center gap-2">
          <Toggle label="Pause during breaks" checked={prefs.pauseOnBreaks} onChange={(v) => update({ pauseOnBreaks: v })} />
          Pause during breaks
        </label>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-2 text-sm">
        <input
          value={prefs.playlistUrl}
          onChange={(e) => update({ playlistUrl: e.target.value })}
          placeholder="Your own playlist link (Spotify, YouTube Music…)"
          className="min-w-0 flex-1 rounded-md border border-slate-200 bg-white px-2 py-1.5 dark:border-slate-700 dark:bg-slate-900"
          aria-label="Playlist link"
        />
        <Button
          disabled={!/^https?:\/\//.test(prefs.playlistUrl)}
          onClick={() => api.openUrl(prefs.playlistUrl).then(() => setError(null), (e) => setError(String(e)))}
        >
          <ExternalLink size={14} /> Open
        </Button>
      </div>
      <p className="mt-2 text-xs text-slate-500">
        {error ?? "Opens in your browser. If your blocking profile blocks that site, use the built-in sounds instead."}
      </p>
    </Card>
  );
}
