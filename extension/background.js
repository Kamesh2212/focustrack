// FocusTrack browser extension (Chrome, Edge, Brave, Opera, Vivaldi, Firefox).
//
// Sends ONLY the active tab's domain (e.g. "youtube.com") to the FocusTrack
// desktop app on this computer. Never full URLs, page titles or content.
// Private/incognito tabs are never reported.

const API = "http://127.0.0.1:17865/v1/heartbeat";

function browserFamily() {
  const ua = navigator.userAgent;
  if (/Firefox\//.test(ua)) return "firefox";
  const brands = (navigator.userAgentData?.brands ?? []).map((b) => b.brand);
  if (brands.some((b) => /Edge/.test(b)) || /Edg\//.test(ua)) return "edge";
  if (brands.includes("Brave") || navigator.brave) return "brave";
  if (brands.some((b) => /Opera/.test(b)) || /OPR\//.test(ua)) return "opera";
  if (brands.some((b) => /Vivaldi/.test(b))) return "vivaldi";
  return "chrome";
}

const BROWSER = browserFamily();

function domainOf(url) {
  try {
    const u = new URL(url);
    if (u.protocol !== "http:" && u.protocol !== "https:") return null;
    return u.hostname.replace(/^www\./, "").toLowerCase() || null;
  } catch {
    return null;
  }
}

function isBlocked(domain, blocked) {
  return !!domain && blocked.some((b) => domain === b || domain.endsWith("." + b));
}

async function blockedList() {
  const { blocked } = await chrome.storage.session.get("blocked");
  return Array.isArray(blocked) ? blocked : [];
}

async function enforce(tab, blocked) {
  if (!tab || tab.id == null || tab.incognito) return;
  const domain = domainOf(tab.url ?? "");
  if (isBlocked(domain, blocked)) {
    await chrome.tabs.update(tab.id, {
      url: chrome.runtime.getURL(`blocked.html?d=${encodeURIComponent(domain)}`),
    });
  }
}

async function activeState() {
  const win = await chrome.windows.getLastFocused().catch(() => null);
  if (!win) return { focused: false, domain: null, tab: null };
  const [tab] = await chrome.tabs.query({ active: true, windowId: win.id });
  const domain = tab && !tab.incognito ? domainOf(tab.url ?? "") : null;
  return { focused: !!win.focused, domain, tab };
}

let inFlight = false;
let pending = false;

async function beat() {
  if (inFlight) {
    pending = true;
    return;
  }
  inFlight = true;
  try {
    const s = await activeState();
    try {
      const res = await fetch(API, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-FocusTrack": "1" },
        body: JSON.stringify({ browser: BROWSER, domain: s.domain, focused: s.focused }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const data = await res.json();
      const blocked = Array.isArray(data.blocked) ? data.blocked : [];
      await chrome.storage.session.set({
        connected: true,
        tracking: !!data.tracking,
        blocked,
        current: s.domain,
        lastOk: Date.now(),
      });
      await enforce(s.tab, blocked);
    } catch {
      await chrome.storage.session.set({ connected: false, current: s.domain });
    }
  } finally {
    inFlight = false;
    if (pending) {
      pending = false;
      beat();
    }
  }
}

chrome.tabs.onActivated.addListener(() => beat());
chrome.windows.onFocusChanged.addListener(() => beat());
chrome.tabs.onUpdated.addListener(async (_id, info, tab) => {
  if (!info.url && info.status !== "complete") return;
  await enforce(tab, await blockedList());
  if (tab.active) beat();
});

chrome.alarms.create("focustrack-heartbeat", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === "focustrack-heartbeat") beat();
});
chrome.runtime.onStartup.addListener(() => beat());
chrome.runtime.onInstalled.addListener(() => beat());

// Popup asks for a fresh status.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg?.type === "refresh") {
    beat().then(() => reply(true));
    return true;
  }
  return false;
});

beat();
