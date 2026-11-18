// Shared by the popup and the new-tab page: fetch today's summary from the
// FocusTrack app and render it. No inline scripts (MV3 CSP).

const SUMMARY_API = "http://127.0.0.1:17865/v1/summary";

export async function fetchSummary() {
  const res = await fetch(SUMMARY_API, { headers: { "X-FocusTrack": "1" } });
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

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
  const m = Math.floor(s / 60);
  return `${m}:${String(s % 60).padStart(2, "0")}`;
}

function el(tag, attrs = {}, ...children) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "style") Object.assign(n.style, v);
    else if (k === "class") n.className = v;
    else n.setAttribute(k, v);
  }
  for (const c of children) n.append(c);
  return n;
}

export function renderSites(container, sites, limit = 6) {
  container.replaceChildren();
  const shown = sites.slice(0, limit);
  if (!shown.length) {
    container.append(el("p", { class: "muted" }, "No websites tracked yet today."));
    return;
  }
  const max = shown[0].secs || 1;
  for (const s of shown) {
    container.append(
      el(
        "div",
        { class: "row" },
        el("span", { class: "name" }, s.domain),
        el("span", { class: "time" }, fmt(s.secs)),
        el("div", { class: "bar" }, el("div", { style: { width: `${Math.max(3, (s.secs / max) * 100)}%`, background: s.color } })),
      ),
    );
  }
}

export function renderLimits(container, limits) {
  container.replaceChildren();
  if (!limits.length) {
    container.append(el("p", { class: "muted" }, "No limits set. Add them in the FocusTrack app."));
    return;
  }
  for (const l of limits) {
    const pct = Math.min(100, (l.usedSecs / l.dailyLimitSecs) * 100);
    const over = l.usedSecs >= l.dailyLimitSecs;
    const state = over ? "over" : pct >= l.warnPct ? "warn" : "ok";
    container.append(
      el(
        "div",
        { class: "row" },
        el("span", { class: "name" }, l.name),
        el("span", { class: `time ${state}` }, over ? (l.block ? "blocked" : "reached") : `${fmt(l.dailyLimitSecs - l.usedSecs)} left`),
        el("div", { class: "bar" }, el("div", { class: `fill-${state}`, style: { width: `${Math.max(3, pct)}%` } })),
      ),
    );
  }
}

export function renderGoals(container, goals) {
  container.replaceChildren();
  if (!goals.length) {
    container.append(el("p", { class: "muted" }, "No goals set. Add them in FocusTrack → Focus Mode."));
    return;
  }
  for (const g of goals) {
    const timeGoal = g.kind !== "min_focus_sessions";
    const pct = Math.min(100, (g.current / Math.max(1, g.target)) * 100);
    const state = g.state === "exceeded" ? "over" : g.state === "achieved" ? "done" : "ok";
    const progress = timeGoal ? `${fmt(g.current)} / ${fmt(g.target)}` : `${g.current} / ${g.target}`;
    container.append(
      el(
        "div",
        { class: "row" },
        el("span", { class: "name" }, g.label),
        el("span", { class: `time ${state}` }, g.state === "achieved" ? "done ✓" : progress),
        el("div", { class: "bar" }, el("div", { class: `fill-${state}`, style: { width: `${Math.max(3, pct)}%` } })),
      ),
    );
  }
}
