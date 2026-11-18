import { WIDGETS, el, fetchSummary, hasHostAccess, loadLayout, requestHostAccess, saveLayout } from "./usage.js";

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

/** Moves the item with `id` to position `to` in `layout`. Returns whether anything moved. */
function moveTo(id, to) {
  const from = layout.findIndex((x) => x.id === id);
  if (from < 0 || to < 0 || to >= layout.length || from === to) return false;
  const [item] = layout.splice(from, 1);
  layout.splice(to, 0, item);
  return true;
}

/** Drag a row (anywhere but its checkbox) to reorder; the order is saved on release. */
function startDrag(e, li, id) {
  if (e.button !== 0 || e.target.closest("input")) return;
  const list = $("chooser-list");
  const sx = e.clientX;
  const sy = e.clientY;
  let dragging = false;
  const onMove = (ev) => {
    if (!dragging) {
      if (Math.hypot(ev.clientX - sx, ev.clientY - sy) < 5) return;
      dragging = true;
      li.classList.add("dragging");
      list.classList.add("is-dragging");
    }
    ev.preventDefault();
    const over = document.elementFromPoint(ev.clientX, ev.clientY)?.closest("li[data-widget]");
    if (!over || over === li || !list.contains(over)) return;
    const to = layout.findIndex((x) => x.id === over.dataset.widget);
    if (!moveTo(id, to)) return;
    // Move the row in place so the dragged element (and its pointer) survive.
    const rows = [...list.children];
    if (rows.indexOf(li) < rows.indexOf(over)) over.after(li);
    else over.before(li);
  };
  const finish = (save) => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerup", onUp);
    window.removeEventListener("pointercancel", onCancel);
    if (!dragging) return;
    li.classList.remove("dragging");
    list.classList.remove("is-dragging");
    // Releasing over a label would otherwise toggle its checkbox.
    const swallow = (ev) => {
      ev.stopPropagation();
      ev.preventDefault();
    };
    window.addEventListener("click", swallow, { capture: true, once: true });
    setTimeout(() => window.removeEventListener("click", swallow, true), 0);
    if (save) commit();
    else loadLayout().then((l) => ((layout = l), renderChooser()));
  };
  const onUp = () => finish(true);
  const onCancel = () => finish(false);
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("pointercancel", onCancel);
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
    const grip = el("button", { type: "button", class: "grip", title: "Drag to reorder", "aria-label": `Reorder ${w.title} (drag, or use the arrow keys)` }, "⠿");
    grip.addEventListener("keydown", (ev) => {
      const d = ev.key === "ArrowUp" ? -1 : ev.key === "ArrowDown" ? 1 : 0;
      if (!d || !moveTo(item.id, i + d)) return;
      ev.preventDefault();
      commit().then(() => $("chooser-list").querySelector(`li[data-widget="${item.id}"] .grip`)?.focus());
    });
    const li = el("li", { "data-widget": item.id }, grip, box, el("label", { for: `w-${item.id}` }, w.title));
    li.addEventListener("pointerdown", (ev) => startDrag(ev, li, item.id));
    list.append(li);
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
  } catch (e) {
    $("offline-detail").textContent = `Details: ${e.message}`;
    $("grant").hidden = await hasHostAccess();
    summary = null;
    $("offline").hidden = false;
  }
  renderWidgets();
}

$("grant").addEventListener("click", async () => {
  if (await requestHostAccess()) refresh();
});

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
