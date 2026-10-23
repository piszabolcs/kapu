// KAPU window: views, routing and the bridge to the Rust commands.
// Every answer comes from crates/kapu-gui/src/api.rs, through Tauri in the window
// or through examples/serve.rs in a browser. Text from the file is always set with
// textContent; the only HTML is the room body, rendered and sanitised in Rust.

import { Graph } from "./graph.js";

const tauri = window.__TAURI__;
const $ = (id) => document.getElementById(id);
const VIEWS = ["room", "search", "graph", "check"];

const state = {
  about: null,
  file: null,
  groupOf: new Map(), // door id -> group index
  groupName: new Map(), // gid -> display name
  roomId: null,
  graph: null,
  graphLoaded: false,
  busyOpen: false,
};

/* --------------------------------------------------------------- bridge */

async function call(cmd, args = {}) {
  const started = performance.now();
  try {
    if (tauri) return await tauri.core.invoke("kapu", { cmd, args });
    const res = await fetch(`/api/${cmd}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(args),
    });
    const body = await res.json();
    if (body.error) throw body.error;
    return body.ok;
  } finally {
    lastTiming(cmd, performance.now() - started);
  }
}

function openExternal(url) {
  if (tauri) {
    tauri.core.invoke("open_external", { url }).catch(() => {});
  } else {
    window.open(url, "_blank", "noopener,noreferrer");
  }
}

/* -------------------------------------------------------------- helpers */

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") node.className = v;
    else if (k === "text") node.textContent = v;
    else node.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    node.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return node;
}

const fmt = (n) => Number(n ?? 0).toLocaleString("en-US");
const tokens = (n) => `${fmt(n)} t`;
const fold = (s) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
const doorHref = (id) => `#room=${encodeURIComponent(id)}`;

function errorText(e) {
  if (e && e.code) return { code: e.code, detail: e.detail };
  return { code: "GUI", detail: String(e?.message ?? e) };
}

function lastTiming(cmd, ms) {
  const label = { open: "open", room: "open door", search: "search", graph: "graph", validate: "check" }[cmd];
  if (label) $("timing").textContent = `${label} ${ms < 10 ? ms.toFixed(1) : Math.round(ms)} ms`;
}

const storage = {
  get(key, fallback) {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : JSON.parse(v);
    } catch {
      return fallback;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, JSON.stringify(value));
    } catch {
      /* private mode or blocked storage: recent files are a convenience only */
    }
  },
};

/* ---------------------------------------------------------------- views */

function show(view) {
  for (const section of document.querySelectorAll(".view")) {
    section.hidden = section.dataset.view !== view;
  }
  for (const a of document.querySelectorAll("#view-nav a")) {
    if (a.dataset.view === view) a.setAttribute("aria-current", "page");
    else a.removeAttribute("aria-current");
  }
  if (view === "room" && state.roomId) $("view-nav").querySelector('[data-view="room"]').href = doorHref(state.roomId);
}

function route() {
  if (!state.file) {
    show("start");
    return;
  }
  const hash = decodeURIComponent(location.hash.slice(1));
  if (hash.startsWith("room=")) openRoom(hash.slice(5));
  else if (hash.startsWith("search=")) {
    show("search");
    runSearch(hash.slice(7));
  } else if (hash === "search") {
    show("search");
    $("search-query").focus();
  } else if (hash === "graph") showGraph();
  else if (hash === "check") show("check");
  else openRoom(state.roomId ?? firstRoom());
}

function go(hash) {
  if (location.hash === `#${hash}`) route();
  else location.hash = hash;
}

function firstRoom() {
  for (const g of state.file?.groups ?? []) {
    const room = g.doors.find((d) => d.room);
    if (room) return room.id;
  }
  return null;
}

/* ----------------------------------------------------------------- open */

async function pickAndOpen() {
  if (tauri) {
    try {
      const path = await tauri.core.invoke("pick");
      if (path) await openFile(path);
    } catch (e) {
      startError(errorText(e));
    }
    return;
  }
  // Browser (development server): no system dialog, type a path instead.
  state.file = null;
  location.hash = "";
  show("start");
  $("path-form").hidden = false;
  $("path-input").focus();
}

async function openFile(path, keepRoute = false) {
  if (state.busyOpen) return;
  state.busyOpen = true;
  $("start-error").hidden = true;
  try {
    const file = await call("open", { path });
    state.file = file;
    state.graphLoaded = false;
    remember(file);
    renderFile(file);
    if (keepRoute && location.hash.length > 1) route();
    else {
      state.roomId = null;
      go(`room=${firstRoom() ?? ""}`);
    }
  } catch (e) {
    startError(errorText(e));
  } finally {
    state.busyOpen = false;
  }
}

function startError({ code, detail }) {
  if (!state.file) show("start");
  const box = $("start-error");
  box.replaceChildren(el("strong", { text: code }), detail);
  box.hidden = false;
}

function remember(file) {
  const list = storage.get("kapu.recent", []).filter((r) => r.path !== file.path);
  list.unshift({ path: file.path, name: file.name });
  storage.set("kapu.recent", list.slice(0, 6));
}

function renderRecent() {
  const list = storage.get("kapu.recent", []);
  $("recent").hidden = list.length === 0;
  $("recent-list").replaceChildren(
    ...list.map((r) =>
      el("li", {}, el("button", { type: "button", "data-path": r.path }, el("span", { text: r.name }), el("span", { text: r.path }))),
    ),
  );
}

function renderFile(file) {
  const h = file.header;
  $("file-name").textContent = file.name;
  $("file-path").textContent = file.path;
  $("file-path").title = file.path;
  for (const id of ["file-chip", "view-nav", "quick-search", "reload", "rail"]) $(id).hidden = false;
  $("rail-totals").textContent = `${fmt(h.rooms)} rooms · ${fmt(h.doors)} doors · ${tokens(file.tokens)}`;

  state.groupOf.clear();
  state.groupName.clear();
  file.groups.forEach((g, i) => {
    state.groupName.set(g.gid, g.name || g.gid);
    for (const d of g.doors) state.groupOf.set(d.id, i);
  });
  renderMap(file);
  renderStatus(file);
  $("filter").value = "";
  $("check-result").hidden = true;
  $("results").hidden = true;
  $("search-empty").hidden = false;
  $("search-status").replaceChildren();
}

function groupColor(index) {
  return `var(--g${(index % 8) + 1})`;
}

function renderMap(file) {
  const max = Math.max(1, file.max_door_tokens);
  const map = $("map");
  map.replaceChildren(
    ...file.groups.map((g, i) => {
      const list = el("div", { class: "group-doors", id: `group-${i}` });
      for (const d of g.doors) {
        const a = el(
          "a",
          {
            class: d.room ? "door" : "door sub",
            href: doorHref(d.id),
            "data-id": d.id,
            "data-search": fold(`${d.title} ${d.id} ${(d.tags ?? []).join(" ")}`),
            title: `${d.title || d.id}\n${d.id} · ${tokens(d.tokens)}`,
          },
          el("span", { class: "door-title", text: d.title || d.id }),
          el("span", { class: "meter", "aria-hidden": "true" }, el("span")),
          el("span", { class: "door-tokens", text: fmt(d.tokens) }),
        );
        a.style.setProperty("--cost-share", (d.tokens / max).toFixed(4));
        list.append(a);
      }
      const swatch = el("span", { class: "swatch", "aria-hidden": "true" });
      swatch.style.setProperty("--swatch", groupColor(i));
      const head = el(
        "button",
        { type: "button", class: "group-head", "aria-expanded": "true", "aria-controls": list.id },
        swatch,
        el("span", { class: "group-name", text: g.name || g.gid }),
        el("span", { class: "group-meta", text: `${g.rooms} · ${fmt(g.tokens)}` }),
      );
      return el("section", { class: "group", "aria-label": `Group ${g.name || g.gid}` }, head, list);
    }),
  );
}

function renderStatus(file) {
  const ok = file.check.valid;
  const h = file.header;
  const parts = [
    el("span", { class: `dot ${ok ? "ok" : "bad"}`, "aria-hidden": "true" }),
    ok ? el("span", { text: "Valid" }) : el("span", { class: "label-bad", text: `${file.check.code} ${file.check.detail}` }),
    el("span", { text: `· ${fmt(h.groups)} groups · ${fmt(file.bytes)} bytes · model ${h.emb}` }),
  ];
  $("status").replaceChildren(...parts);
}

function markCurrent(id) {
  for (const a of document.querySelectorAll("#map .door[aria-current]")) a.removeAttribute("aria-current");
  const row = document.querySelector(`#map .door[data-id="${CSS.escape(id)}"]`);
  if (row) {
    row.setAttribute("aria-current", "true");
    row.scrollIntoView({ block: "nearest" });
  }
}

/* ----------------------------------------------------------------- room */

async function openRoom(id) {
  if (!id) return;
  show("room");
  state.roomId = id;
  markCurrent(id);
  const article = $("room");
  article.setAttribute("aria-busy", "true");
  try {
    const r = await call("room", { id });
    if (state.roomId !== id) return;
    renderRoom(r);
    article.scrollTop = 0;
  } catch (e) {
    const { code, detail } = errorText(e);
    $("room-title").textContent = "This door does not open";
    $("room-meta").replaceChildren(el("span", { text: `${code} · ${id}` }));
    $("room-lead").textContent = detail;
    $("room-body").replaceChildren();
    $("room-crumbs").replaceChildren();
    for (const list of ["room-tags", "room-subs", "room-neighbours"]) $(list).replaceChildren();
  } finally {
    article.setAttribute("aria-busy", "false");
  }
  if (state.graph) state.graph.select(id);
}

function renderRoom(r) {
  const gi = state.groupOf.get(r.id) ?? state.groupOf.get(r.parent?.id);
  const group = gi === undefined ? null : state.file.groups[gi];
  const sep = () => el("span", { "aria-hidden": "true", text: "/" });
  const crumbs = [];
  if (group) crumbs.push(el("span", { text: group.name || group.gid }), sep());
  if (r.parent) crumbs.push(el("a", { href: doorHref(r.parent.id), text: r.parent.title || r.parent.id }), sep());
  crumbs.push(el("span", { "aria-current": "page", text: r.id }));
  $("room-crumbs").replaceChildren(...crumbs);

  $("room-title").textContent = r.title || r.id;
  const meta = [el("span", { class: "price", text: `${fmt(r.tokens)} tokens to open` })];
  meta.push(el("span", { text: r.room ? `room · ${r.children.length} sub-doors` : "sub-door" }));
  meta.push(el("span", { text: `${r.neighbours.length} neighbours` }));
  if (r.index_stale) meta.push(el("span", { class: "label-bad", text: "index out of date" }));
  $("room-meta").replaceChildren(...meta);
  $("room-lead").textContent = r.description;
  $("room-body").innerHTML = r.html; // rendered and sanitised in Rust (markdown.rs)

  const tags = r.tags.length ? r.tags : null;
  $("room-tags").replaceChildren(...(tags ? tags.map((t) => el("li", { text: t })) : [el("li", { class: "none", text: "none" })]));
  $("panel-tags").hidden = !tags && !r.room;

  $("panel-subs").hidden = !r.room;
  $("room-subs").replaceChildren(
    ...(r.children.length
      ? r.children.map((c) => doorItem(c.id, c.title, c.tokens))
      : [el("li", { class: "none", text: "No sub-doors" })]),
  );

  const order = { link: 0, term: 1, sim: 2 };
  const neighbours = [...r.neighbours].sort((a, b) => (order[a.kind] ?? 3) - (order[b.kind] ?? 3));
  $("room-neighbours").replaceChildren(
    ...(neighbours.length
      ? neighbours.map((n) => doorItem(n.id, n.title, n.tokens, n.kind))
      : [el("li", { class: "none", text: "No edges in the graph" })]),
  );
  $("room-print").textContent = r.fingerprint ?? "";
}

function doorItem(id, title, tokenCount, kind) {
  return el(
    "li",
    {},
    el(
      "a",
      { href: doorHref(id), title: id },
      el("span", { class: "t", text: title || id }),
      el("span", { class: "m" }, kind ? el("span", { class: `kind ${kind}`, text: kind }) : null, fmt(tokenCount)),
    ),
  );
}

/* --------------------------------------------------------------- search */

async function runSearch(query) {
  const q = query.trim();
  $("search-query").value = q;
  $("quick-query").value = q;
  if (!q) return;
  const k = Number($("search-k").value) || 10;
  const budgetRaw = $("search-budget").value.trim();
  const budget = budgetRaw === "" ? null : Math.max(0, Math.floor(Number(budgetRaw)));
  const status = $("search-status");
  status.replaceChildren(el("span", { text: "Searching…" }));
  const slow = setTimeout(() => {
    status.replaceChildren(el("span", { text: "Loading the vector model (first search only)…" }));
  }, 700);
  try {
    const r = await call("search", { query: q, k, max_tokens: budget });
    clearTimeout(slow);
    renderResults(r, budget);
  } catch (e) {
    clearTimeout(slow);
    const { code, detail } = errorText(e);
    status.replaceChildren(el("span", { class: "warn", text: `${code} ${detail}` }));
  }
}

function renderResults(r, budget) {
  const body = $("results-body");
  const total = r.hits.reduce((s, h) => s + h.tokens, 0);
  const scale = Math.max(1, budget ?? total);
  let running = 0;
  body.replaceChildren(
    ...r.hits.map((h, i) => {
      running += h.tokens;
      const bar = el("span");
      bar.style.setProperty("--share", Math.min(1, running / scale).toFixed(4));
      const row = el(
        "tr",
        { "data-id": h.id },
        el("td", { class: "num", text: i + 1 }),
        el(
          "td",
          {},
          el("a", { href: doorHref(h.id), class: "hit-title", text: h.title || h.id }),
          el("span", { class: "hit-id", text: h.id }),
        ),
        el("td", { class: "source", text: { both: "both", bm25: "bm25", vec: "vector" }[h.source] ?? h.source }),
        el("td", { class: "num", text: h.score.toFixed(4) }),
        el("td", { class: "num", text: fmt(h.tokens) }),
        el("td", {}, el("div", { class: "budget" }, el("div", { class: "budget-bar" }, bar), el("span", { class: "mono", text: fmt(running) }))),
      );
      return row;
    }),
  );
  $("results").hidden = r.hits.length === 0;
  $("search-empty").hidden = r.hits.length !== 0;
  if (r.hits.length === 0) {
    $("search-empty").replaceChildren(el("p", {}, el("strong", { text: "No door matches." }), " Try other words, or a question in plain language if the file has a vector model."));
  }
  const vector = r.vector.available
    ? el("span", { class: "ok", text: "✓ words and meaning (BM25 + vectors, RRF)" })
    : el("span", { class: "warn", text: `△ words only (BM25): ${r.vector.reason}` });
  const summary = el("span", { text: `${r.hits.length} doors · ${fmt(total)} tokens${budget !== null ? ` of ${fmt(budget)}` : ""}` });
  const parts = [vector, summary];
  if (r.model_ms > 50) parts.push(el("span", { text: `model loaded in ${Math.round(r.model_ms)} ms` }));
  $("search-status").replaceChildren(...parts);
}

/* ---------------------------------------------------------------- graph */

async function showGraph() {
  show("graph");
  if (!state.graph) {
    state.graph = new Graph($("graph"), $("graph-tip"), (id) => go(`room=${id}`));
  }
  if (state.graphLoaded) {
    state.graph.resize();
    return;
  }
  try {
    const g = await call("graph");
    state.graphLoaded = true;
    const counts = { link: 0, term: 0, sim: 0 };
    for (const e of g.edges) counts[e.kind] = (counts[e.kind] ?? 0) + 1;
    for (const span of document.querySelectorAll("[data-count]")) span.textContent = fmt(counts[span.dataset.count] ?? 0);
    const groupIndex = new Map(state.file.groups.map((gr, i) => [gr.gid, i]));
    const shown = state.graph.setData(g.nodes, g.edges, groupIndex);
    const note = $("graph-note");
    note.hidden = shown === g.nodes.length;
    note.textContent = `Rooms only: ${fmt(shown)} of ${fmt(g.nodes.length)} doors`;
    renderLegend();
    state.graph.select(state.roomId);
  } catch (e) {
    const { code, detail } = errorText(e);
    $("graph-note").hidden = false;
    $("graph-note").textContent = `${code} ${detail}`;
  }
}

function renderLegend() {
  $("legend").replaceChildren(
    ...state.file.groups.map((g, i) => {
      const swatch = el("span", { class: "swatch", "aria-hidden": "true" });
      swatch.style.setProperty("--swatch", groupColor(i));
      return el("li", {}, swatch, el("span", { class: "name", text: g.name || g.gid }), el("span", { class: "mono", text: `${g.rooms}` }));
    }),
  );
}

/* ---------------------------------------------------------------- check */

const ICONS = {
  ok: '<svg viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m4 10.5 4 4 8-9"/></svg>',
  bad: '<svg viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="m5 5 10 10M15 5 5 15"/></svg>',
};

async function runCheck(full) {
  const buttons = [$("check-quick"), $("check-full")];
  for (const b of buttons) b.disabled = true;
  const box = $("check-result");
  box.hidden = false;
  box.replaceChildren(el("p", { text: full ? "Reading every byte and rebuilding…" : "Checking…" }));
  try {
    const r = await call("validate", { full });
    const verdict = el("p", { class: `verdict ${r.valid ? "ok" : "bad"}` });
    verdict.innerHTML = r.valid ? ICONS.ok : ICONS.bad; // static markup above
    verdict.append(r.valid ? "Valid" : `${r.code}: not valid`);
    const rows = r.valid
      ? [
          ["Level", full ? "full (body, fingerprints, rebuild)" : "quick (header and sections)"],
          ["Rooms", fmt(r.rooms)],
          ["Doors", fmt(r.doors)],
          ["Rebuild compared", full ? (r.rebuild_checked ? "yes, same bytes" : "skipped") : "not in a quick check"],
          ["Time", `${Math.round(r.ms)} ms`],
        ]
      : [
          ["Code", r.code],
          ["Detail", r.detail],
          ["Level", full ? "full" : "quick"],
          ["Time", `${Math.round(r.ms)} ms`],
        ];
    const dl = el("dl", {}, ...rows.flatMap(([k, v]) => [el("dt", { text: k }), el("dd", { text: v })]));
    const warnings = (r.warnings ?? []).map((w) => el("p", { class: "warn", text: `△ ${w.code}: ${w.detail}` }));
    box.replaceChildren(verdict, dl, ...warnings);
  } catch (e) {
    const { code, detail } = errorText(e);
    box.replaceChildren(el("p", { class: "verdict bad", text: `${code}: ${detail}` }));
  } finally {
    for (const b of buttons) b.disabled = false;
  }
}

/* ---------------------------------------------------------------- about */

function renderAbout(about) {
  $("signature").textContent = about.signature;
  $("signature").setAttribute("aria-label", `${about.signature}: about KAPU`);
  $("about-version").textContent = `v${about.version}`;
  $("about-format").textContent = `file format KAPU ${about.format}`;
  $("about-purpose").textContent = about.purpose;
  $("about-layers").replaceChildren(...about.layers.flatMap((l) => [el("dt", { text: l.name }), el("dd", { text: l.role })]));
  $("about-built").replaceChildren(...about.built_with.map((b) => el("li", { text: b })));
  $("about-repos").replaceChildren(
    ...about.repositories.map((r) => el("li", {}, el("a", { href: r.url, "data-external": "true", text: r.name }))),
  );
  $("about-sign").textContent = about.signature;
}

/* --------------------------------------------------------------- wiring */

function wire() {
  window.addEventListener("hashchange", route);
  $("open").addEventListener("click", pickAndOpen);
  $("start-open").addEventListener("click", pickAndOpen);
  $("reload").addEventListener("click", () => state.file && openFile(state.file.path, true));
  $("path-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const path = $("path-input").value.trim();
    if (path) openFile(path);
  });
  $("recent-list").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-path]");
    if (b) openFile(b.dataset.path);
  });

  $("quick-search").addEventListener("submit", (e) => {
    e.preventDefault();
    const q = $("quick-query").value.trim();
    if (q) go(`search=${encodeURIComponent(q)}`);
  });
  $("search-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const q = $("search-query").value.trim();
    if (q) go(`search=${encodeURIComponent(q)}`);
  });
  $("results-body").addEventListener("click", (e) => {
    if (e.target.closest("a")) return;
    const row = e.target.closest("tr[data-id]");
    if (row) go(`room=${row.dataset.id}`);
  });

  $("filter").addEventListener("input", () => applyFilter($("filter").value));
  $("map").addEventListener("click", (e) => {
    const head = e.target.closest(".group-head");
    if (!head) return;
    const open = head.getAttribute("aria-expanded") === "true";
    head.setAttribute("aria-expanded", String(!open));
    $(head.getAttribute("aria-controls")).hidden = open;
  });

  // Links inside a room: door links stay in the window, web links go to the browser.
  for (const container of [$("room-body"), $("about")]) {
    container.addEventListener("click", (e) => {
      const a = e.target.closest("a[href]");
      if (!a) return;
      const href = a.getAttribute("href");
      if (/^(https?:|mailto:)/i.test(href)) {
        e.preventDefault();
        openExternal(href);
      }
    });
  }

  $("check-quick").addEventListener("click", () => runCheck(false));
  $("check-full").addEventListener("click", () => runCheck(true));

  for (const box of document.querySelectorAll(".edge-kinds input")) {
    box.addEventListener("change", () => {
      const kinds = new Set([...document.querySelectorAll(".edge-kinds input:checked")].map((i) => i.value));
      state.graph?.setKinds(kinds);
    });
  }
  $("graph-fit").addEventListener("click", () => state.graph?.fit());
  $("graph-zoom-in").addEventListener("click", () => state.graph?.zoomBy(1.25));
  $("graph-zoom-out").addEventListener("click", () => state.graph?.zoomBy(0.8));

  const about = $("about");
  $("signature").addEventListener("click", () => about.showModal());
  about.addEventListener("click", (e) => {
    if (e.target === about) about.close(); // click on the backdrop
  });

  document.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || e.altKey) return;
    const key = e.key.toLowerCase();
    if (key === "o") {
      e.preventDefault();
      pickAndOpen();
    } else if (key === "k" && state.file) {
      e.preventDefault();
      $("quick-query").focus();
      $("quick-query").select();
    } else if (["1", "2", "3", "4"].includes(key) && state.file) {
      e.preventDefault();
      const view = VIEWS[Number(key) - 1];
      go(view === "room" && state.roomId ? `room=${state.roomId}` : view);
    }
  });

  if (tauri?.event) {
    const body = document.body;
    tauri.event.listen("tauri://drag-enter", () => body.classList.add("dropping"));
    tauri.event.listen("tauri://drag-leave", () => body.classList.remove("dropping"));
    tauri.event.listen("tauri://drag-drop", (e) => {
      body.classList.remove("dropping");
      const path = (e.payload?.paths ?? []).find((p) => p.toLowerCase().endsWith(".kapu"));
      if (path) openFile(path);
      else if ((e.payload?.paths ?? []).length) startError({ code: "GUI", detail: "Only .kapu files can be opened." });
    });
  } else {
    $("drop-hint").textContent = "or type a path below";
    $("path-form").hidden = false;
  }
}

function applyFilter(text) {
  const q = fold(text.trim());
  for (const group of document.querySelectorAll("#map .group")) {
    let any = false;
    let roomRow = null;
    let roomMatch = false;
    for (const row of group.querySelectorAll(".door")) {
      const hit = !q || row.dataset.search.includes(q);
      if (!row.classList.contains("sub")) {
        roomRow = row;
        roomMatch = hit;
        row.hidden = !hit;
      } else {
        row.hidden = !hit && !roomMatch;
        if (hit && roomRow) roomRow.hidden = false;
      }
      any ||= !row.hidden;
    }
    group.hidden = !any;
  }
}

async function main() {
  wire();
  renderRecent();
  try {
    const start = await call("start");
    state.about = start.about;
    renderAbout(start.about);
    if (start.initial) await openFile(start.initial, true);
    else route();
  } catch (e) {
    startError(errorText(e));
  }
}

main();
