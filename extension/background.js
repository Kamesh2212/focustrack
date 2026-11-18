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

function matches(domain, list) {
  return !!domain && list.some((b) => domain === b || domain.endsWith("." + b));
}

async function blockState() {
  const s = await chrome.storage.session.get(["blocked", "focusBlocked", "focusAllowOnly", "focusEndsAt"]);
  return {
    blocked: Array.isArray(s.blocked) ? s.blocked : [],
    focusBlocked: Array.isArray(s.focusBlocked) ? s.focusBlocked : [],
    focusAllowOnly: Array.isArray(s.focusAllowOnly) ? s.focusAllowOnly : null,
    focusEndsAt: s.focusEndsAt ?? null,
  };
}

/** Redirects a tab to the blocked page if its site is over its limit or blocked for focus. */
async function enforce(tab, state) {
  if (!tab || tab.id == null || tab.incognito) return;
  const domain = domainOf(tab.url ?? "");
  let reason = null;
  if (matches(domain, state.blocked)) reason = "limit";
  else if (matches(domain, state.focusBlocked)) reason = "focus";
  // "Allow only" sessions: any site not on the list is blocked.
  else if (domain && state.focusAllowOnly && !matches(domain, state.focusAllowOnly)) reason = "focus";
  if (!reason) return;
  const q = new URLSearchParams({ d: domain, r: reason });
  if (reason === "focus" && state.focusEndsAt) q.set("until", state.focusEndsAt);
  await chrome.tabs.update(tab.id, { url: chrome.runtime.getURL(`blocked.html?${q}`) });
}

async function activeState() {
  const win = await chrome.windows.getLastFocused().catch(() => null);
  if (!win) return { focused: false, domain: null, tab: null };
  const [tab] = await chrome.tabs.query({ active: true, windowId: win.id });
  const domain = tab && !tab.incognito ? domainOf(tab.url ?? "") : null;
  return { focused: !!win.focused, domain, tab };
}

/** Never let a tab we can't redirect (closing, dragged, privileged page) break the heartbeat. */
async function safeEnforce(tab, state) {
  try {
    await enforce(tab, state);
  } catch (e) {
    console.warn("FocusTrack: could not redirect a blocked tab", e);
  }
}

/** One request to the app, with a timeout. Throws a short, user-readable reason on failure. */
async function send(s) {
  let res;
  try {
    res = await fetch(API, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-FocusTrack": "1" },
      body: JSON.stringify({ browser: BROWSER, domain: s.domain, focused: s.focused }),
      signal: AbortSignal.timeout(5000),
    });
  } catch (e) {
    throw new Error(e?.name === "TimeoutError" ? "timed out" : "can't reach 127.0.0.1:17865");
  }
  if (!res.ok) throw new Error(`HTTP ${res.status}`);
  return res.json();
}

let current = null; // the heartbeat in progress, shared by everyone who asks
let again = false;

/** Sends a heartbeat. Concurrent callers share one request and all wait for its result. */
function beat() {
  if (current) {
    again = true;
    return current;
  }
  current = (async () => {
    const s = await activeState().catch(() => ({ focused: false, domain: null, tab: null }));
    let data;
    try {
      data = await send(s);
    } catch (e) {
      // Only a failed request means the app isn't reachable.
      await chrome.storage.session.set({ connected: false, lastError: e.message, current: s.domain });
      return false;
    }
    const state = {
      blocked: Array.isArray(data.blocked) ? data.blocked : [],
      focusBlocked: Array.isArray(data.focusBlocked) ? data.focusBlocked : [],
      focusAllowOnly: Array.isArray(data.focusAllowOnly) ? data.focusAllowOnly : null,
      focusEndsAt: data.focusEndsAt ?? null,
    };
    const before = await blockState();
    const wasFocus = before.focusBlocked.length > 0 || before.focusAllowOnly !== null;
    await chrome.storage.session.set({
      connected: true,
      lastError: null,
      tracking: !!data.tracking,
      ...state,
      current: s.domain,
      lastOk: Date.now(),
    });
    await safeEnforce(s.tab, state);
    // A focus round just started: close the door on distracting tabs that are already open.
    if (!wasFocus && (state.focusBlocked.length || state.focusAllowOnly)) {
      for (const tab of await chrome.tabs.query({}).catch(() => [])) await safeEnforce(tab, state);
    }
    return true;
  })().finally(() => {
    current = null;
    if (again) {
      again = false;
      beat();
    }
  });
  return current;
}

chrome.tabs.onActivated.addListener(() => beat());
chrome.windows.onFocusChanged.addListener(() => beat());
chrome.tabs.onUpdated.addListener(async (_id, info, tab) => {
  if (!info.url && info.status !== "complete") return;
  await safeEnforce(tab, await blockState());
  if (tab.active) beat();
});

chrome.alarms.create("focustrack-heartbeat", { periodInMinutes: 0.5 });
chrome.alarms.onAlarm.addListener((a) => {
  if (a.name === "focustrack-heartbeat") beat();
});
chrome.runtime.onStartup.addListener(() => beat());
chrome.runtime.onInstalled.addListener(() => beat());

// The popup and side panel ask for a fresh heartbeat and get its outcome.
chrome.runtime.onMessage.addListener((msg, _sender, reply) => {
  if (msg?.type === "refresh") {
    beat().then(reply, () => reply(false));
    return true;
  }
  return false;
});

beat();
