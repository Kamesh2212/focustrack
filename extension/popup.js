import { WIDGETS, el, fetchSummary, hasHostAccess, loadLayout, requestHostAccess } from "./usage.js";

const $ = (id) => document.getElementById(id);

function setStatus(kind, text) {
  $("status").className = `status ${kind}`;
  $("status-text").textContent = text;
}

/** Explains why we can't reach the app, with a fix where there is one. */
async function renderProblem(error) {
  const root = $("widgets");
  root.replaceChildren();
  if (!(await hasHostAccess())) {
    setStatus("warn", "Needs permission");
    const btn = el("button", { type: "button", class: "btn wide" }, "Allow access to FocusTrack");
    btn.addEventListener("click", async () => {
      if (await requestHostAccess()) {
        await chrome.runtime.sendMessage({ type: "refresh" }).catch(() => {});
        render();
      }
    });
    root.append(el("p", { class: "muted" }, "Your browser hasn't allowed this extension to talk to the FocusTrack app on this computer."), btn);
    return;
  }
  setStatus("off", "Can't reach the app");
  root.append(
    el("p", { class: "muted" }, "Make sure FocusTrack is running (check the system tray). "),
    el("p", { class: "small muted" }, `Details: ${error}`),
  );
}

async function render() {
  const { current, tracking } = await chrome.storage.session.get(["current", "tracking"]);
  $("current").textContent = current || "—";
  let s;
  try {
    // The popup checks the app itself rather than trusting an older stored status.
    s = await fetchSummary();
  } catch (e) {
    await renderProblem(e.message);
    return;
  }
  if (tracking === false) setStatus("warn", "Website tracking off");
  else setStatus("on", "Connected");

  const root = $("widgets");
  // The popup shows the first two widgets you chose for the side panel.
  const layout = (await loadLayout()).filter((x) => x.on && x.id !== "note").slice(0, 2);
  root.replaceChildren();
  for (const item of layout) {
    const w = WIDGETS.find((x) => x.id === item.id);
    root.append(el("section", { class: "widget" }, el("h2", {}, w.title), ...w.render(s, render)));
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

setStatus("", "Checking…");
render();
// Also nudge the background heartbeat so tracking and blocking catch up.
chrome.runtime.sendMessage({ type: "refresh" }).catch(() => {});
