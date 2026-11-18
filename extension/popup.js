import { fetchSummary, fmt, fmtClock, renderLimits, renderSites } from "./usage.js";

const $ = (id) => document.getElementById(id);

async function renderStatus() {
  const s = await chrome.storage.session.get(["connected", "tracking", "current"]);
  const status = $("status");
  if (!s.connected) {
    status.className = "status off";
    $("status-text").textContent = "App not running";
  } else if (!s.tracking) {
    status.className = "status warn";
    $("status-text").textContent = "Website tracking off";
  } else {
    status.className = "status on";
    $("status-text").textContent = "Connected";
  }
  $("current").textContent = s.current || "—";
}

function renderFocus(focus) {
  const box = $("focus");
  if (!focus) {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  const end = new Date(focus.endsAt.replace(" ", "T")).getTime();
  const tick = () => {
    const left = (end - Date.now()) / 1000;
    const what = focus.phase === "focus" ? `Focusing: ${focus.label}` : "Break";
    const round = focus.rounds > 1 ? ` · round ${focus.round}/${focus.rounds}` : "";
    box.textContent = `${what}${round} — ${fmtClock(left)} left`;
  };
  tick();
  setInterval(tick, 1000);
}

async function renderUsage() {
  try {
    const s = await fetchSummary();
    $("screen").textContent = fmt(s.screenSecs);
    $("web").textContent = fmt(s.webSecs);
    renderSites($("sites"), s.topSites, 5);
    renderLimits($("limits"), s.limits);
    renderFocus(s.focus);
  } catch {
    $("sites").textContent = "Start the FocusTrack app to see your usage.";
  }
}

renderStatus();
renderUsage();
chrome.runtime.sendMessage({ type: "refresh" }).then(renderStatus).catch(renderStatus);
