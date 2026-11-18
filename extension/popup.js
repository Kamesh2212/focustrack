import { WIDGETS, el, fetchSummary, loadLayout } from "./usage.js";

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

async function renderUsage() {
  const root = $("widgets");
  try {
    const s = await fetchSummary();
    // The popup shows the first two widgets you chose for the side panel.
    const layout = (await loadLayout()).filter((x) => x.on && x.id !== "note").slice(0, 2);
    root.replaceChildren();
    for (const item of layout) {
      const w = WIDGETS.find((x) => x.id === item.id);
      root.append(el("section", { class: "widget" }, el("h2", {}, w.title), ...w.render(s, renderUsage)));
    }
  } catch {
    root.replaceChildren(el("p", { class: "muted" }, "Start the FocusTrack app to see your usage."));
  }
}

// Look the window up now: the side panel must open directly from the click.
let windowId = null;
chrome.windows.getCurrent().then((w) => (windowId = w.id));

// Chrome/Edge/Brave: side panel API. Firefox: sidebar. Otherwise a small window.
$("open-panel").addEventListener("click", async () => {
  try {
    if (chrome.sidePanel?.open && windowId != null) {
      await chrome.sidePanel.open({ windowId });
    } else if (globalThis.browser?.sidebarAction?.open) {
      await browser.sidebarAction.open();
    } else {
      throw new Error("no side panel");
    }
  } catch {
    await chrome.windows.create({ url: chrome.runtime.getURL("sidepanel.html"), type: "popup", width: 380, height: 720 });
  }
  window.close();
});

renderStatus();
renderUsage();
chrome.runtime.sendMessage({ type: "refresh" }).then(renderStatus).catch(renderStatus);
