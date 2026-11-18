import { WIDGETS, el, fetchSummary, loadLayout, saveLayout } from "./usage.js";

const $ = (id) => document.getElementById(id);
let layout = [];
let summary = null;

function renderWidgets() {
  const root = $("widgets");
  root.replaceChildren();
  for (const item of layout.filter((x) => x.on)) {
    const w = WIDGETS.find((x) => x.id === item.id);
    if (!w) continue;
    if (!summary && !w.static) continue;
    root.append(el("section", { class: "card widget" }, el("h2", {}, w.title), ...w.render(summary, refresh)));
  }
  if (!root.children.length) root.append(el("p", { class: "muted" }, "No widgets selected. Use Customize to add some."));
}

function renderChooser() {
  const list = $("chooser-list");
  list.replaceChildren();
  layout.forEach((item, i) => {
    const w = WIDGETS.find((x) => x.id === item.id);
    const box = el("input", { type: "checkbox", id: `w-${item.id}` });
    box.checked = item.on;
    box.addEventListener("change", () => {
      item.on = box.checked;
      commit();
    });
    const move = (d) => {
      const j = i + d;
      if (j < 0 || j >= layout.length) return;
      [layout[i], layout[j]] = [layout[j], layout[i]];
      commit();
    };
    list.append(
      el(
        "li",
        {},
        box,
        el("label", { for: `w-${item.id}` }, w.title),
        el("button", { type: "button", class: "icon-btn", "aria-label": `Move ${w.title} up`, onclick: () => move(-1) }, "↑"),
        el("button", { type: "button", class: "icon-btn", "aria-label": `Move ${w.title} down`, onclick: () => move(1) }, "↓"),
      ),
    );
  });
}

async function commit() {
  await saveLayout(layout);
  renderChooser();
  renderWidgets();
}

async function refresh() {
  try {
    summary = await fetchSummary();
    $("offline").hidden = true;
  } catch {
    summary = null;
    $("offline").hidden = false;
  }
  renderWidgets();
}

$("customize").addEventListener("click", () => {
  const c = $("chooser");
  c.hidden = !c.hidden;
  $("customize").setAttribute("aria-expanded", String(!c.hidden));
  $("customize").textContent = c.hidden ? "Customize" : "Done";
});

layout = await loadLayout();
renderChooser();
await refresh();
setInterval(refresh, 30_000);
document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && refresh());
