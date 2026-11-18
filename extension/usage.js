// Shared by the popup and the side panel: talk to the FocusTrack app and
// render widgets. No inline scripts (MV3 CSP).

const BASE = "http://127.0.0.1:17865/v1";
const HEADERS = { "X-FocusTrack": "1" };

const HOST = { origins: ["http://127.0.0.1:17865/*"] };

/** fetch with a timeout and short, user-readable errors. */
async function call(path, init = {}) {
  let res;
  try {
    res = await fetch(`${BASE}${path}`, { ...init, signal: AbortSignal.timeout(5000) });
  } catch (e) {
    throw new Error(e?.name === "TimeoutError" ? "timed out" : "can't reach 127.0.0.1:17865");
  }
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

export const fetchSummary = () => call("/summary", { headers: HEADERS });

const post = (path, body) =>
  call(path, { method: "POST", headers: { ...HEADERS, "Content-Type": "application/json" }, body: JSON.stringify(body) });

/** Whether the browser lets the extension talk to the app (Firefox can withhold this). */
export async function hasHostAccess() {
  try {
    return await chrome.permissions.contains(HOST);
  } catch {
    return true;
  }
}

/** Must be called from a click. */
export const requestHostAccess = () => chrome.permissions.request(HOST);

export const toggleGoal = (id) => post("/goals/toggle", { id });
export const saveQuickNote = (text) => post("/notes", { text });

export function fmt(secs) {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return m > 0 ? `${h}h ${m}m` : `${h}h`;
  if (m > 0) return `${m}m`;
  return `${s}s`;
}

export function fmtClock(secs) {
  const s = Math.max(0, Math.round(secs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = String(s % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${r}` : `${m}:${r}`;
}

export function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "style") Object.assign(n.style, v);
    else if (k === "class") n.className = v;
    else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v);
  }
  for (const c of children) if (c != null) n.append(c);
  return n;
}

function bar(pct, cls, color) {
  return el("div", { class: "bar" }, el("div", { class: cls ?? "", style: { width: `${Math.max(3, pct)}%`, ...(color ? { background: color } : {}) } }));
}

function rows(items, empty, make) {
  if (!items.length) return [el("p", { class: "muted" }, empty)];
  return items.map(make);
}

// ------------------------------------------------------------------ widgets

export function screenTime(s) {
  return [
    el(
      "div",
      { class: "stats" },
      el("div", {}, el("span", { class: "label" }, "Screen time"), el("span", { class: "big" }, fmt(s.screenSecs))),
      el("div", {}, el("span", { class: "label" }, "On the web"), el("span", { class: "big" }, fmt(s.webSecs))),
      el("div", {}, el("span", { class: "label" }, "Active"), el("span", { class: "mid" }, fmt(s.activeSecs))),
      el("div", {}, el("span", { class: "label" }, "Focused"), el("span", { class: "mid" }, fmt(s.focusTodaySecs))),
    ),
  ];
}

export function websites(s, limit = 6) {
  const shown = s.topSites.slice(0, limit);
  const max = shown[0]?.secs || 1;
  return rows(shown, "No websites tracked yet today.", (x) =>
    el("div", { class: "row" }, el("span", { class: "name" }, x.domain), el("span", { class: "time" }, fmt(x.secs)), bar((x.secs / max) * 100, null, x.color)),
  );
}

export function apps(s, limit = 6) {
  const shown = s.topApps.slice(0, limit);
  const max = shown[0]?.secs || 1;
  return rows(shown, "No app usage yet today.", (x) =>
    el("div", { class: "row" }, el("span", { class: "name" }, x.name), el("span", { class: "time" }, fmt(x.secs)), bar((x.secs / max) * 100, null, x.color)),
  );
}

export function limits(s) {
  return rows(s.limits, "No limits set. Add them in the FocusTrack app.", (l) => {
    const allowed = l.dailyLimitSecs + (l.extraSecs ?? 0);
    const pct = Math.min(100, (l.usedSecs / allowed) * 100);
    const over = l.usedSecs >= allowed;
    const state = over ? "over" : pct >= l.warnPct ? "warn" : "ok";
    const text = over ? (l.action === "notify" ? "reached" : "blocked") : `${fmt(allowed - l.usedSecs)} left`;
    return el("div", { class: "row" }, el("span", { class: "name" }, l.name), el("span", { class: `time ${state}` }, text), bar(pct, `fill-${state}`));
  });
}

/** Goals, with checkboxes for custom (to-do style) goals. */
export function goals(s, onChange) {
  return rows(s.goals, "No goals yet. Add them on the FocusTrack dashboard.", (g) => {
    if (g.kind === "custom") {
      const box = el("input", { type: "checkbox", "aria-label": g.label });
      box.checked = g.state === "achieved";
      box.addEventListener("change", async () => {
        box.disabled = true;
        try {
          await toggleGoal(g.id);
        } finally {
          onChange?.();
        }
      });
      return el("label", { class: `todo ${box.checked ? "done" : ""}` }, box, el("span", {}, g.label));
    }
    const timeGoal = g.kind !== "min_focus_sessions";
    const pct = Math.min(100, (g.current / Math.max(1, g.target)) * 100);
    const state = g.state === "exceeded" ? "over" : g.state === "achieved" ? "done" : "ok";
    const progress = timeGoal ? `${fmt(g.current)} / ${fmt(g.target)}` : `${g.current} / ${g.target}`;
    return el(
      "div",
      { class: "row" },
      el("span", { class: "name" }, g.label),
      el("span", { class: `time ${state}` }, g.state === "achieved" ? "done ✓" : progress),
      bar(pct, `fill-${state}`),
    );
  });
}

let focusTimer = null;
export function focus(s) {
  clearInterval(focusTimer);
  const f = s.focus;
  if (!f) return [el("p", { class: "muted" }, "No session running. Start one from FocusTrack → Focus Mode or Study.")];
  const end = new Date(f.endsAt.replace(" ", "T")).getTime();
  const time = el("span", { class: "big" });
  const tick = () => (time.textContent = fmtClock((end - Date.now()) / 1000));
  tick();
  focusTimer = setInterval(tick, 1000);
  const what = f.phase === "focus" ? (f.mode === "study" ? "Studying" : "Focusing") : "Break";
  return [
    el(
      "div",
      { class: "focus-banner" },
      el("div", {}, `${what}: ${f.subject || f.label}`),
      time,
      el("div", { class: "small" }, [f.rounds > 1 ? `Round ${f.round} of ${f.rounds}` : null, f.profileName ? `${f.profileName} on` : null].filter(Boolean).join(" · ")),
      f.goalText ? el("div", { class: "small" }, `Goal: ${f.goalText}`) : null,
    ),
  ];
}

export function productivity(s) {
  const total = s.productiveSecs + s.neutralSecs + s.distractingSecs;
  if (!total) return [el("p", { class: "muted" }, "Nothing tracked yet today.")];
  const pct = (x) => (x / total) * 100;
  return [
    el(
      "div",
      { class: "split" },
      el("div", { class: "fill-done", style: { width: `${pct(s.productiveSecs)}%` } }),
      el("div", { class: "neutral", style: { width: `${pct(s.neutralSecs)}%` } }),
      el("div", { class: "fill-over", style: { width: `${pct(s.distractingSecs)}%` } }),
    ),
    el(
      "div",
      { class: "legend" },
      el("span", {}, `Productive ${fmt(s.productiveSecs)}`),
      el("span", {}, `Neutral ${fmt(s.neutralSecs)}`),
      el("span", {}, `Distracting ${fmt(s.distractingSecs)}`),
    ),
  ];
}

export function quickNote() {
  const area = el("textarea", { rows: "3", placeholder: "Jot something down… (saved to Quick notes in FocusTrack)", "aria-label": "Quick note" });
  const status = el("span", { class: "small muted" });
  const save = el("button", { type: "button", class: "btn" }, "Save note");
  save.addEventListener("click", async () => {
    const text = area.value.trim();
    if (!text) return;
    save.disabled = true;
    try {
      await saveQuickNote(text);
      area.value = "";
      status.textContent = "Saved ✓";
    } catch {
      status.textContent = "Couldn't save — is FocusTrack running?";
    } finally {
      save.disabled = false;
      setTimeout(() => (status.textContent = ""), 3000);
    }
  });
  return [area, el("div", { class: "actions" }, save, status)];
}

export const WIDGETS = [
  { id: "screen", title: "Today's screen time", render: screenTime },
  { id: "goals", title: "Today's goals & to-dos", render: goals },
  { id: "focus", title: "Focus session", render: focus },
  { id: "websites", title: "Website usage", render: websites },
  { id: "apps", title: "App usage", render: apps },
  { id: "productivity", title: "Productivity summary", render: productivity },
  { id: "limits", title: "Usage limits", render: limits },
  { id: "note", title: "Quick note", render: quickNote, static: true },
];

export const DEFAULT_LAYOUT = [
  { id: "screen", on: true },
  { id: "goals", on: true },
  { id: "focus", on: true },
  { id: "note", on: true },
  { id: "websites", on: false },
  { id: "apps", on: false },
  { id: "productivity", on: false },
  { id: "limits", on: false },
];

export async function loadLayout() {
  const { layout } = await chrome.storage.sync.get("layout");
  if (!Array.isArray(layout)) return DEFAULT_LAYOUT.map((x) => ({ ...x }));
  const known = new Set(WIDGETS.map((w) => w.id));
  const out = layout.filter((x) => known.has(x.id));
  for (const d of DEFAULT_LAYOUT) if (!out.some((x) => x.id === d.id)) out.push({ ...d, on: false });
  return out;
}

export const saveLayout = (layout) => chrome.storage.sync.set({ layout });
