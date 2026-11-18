const q = new URLSearchParams(location.search);
const domain = q.get("d");
const reason = q.get("r");
const until = q.get("until");

if (domain) {
  document.getElementById("domain").textContent = domain;
  document.title = `${domain} is blocked — FocusTrack`;
}

if (reason === "focus") {
  const end = until ? new Date(until.replace(" ", "T")) : null;
  document.getElementById("headline-rest").textContent = " is blocked while you focus";
  document.getElementById("reason").textContent = end
    ? `You're in a FocusTrack focus session until ${end.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" })}. Distracting sites are blocked until then.`
    : "You're in a FocusTrack focus session. Distracting sites are blocked until it ends.";
  document.getElementById("how").textContent = "To end the session early, open FocusTrack → Focus Mode.";
}
