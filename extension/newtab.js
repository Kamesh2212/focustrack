import { fetchSummary, fmt, fmtClock, renderGoals, renderLimits, renderSites } from "./usage.js";

const $ = (id) => document.getElementById(id);

function renderClock() {
  const now = new Date();
  $("time").textContent = now.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  $("date").textContent = now.toLocaleDateString(undefined, { weekday: "long", day: "numeric", month: "long" });
}

// Uses the browser's own default search engine.
$("search").addEventListener("submit", (e) => {
  e.preventDefault();
  const text = $("q").value.trim();
  if (!text) return;
  if (globalThis.browser?.search?.search) browser.search.search({ query: text });
  else chrome.search.query({ text, disposition: "CURRENT_TAB" });
});

let focusTimer = null;
function renderFocus(focus) {
  const box = $("focus");
  clearInterval(focusTimer);
  if (!focus) {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  const end = new Date(focus.endsAt.replace(" ", "T")).getTime();
  const tick = () => {
    const what = focus.phase === "focus" ? `Focusing on ${focus.label}` : "On a break";
    const round = focus.rounds > 1 ? ` · round ${focus.round} of ${focus.rounds}` : "";
    box.textContent = `${what}${round} — ${fmtClock((end - Date.now()) / 1000)} left`;
  };
  tick();
  focusTimer = setInterval(tick, 1000);
}

async function renderUsage() {
  try {
    const s = await fetchSummary();
    $("offline").hidden = true;
    $("usage").hidden = false;
    $("screen").textContent = fmt(s.screenSecs);
    $("active").textContent = fmt(s.activeSecs);
    $("web").textContent = fmt(s.webSecs);
    $("focused").textContent = fmt(s.focusTodaySecs);
    renderSites($("sites"), s.topSites, 6);
    renderLimits($("limits"), s.limits);
    renderGoals($("goals"), s.goals);
    renderFocus(s.focus);
  } catch {
    $("usage").hidden = true;
    $("offline").hidden = false;
  }
}

renderClock();
setInterval(renderClock, 10_000);
renderUsage();
setInterval(renderUsage, 60_000);
