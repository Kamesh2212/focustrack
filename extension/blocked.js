const domain = new URLSearchParams(location.search).get("d");
if (domain) {
  document.getElementById("domain").textContent = domain;
  document.title = `${domain} is blocked — FocusTrack`;
}
