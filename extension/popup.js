async function render() {
  const s = await chrome.storage.session.get(["connected", "tracking", "current", "blocked"]);
  const status = document.getElementById("status");
  const text = document.getElementById("status-text");
  if (!s.connected) {
    status.className = "status off";
    text.textContent = "FocusTrack app not running";
  } else if (!s.tracking) {
    status.className = "status warn";
    text.textContent = "Website tracking is off in FocusTrack";
  } else {
    status.className = "status on";
    text.textContent = "Connected — tracking websites";
  }
  document.getElementById("current").textContent = s.current || "—";
  const blocked = Array.isArray(s.blocked) ? s.blocked : [];
  document.getElementById("blocked").textContent = blocked.length ? blocked.join(", ") : "None";
}

render();
chrome.runtime.sendMessage({ type: "refresh" }).then(render).catch(render);
