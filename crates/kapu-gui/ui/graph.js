// Graph view: the doors and the #GRAPH edges on a canvas.
// The layout is a small force simulation with seeded start positions, so the same
// file always draws the same picture. Colours come from the CSS tokens.

const MAX_NODES = 600; // above this, sub-doors fold into their rooms
const ITERATIONS = 320;

function hash(text) {
  let h = 2166136261;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0) / 4294967296;
}

function token(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

export class Graph {
  constructor(canvas, tip, onOpen) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.tip = tip;
    this.onOpen = onOpen;
    this.nodes = [];
    this.edges = [];
    this.kinds = new Set(["link", "term", "sim"]);
    this.view = { x: 0, y: 0, k: 1 };
    this.hover = null;
    this.selected = null;
    this.drag = null;
    new ResizeObserver(() => this.resize()).observe(canvas);
    matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => this.draw());
    this.listen();
  }

  /** Loads the doors and edges; returns how many nodes are drawn. */
  setData(nodes, edges, groupIndex) {
    const roomsOnly = nodes.length > MAX_NODES;
    const kept = roomsOnly ? nodes.filter((n) => n.room) : nodes;
    const byId = new Map();
    const groups = [...new Set(kept.map((n) => n.gid))];
    const ring = 150 * Math.sqrt(Math.max(1, groups.length));
    const centre = new Map(
      groups.map((g, i) => {
        const a = (i / groups.length) * Math.PI * 2 - Math.PI / 2;
        return [g, { x: Math.cos(a) * ring, y: Math.sin(a) * ring }];
      }),
    );
    this.nodes = kept.map((n) => {
      const c = centre.get(n.gid);
      const a = hash(n.id) * Math.PI * 2;
      const d = 20 + hash(`${n.id}#`) * 60;
      const node = {
        ...n,
        parent: n.room ? null : n.id.split("/")[0],
        group: groupIndex.get(n.gid) ?? 0,
        cx: c.x,
        cy: c.y,
        x: c.x + Math.cos(a) * d,
        y: c.y + Math.sin(a) * d,
        vx: 0,
        vy: 0,
        r: n.room ? Math.min(11, 3 + Math.sqrt(n.tokens) * 0.2) : Math.min(5, 2 + Math.sqrt(n.tokens) * 0.08),
      };
      byId.set(n.id, node);
      return node;
    });
    const fold = (id) => byId.get(id) ?? byId.get(id.split("/")[0]);
    this.edges = [];
    const seen = new Set();
    for (const e of edges) {
      const a = fold(e.from);
      const b = fold(e.to);
      if (!a || !b || a === b) continue;
      const key = `${a.id}\u0000${b.id}\u0000${e.kind}`;
      if (seen.has(key)) continue;
      seen.add(key);
      this.edges.push({ a, b, kind: e.kind, weight: e.weight });
    }
    this.structure = [];
    for (const n of this.nodes) {
      if (n.parent && byId.has(n.parent)) this.structure.push({ a: byId.get(n.parent), b: n });
    }
    this.byId = byId;
    this.simulate();
    this.resize();
    this.fit();
    return this.nodes.length;
  }

  simulate() {
    const nodes = this.nodes;
    const n = nodes.length;
    const springs = [
      ...this.structure.map((s) => ({ a: s.a, b: s.b, len: 22 + s.b.r, k: 0.08 })),
      ...this.edges.map((e) => ({
        a: e.a,
        b: e.b,
        len: e.kind === "link" ? 90 : e.kind === "term" ? 120 : 110,
        k: e.kind === "sim" ? 0.012 * Math.max(0.2, e.weight) : 0.02,
      })),
    ];
    let heat = 1;
    for (let it = 0; it < ITERATIONS; it++) {
      for (let i = 0; i < n; i++) {
        const p = nodes[i];
        for (let j = i + 1; j < n; j++) {
          const q = nodes[j];
          let dx = p.x - q.x;
          let dy = p.y - q.y;
          let d2 = dx * dx + dy * dy;
          if (d2 < 0.01) {
            dx = 0.1;
            dy = 0.1;
            d2 = 0.02;
          }
          if (d2 > 160000) continue;
          const f = (p.room && q.room ? 2600 : 420) / d2;
          const d = Math.sqrt(d2);
          const fx = (dx / d) * f;
          const fy = (dy / d) * f;
          p.vx += fx;
          p.vy += fy;
          q.vx -= fx;
          q.vy -= fy;
        }
      }
      for (const s of springs) {
        const dx = s.b.x - s.a.x;
        const dy = s.b.y - s.a.y;
        const d = Math.sqrt(dx * dx + dy * dy) || 0.01;
        const f = (d - s.len) * s.k;
        const fx = (dx / d) * f;
        const fy = (dy / d) * f;
        s.a.vx += fx;
        s.a.vy += fy;
        s.b.vx -= fx;
        s.b.vy -= fy;
      }
      for (const p of nodes) {
        p.vx += (p.cx - p.x) * 0.016 - p.x * 0.002;
        p.vy += (p.cy - p.y) * 0.016 - p.y * 0.002;
        const v = Math.sqrt(p.vx * p.vx + p.vy * p.vy);
        const cap = 12 * heat + 0.5;
        const s = v > cap ? cap / v : 1;
        p.x += p.vx * s;
        p.y += p.vy * s;
        p.vx *= 0.55;
        p.vy *= 0.55;
      }
      heat *= 0.985;
    }
  }

  resize() {
    const rect = this.canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    const dpr = window.devicePixelRatio || 1;
    this.canvas.width = Math.round(rect.width * dpr);
    this.canvas.height = Math.round(rect.height * dpr);
    this.w = rect.width;
    this.h = rect.height;
    this.dpr = dpr;
    if (!this.fitted && this.nodes.length) this.fit();
    this.draw();
  }

  fit() {
    if (!this.nodes.length || !this.w) return;
    let x0 = Infinity;
    let y0 = Infinity;
    let x1 = -Infinity;
    let y1 = -Infinity;
    for (const p of this.nodes) {
      x0 = Math.min(x0, p.x - p.r);
      y0 = Math.min(y0, p.y - p.r);
      x1 = Math.max(x1, p.x + p.r);
      y1 = Math.max(y1, p.y + p.r);
    }
    const pad = 64;
    const k = Math.min((this.w - pad * 2) / Math.max(1, x1 - x0), (this.h - pad * 2) / Math.max(1, y1 - y0), 1.6);
    this.view = { k, x: this.w / 2 - ((x0 + x1) / 2) * k, y: this.h / 2 - ((y0 + y1) / 2) * k };
    this.fitted = true;
    this.draw();
  }

  zoomBy(f, cx = this.w / 2, cy = this.h / 2) {
    const k = Math.min(8, Math.max(0.1, this.view.k * f));
    const r = k / this.view.k;
    this.view = { k, x: cx - (cx - this.view.x) * r, y: cy - (cy - this.view.y) * r };
    this.draw();
  }

  setKinds(kinds) {
    this.kinds = kinds;
    this.draw();
  }

  select(id) {
    this.selected = id ? (this.byId?.get(id) ?? this.byId?.get(id.split("/")[0]) ?? null) : null;
    this.draw();
  }

  toScreen(p) {
    return { x: p.x * this.view.k + this.view.x, y: p.y * this.view.k + this.view.y };
  }

  nodeAt(sx, sy) {
    let best = null;
    let bestD = Infinity;
    for (const p of this.nodes) {
      const s = this.toScreen(p);
      const d = Math.hypot(s.x - sx, s.y - sy);
      const reach = Math.max(6, p.r * this.view.k) + 4;
      if (d < reach && d < bestD) {
        best = p;
        bestD = d;
      }
    }
    return best;
  }

  draw() {
    const { ctx } = this;
    if (!ctx || !this.w) return;
    const colours = Array.from({ length: 8 }, (_, i) => token(`--g${i + 1}`));
    const fg = token("--foreground");
    const muted = token("--muted-foreground");
    const bg = token("--background");
    const edge = token("--edge");
    const primary = token("--primary");
    const sans = token("--font-sans");

    ctx.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    ctx.clearRect(0, 0, this.w, this.h);
    ctx.save();
    ctx.translate(this.view.x, this.view.y);
    ctx.scale(this.view.k, this.view.k);
    const px = 1 / this.view.k;

    const focus = this.hover ?? this.selected;
    const near = new Set();
    if (focus) {
      for (const e of this.edges) {
        if (e.a === focus) near.add(e.b);
        if (e.b === focus) near.add(e.a);
      }
    }

    ctx.lineWidth = px;
    ctx.strokeStyle = edge;
    ctx.globalAlpha = 0.6;
    ctx.beginPath();
    for (const s of this.structure) {
      ctx.moveTo(s.a.x, s.a.y);
      ctx.lineTo(s.b.x, s.b.y);
    }
    ctx.stroke();

    for (const kind of ["sim", "term", "link"]) {
      if (!this.kinds.has(kind)) continue;
      ctx.setLineDash(kind === "term" ? [4 * px, 3 * px] : []);
      ctx.strokeStyle = kind === "link" ? fg : kind === "term" ? muted : edge;
      ctx.lineWidth = (kind === "link" ? 1.4 : 1) * px;
      for (const e of this.edges) {
        if (e.kind !== kind) continue;
        const lit = focus && (e.a === focus || e.b === focus);
        ctx.globalAlpha = focus ? (lit ? 0.95 : 0.08) : kind === "link" ? 0.75 : kind === "term" ? 0.28 : 0.2 + 0.35 * Math.min(1, e.weight);
        ctx.beginPath();
        ctx.moveTo(e.a.x, e.a.y);
        ctx.lineTo(e.b.x, e.b.y);
        ctx.stroke();
      }
    }
    ctx.setLineDash([]);

    for (const p of this.nodes) {
      const dim = focus && p !== focus && !near.has(p) && p.parent !== focus.id && focus.parent !== p.id;
      ctx.globalAlpha = dim ? 0.3 : 1;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
      if (p.room) {
        ctx.fillStyle = colours[p.group % 8];
        ctx.fill();
      } else {
        ctx.fillStyle = bg;
        ctx.fill();
        ctx.lineWidth = 1.5 * px;
        ctx.strokeStyle = colours[p.group % 8];
        ctx.stroke();
      }
    }

    if (this.selected) {
      const p = this.selected;
      ctx.globalAlpha = 1;
      ctx.lineWidth = 2 * px;
      ctx.strokeStyle = primary;
      ctx.beginPath();
      ctx.arc(p.x, p.y, p.r + 4 * px, 0, Math.PI * 2);
      ctx.stroke();
    }

    // Labels: the focused door and its neighbours first, then rooms by price,
    // each only where it does not cover a label already drawn.
    ctx.font = `500 ${12 * px}px ${sans}`;
    ctx.textBaseline = "middle";
    ctx.lineJoin = "round";
    const placed = [];
    const candidates = this.nodes
      .map((p) => ({ p, important: p === focus || p === this.selected || near.has(p) }))
      .filter(({ p, important }) => p.room || important)
      .sort((a, b) => Number(b.important) - Number(a.important) || b.p.tokens - a.p.tokens);
    for (const { p, important } of candidates) {
      const text = p.title || p.id;
      const label = text.length > 34 ? `${text.slice(0, 33)}…` : text;
      const x = p.x + p.r + 5 * px;
      const w = ctx.measureText(label).width;
      const box = { x0: x - 2 * px, x1: x + w + 2 * px, y0: p.y - 8 * px, y1: p.y + 8 * px };
      if (placed.some((b) => box.x0 < b.x1 && box.x1 > b.x0 && box.y0 < b.y1 && box.y1 > b.y0)) continue;
      placed.push(box);
      ctx.globalAlpha = focus && !important ? 0.35 : 1;
      ctx.lineWidth = 3 * px;
      ctx.strokeStyle = bg;
      ctx.strokeText(label, x, p.y);
      ctx.fillStyle = important ? fg : muted;
      ctx.fillText(label, x, p.y);
    }
    ctx.restore();
    ctx.globalAlpha = 1;
  }

  listen() {
    const c = this.canvas;
    c.addEventListener("pointerdown", (e) => {
      c.setPointerCapture(e.pointerId);
      this.drag = { x: e.offsetX, y: e.offsetY, vx: this.view.x, vy: this.view.y, moved: false };
    });
    c.addEventListener("pointermove", (e) => {
      if (this.drag) {
        const dx = e.offsetX - this.drag.x;
        const dy = e.offsetY - this.drag.y;
        if (Math.abs(dx) + Math.abs(dy) > 3) this.drag.moved = true;
        if (this.drag.moved) {
          c.classList.add("dragging");
          this.view.x = this.drag.vx + dx;
          this.view.y = this.drag.vy + dy;
          this.draw();
          return;
        }
      }
      const p = this.nodeAt(e.offsetX, e.offsetY);
      if (p !== this.hover) {
        this.hover = p;
        c.classList.toggle("pointing", Boolean(p));
        this.draw();
      }
      this.showTip(p, e.offsetX, e.offsetY);
    });
    c.addEventListener("pointerup", (e) => {
      const wasDrag = this.drag?.moved;
      this.drag = null;
      c.classList.remove("dragging");
      if (wasDrag) return;
      const p = this.nodeAt(e.offsetX, e.offsetY);
      if (p) this.onOpen(p.id);
    });
    c.addEventListener("pointerleave", () => {
      this.hover = null;
      this.showTip(null);
      this.draw();
    });
    c.addEventListener(
      "wheel",
      (e) => {
        e.preventDefault();
        this.zoomBy(Math.exp(-e.deltaY * 0.0015), e.offsetX, e.offsetY);
      },
      { passive: false },
    );
  }

  showTip(p, x, y) {
    if (!p) {
      this.tip.hidden = true;
      return;
    }
    this.tip.replaceChildren(
      document.createTextNode(p.title || p.id),
      Object.assign(document.createElement("span"), { className: "mono", textContent: `${p.id} · ${p.tokens} t` }),
    );
    this.tip.hidden = false;
    const left = Math.min(x + 14, this.w - this.tip.offsetWidth - 8);
    const top = Math.min(y + 14, this.h - this.tip.offsetHeight - 8);
    this.tip.style.setProperty("left", `${Math.max(8, left)}px`);
    this.tip.style.setProperty("top", `${Math.max(8, top)}px`);
  }
}
