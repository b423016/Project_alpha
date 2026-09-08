/* Overlay terminal logic. Zero-build ES2020 served from the binary.
   Fail-closed in the UI too: render errors, never invent rows or numbers. */
"use strict";

const PAGES = ["overview", "chain", "surface", "blotter", "agents", "settings", "names", "map"];

function readFocused() {
  try {
    const s = sessionStorage.getItem("nr_focused");
    if (s && /^[A-Z.]{1,8}$/.test(s)) return s;
  } catch {
    /* private mode */
  }
  return "AAPL";
}

function writeFocused(sym) {
  try {
    sessionStorage.setItem("nr_focused", sym);
  } catch {
    /* private mode */
  }
}

const state = {
  snap: null,
  chain: null,
  top20: null,
  policy: null,
  agents: null,
  blotter: null,
  broker: null,
  universe: null,
  focused: readFocused(),
  mapGrid: null,
  mapSym: null,
  lastFeatures: null,
  lastSuggest: null,
  bars: null,
  filters: { dteMin: "", dteMax: "", deltaMin: "", deltaMax: "", top20Only: true },
};

const $ = (id) => document.getElementById(id);
const set = (id, text) => {
  const el = $(id);
  if (el) el.textContent = text;
};
const fmt = (x, d) => (typeof x === "number" && Number.isFinite(x) ? x.toFixed(d) : "—");

function formatAge(ageMs) {
  if (ageMs == null) return "—";
  if (ageMs < 60_000) return `${Math.round(ageMs / 1000)}s`;
  if (ageMs < 3_600_000) return `${Math.round(ageMs / 60_000)}m`;
  if (ageMs < 86_400_000) return `${Math.round(ageMs / 3_600_000)}h`;
  return `${Math.round(ageMs / 86_400_000)}d`;
}

function msg(text, cls) {
  const el = $("sb-msg");
  if (!el) return;
  el.textContent = text;
  el.className = `msg ${cls ?? ""}`;
}

async function getJson(path) {
  const res = await fetch(path, { cache: "no-store" });
  if (!res.ok) throw new Error(`${path} -> ${res.status}`);
  return res.json();
}

/* ---------- routing ---------- */

function currentRoute() {
  const h = location.hash.replace(/^#/, "");
  return PAGES.includes(h) ? h : "overview";
}

function applyRoute() {
  const route = currentRoute();
  for (const p of PAGES) {
    $(`page-${p}`)?.classList.toggle("on", p === route);
  }
  document.querySelectorAll("#tabs a").forEach((a) => {
    a.classList.toggle("on", a.getAttribute("href") === `#${route}`);
  });
}

/* Pages use display:none; canvases have 0 layout until two frames after .on. */
function afterLayout(fn) {
  requestAnimationFrame(() => requestAnimationFrame(fn));
}

function goto(route) {
  location.hash = `#${route}`;
}

/* ---------- renderers ---------- */

function top20Map() {
  const m = new Map();
  for (const r of state.top20?.rows ?? []) m.set(r.contract?.occ, r);
  return m;
}

function isKilled() {
  return Boolean(state.blotter?.killed || state.agents?.killed || state.snap?.killed);
}

function renderChrome() {
  const s = state.snap;
  if (!s) return;
  $("c-under").textContent = fmt(s.under_price, 2);
  $("c-badge").textContent = s.delayed_badge ?? "DELAYED";
  const ageMs =
    typeof s.asof_unix_ms === "number" ? Math.max(0, Date.now() - s.asof_unix_ms) : null;
  $("c-age").textContent = formatAge(ageMs);
  const h = state.agents?.decide_hist;
  $("c-decide").textContent = h && h.n > 0 ? `~${Math.round(h.sum_ms / h.n)}ms` : "—";
  const br = state.broker;
  set("c-alpaca", br?.alpaca ?? "—");
  set("c-claude", br?.claude_configured ? "on" : "off");
  set("ov-alpaca", br ? `${br.status ?? br.alpaca} ${br.account ?? ""}` : "—");
  set("ov-equity", br?.equity ?? "—");
  if (isKilled()) {
    msg("KILL ENGAGED — kernel refuses new tickets until restart", "err");
  }
}

function renderOverview() {
  const s = state.snap;
  if (!s) return;
  set("ov-snapshot", s.snapshot_id ?? "—");
  set("ov-under-price", fmt(s.under_price, 2));
  // Book $Δ needs the position feed (Alpaca recon bit); never fake it.
  const pick0 = state.top20?.rows?.[0];
  if (pick0?.greeks && Number.isFinite(s.under_price)) {
    const d1 = Math.abs(pick0.greeks.delta) * 100 * s.under_price;
    set("ov-band", `1-lot pick Δ$ ${d1.toFixed(0)} (book pending)`);
  } else {
    set("ov-band", "pending position feed");
  }
  const killed = isKilled();
  set("ov-posture", killed ? "KILLED" : "HOLD");
  const k = $("ov-killed");
  if (k) {
    k.textContent = killed ? "KILLED" : "armed";
    k.className = killed ? "v dn" : "v";
  }
  set("ov-source", s.source ?? "—");
  set("ov-n-contracts", String(s.n_contracts ?? "—"));

  const pick = state.top20?.rows?.[0];
  const box = $("ov-pick");
  if (box && pick) {
    const c = pick.contract;
    box.innerHTML = "";
    for (const [k2, v, cls] of [
      ["occ", c.occ],
      ["exp / dte", `${c.expiry} · ${c.dte}d`],
      ["strike", fmt(c.strike, 1)],
      ["Δ", fmt(pick.greeks?.delta, 3)],
      ["mid", fmt((c.bid + c.ask) / 2, 2)],
      ["utility λ·|Δ|/mid", fmt(pick.utility, 4)],
    ]) {
      const kk = document.createElement("span");
      kk.className = "k";
      kk.textContent = k2;
      const vv = document.createElement("span");
      vv.className = `v ${cls ?? ""}`;
      vv.textContent = v;
      box.append(kk, vv);
    }
  } else if (box) {
    box.innerHTML = '<span class="k">funnel</span><span class="v">empty</span>';
  }
  set("ov-h-delta", "—");
}

function passesFilters(c, greeks) {
  const f = state.filters;
  if (f.dteMin !== "" && c.dte < Number(f.dteMin)) return false;
  if (f.dteMax !== "" && c.dte > Number(f.dteMax)) return false;
  const absD = greeks != null && Number.isFinite(greeks.delta) ? Math.abs(greeks.delta) : null;
  if (f.deltaMin !== "") {
    if (absD === null || absD < Number(f.deltaMin)) return false;
  }
  if (f.deltaMax !== "") {
    if (absD === null || absD > Number(f.deltaMax)) return false;
  }
  return true;
}

function renderChain() {
  const body = $("chain-body");
  if (!body || !state.chain) return;
  const tmap = top20Map();
  const f = state.filters;
  
  // Group by strike
  const strikes = new Map();
  for (const c of state.chain.rows ?? []) {
    const g = tmap.get(c.occ)?.greeks ?? null;
    if (f.top20Only && !tmap.has(c.occ)) continue;
    if (!passesFilters(c, g)) continue;
    
    if (!strikes.has(c.strike)) strikes.set(c.strike, { call: null, put: null });
    if (c.right === "Call") strikes.get(c.strike).call = { c, g, inTop: tmap.has(c.occ) };
    else strikes.get(c.strike).put = { c, g, inTop: tmap.has(c.occ) };
  }
  
  const sortedStrikes = Array.from(strikes.entries()).sort((a, b) => a[0] - b[0]);
  
  $("chain-showing").textContent = `showing ${sortedStrikes.length} strikes`;
  body.textContent = "";
  if (!sortedStrikes.length) {
    body.innerHTML = '<tr><td colspan="9" class="dim" style="text-align:center">no strikes match filters</td></tr>';
    return;
  }
  
  const underPrice = state.snap?.under_price || 0;
  
  // Use DocumentFragment to prevent DOM thrashing
  const fragment = document.createDocumentFragment();
  
  for (const [k, { call, put }] of sortedStrikes) {
    const tr = document.createElement("tr");
    
    // Calls
    const ctds = [];
    if (call) {
      const isItm = k < underPrice;
      const cls = call.inTop ? "top20 itm-call" : (isItm ? "itm-call" : "");
      ctds.push(`<td class="${cls}">${fmt(call.c.bid, 2)}</td>`);
      ctds.push(`<td class="${cls}">${fmt(call.c.ask, 2)}</td>`);
      ctds.push(`<td class="${cls}">${call.c.volume}</td>`);
      ctds.push(`<td class="${cls}">${call.g ? fmt(call.g.delta, 3) : "—"}</td>`);
    } else {
      ctds.push(`<td colspan="4" class="dim" style="text-align:center">—</td>`);
    }
    
    // Strike
    const ktd = `<td class="strike-col">${fmt(k, 1)}</td>`;
    
    // Puts
    const ptds = [];
    if (put) {
      const isItm = k > underPrice;
      const cls = put.inTop ? "top20 itm-put" : (isItm ? "itm-put" : "");
      ptds.push(`<td class="${cls}">${fmt(put.c.bid, 2)}</td>`);
      ptds.push(`<td class="${cls}">${fmt(put.c.ask, 2)}</td>`);
      ptds.push(`<td class="${cls}">${put.c.volume}</td>`);
      ptds.push(`<td class="${cls}">${put.g ? fmt(put.g.delta, 3) : "—"}</td>`);
    } else {
      ptds.push(`<td colspan="4" class="dim" style="text-align:center">—</td>`);
    }
    
    tr.innerHTML = ctds.join("") + ktd + ptds.join("");
    fragment.appendChild(tr);
  }
  
  body.appendChild(fragment);
}

function renderBlotter() {
  const b = state.blotter;
  const body = $("blotter-body");
  if (!b || !body) return;
  const orders = b.orders ?? [];
  const pick = state.top20?.rows?.[0];
  const draft = $("blot-draft");
  if (draft && pick) {
    kvRows(draft, [
      ["occ", pick.contract?.occ ?? "—"],
      ["Δ", fmt(pick.greeks?.delta, 3)],
      ["mid", fmt(((pick.contract?.bid ?? 0) + (pick.contract?.ask ?? 0)) / 2, 2)],
      ["status", "press HEDGE to send paper (gate still owns submit)"],
    ]);
  }
  if (!orders.length) {
    body.innerHTML =
      '<tr><td colspan="8" class="dim">no orders yet — press hedge or wait for a band breach</td></tr>';
    return;
  }
  body.textContent = "";
  for (const o of orders) {
    const tr = document.createElement("tr");
    for (const cell of [
      "—",
      o.client_order_id ?? "",
      o.occ ?? "",
      "BUY",
      String(o.qty ?? ""),
      "—",
      "IOC",
      o.state ?? "",
    ]) {
      const td = document.createElement("td");
      td.textContent = cell;
      tr.appendChild(td);
    }
    body.appendChild(tr);
  }
}

function renderAgents() {
  const p = state.policy;
  if (p) {
    const risk = $("set-risk");
    if (risk) {
      risk.innerHTML = "";
      for (const [k, v] of [
        ["regime", p.regime ?? "unknown"],
        ["DTE band", `${p.dte_min}–${p.dte_max}d`],
        ["put Δ band", `${fmt(p.delta_min, 2)} … ${fmt(p.delta_max, 2)}`],
        ["premium cap", `$${(p.max_premium_cents / 100).toFixed(0)}`],
        ["λ svi/pca/eff", `${fmt(p.lambda_svi, 2)} / ${fmt(p.lambda_pca, 2)} / ${fmt(p.lambda_eff, 2)}`],
        ["policy_id", p.policy_id],
      ]) {
        const kk = document.createElement("span");
        kk.className = "k";
        kk.textContent = k;
        const vv = document.createElement("span");
        vv.className = "v";
        vv.textContent = String(v);
        risk.append(kk, vv);
      }
    }
  }
  const h = state.agents?.decide_hist;
  const hist = $("ag-hist");
  const labels = $("ag-hist-labels");
  if (hist && labels && h) {
    hist.innerHTML = "";
    labels.innerHTML = "";
    const max = Math.max(1, ...(h.counts ?? [0]));
    (h.labels ?? []).forEach((lab, i) => {
      const bar = document.createElement("div");
      bar.className = `bar${i <= 4 ? " hot" : ""}`;
      bar.style.height = `${Math.round(((h.counts?.[i] ?? 0) / max) * 100)}%`;
      const num = document.createElement("span");
      num.textContent = String(h.counts?.[i] ?? 0);
      bar.appendChild(num);
      hist.appendChild(bar);
      const li = document.createElement("i");
      li.textContent = lab;
      labels.appendChild(li);
    });
    set("ag-n", String(h.n ?? 0));
    set("ag-sum", String(h.sum_ms ?? 0));
  } else if (hist && labels) {
    hist.innerHTML = '<div class="dim">no decide_ms samples yet</div>';
    labels.innerHTML = "";
    set("ag-n", "0");
    set("ag-sum", "0");
  }
  set("set-source", state.snap?.source ?? "—");
  const on = (v) => (v ? "on" : "off");
  set("set-llm-s", on(state.agents?.llm_strategist));
  set("set-llm-q", on(state.agents?.llm_quant));
  set("set-llm-n", on(state.agents?.llm_names));
  const ageMs =
    typeof state.snap?.asof_unix_ms === "number"
      ? Math.max(0, Date.now() - state.snap.asof_unix_ms)
      : null;
  set("set-stale", ageMs == null ? "—" : ageMs > 900_000 ? "STALE" : "ok");

  paintAgentNodes();
  if (!window._activeAgent) selectAgent("strategist");
  else updateAgentMemory(window._activeAgent);
  if (currentRoute() === "agents") startAgentGraph();
  else stopAgentGraph();
}

function kvRows(el, rows) {
  if (!el) return;
  el.innerHTML = "";
  for (const [k, v, cls] of rows) {
    const kk = document.createElement("span");
    kk.className = "k";
    kk.textContent = k;
    const vv = document.createElement("span");
    vv.className = `v ${cls ?? ""}`;
    vv.textContent = v;
    el.append(kk, vv);
  }
}

/* Line chart on canvas, sized to fill whatever the flex box gives it —
   not a caption-sized sparkline. Redrawn on resize/route-return since the
   canvas has no layout while its page is display:none. */
function drawLineChart(canvas, closes, up) {
  const wrap = canvas.parentElement;
  const w = Math.max(wrap?.clientWidth || 0, 240);
  const h = Math.max(wrap?.clientHeight || 0, 140);
  if (w < 2 || h < 2) return;
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.clearRect(0, 0, w, h);

  const lo = Math.min(...closes);
  const hi = Math.max(...closes);
  const span = hi - lo || 1;
  const pad = 4;
  const stepX = closes.length > 1 ? (w - pad * 2) / (closes.length - 1) : 0;
  const yFor = (c) => h - pad - ((c - lo) / span) * (h - pad * 2);

  ctx.strokeStyle = "rgba(255,255,255,0.08)";
  ctx.lineWidth = 1;
  for (let i = 1; i < 4; i++) {
    const y = Math.round((h / 4) * i) + 0.5;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(w, y);
    ctx.stroke();
  }

  const lineColor = up ? "#00ff00" : "#ff0000";
  ctx.beginPath();
  ctx.moveTo(pad, yFor(closes[0]));
  closes.forEach((c, i) => ctx.lineTo(pad + i * stepX, yFor(c)));
  ctx.lineTo(pad + (closes.length - 1) * stepX, h);
  ctx.lineTo(pad, h);
  ctx.closePath();
  ctx.fillStyle = up ? "rgba(0,255,0,0.10)" : "rgba(255,0,0,0.10)";
  ctx.fill();

  ctx.beginPath();
  closes.forEach((c, i) => {
    const x = pad + i * stepX;
    const y = yFor(c);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.strokeStyle = lineColor;
  ctx.lineWidth = 1.5;
  ctx.stroke();
}

function renderChart(bars) {
  const canvas = $("nm-chart-canvas");
  const stats = $("nm-chart-stats");
  if (!canvas) return;
  if (!bars.length) {
    if (stats) stats.textContent = "no bars";
    return;
  }
  state.bars = bars;
  const closes = bars.map((b) => b.c);
  const last = closes[closes.length - 1];
  const chg = closes[0] ? ((last - closes[0]) / closes[0]) * 100 : 0;
  const hi = Math.max(...closes);
  const lo = Math.min(...closes);
  drawLineChart(canvas, closes, chg >= 0);
  if (stats) {
    stats.textContent =
      `last ${fmt(last, 2)}  ${chg >= 0 ? "+" : ""}${fmt(chg, 2)}%  ·  hi ${fmt(hi, 2)}  lo ${fmt(lo, 2)}  (${bars.length}b)`;
  }
}

function featureRows(f) {
  const rows = [["status", f.stale ? "STALE — HOLD" : "live", f.stale ? "dn" : "up"]];
  for (const c of f.cells ?? []) {
    rows.push([`${c.name} (${c.units})`, `${fmt(c.value, 4)} — ${c.meaning}`]);
  }
  return rows;
}

function renderFeatures(f) {
  kvRows($("nm-feat"), featureRows(f));
}

function renderSuggest(body, ok) {
  const why = $("nm-why");
  if (!ok) {
    kvRows($("nm-sug"), [
      ["rejected", body.code ?? "ERROR", "dn"],
      ["field", body.field ?? "—"],
      ["got", body.got ?? "—"],
    ]);
    if (why) why.textContent = "rejected — bound check failed. Never a submit either way.";
    return;
  }
  const cls = body.side === "LONG" ? "up" : body.side === "SHORT" ? "dn" : "dim";
  kvRows($("nm-sug"), [
    ["side", body.side ?? "HOLD", cls],
    ["horizon", `${body.horizon_bars ?? "—"} bars`],
    ["conf", fmt(body.conf, 2)],
  ]);
  if (why) why.textContent = body.why ?? "";
}

/* Real |return| grid from bars, shaded by magnitude — quoted IV grid once
   the names desk has an options chain (see surface `note`). */
function heatmapText(grid, sym, note) {
  if (!grid?.length) return "no data";
  const shades = " .:-=+*#%@";
  let max = 0;
  for (const row of grid) for (const v of row) if (Number.isFinite(v) && v > max) max = v;
  const lines = grid.map((row) =>
    row
      .map((v) => shades[Math.min(shades.length - 1, Math.floor((v / (max || 1)) * (shades.length - 1)))])
      .join(" "),
  );
  return `${sym} — |return| grid, darker = bigger move\n${note ?? ""}\n\n${lines.join("\n")}`;
}

function drawHeatmap(canvas, grid) {
  const wrap = canvas.parentElement;
  const w = Math.max(wrap?.clientWidth || 0, 240);
  const h = Math.max(wrap?.clientHeight || 0, 180);
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx || !grid?.length) return;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, w, h);
  const rows = grid.length;
  const cols = grid[0].length || 1;
  let max = 0;
  for (const row of grid) for (const v of row) if (Number.isFinite(v) && v > max) max = v;
  const cw = w / cols;
  const ch = h / rows;
  for (let i = 0; i < rows; i++) {
    for (let j = 0; j < cols; j++) {
      const t = max > 0 ? grid[i][j] / max : 0;
      const r = Math.round(255 * t);
      const g = Math.round(160 * (1 - t));
      ctx.fillStyle = `rgb(${r},${g},0)`;
      ctx.fillRect(j * cw, i * ch, cw + 0.5, ch + 0.5);
    }
  }
}

function ivGridFromPoints(points) {
  const exps = [...new Set(points.map((p) => p.expiry))].sort();
  const strikes = [...new Set(points.map((p) => p.strike))].sort((a, b) => a - b);
  if (!exps.length || !strikes.length) return [];
  const takeS = strikes.length > 24 ? strikes.filter((_, i) => i % Math.ceil(strikes.length / 24) === 0) : strikes;
  const takeE = exps.length > 12 ? exps.filter((_, i) => i % Math.ceil(exps.length / 12) === 0) : exps;
  const lookup = new Map(points.map((p) => [`${p.expiry}|${p.strike}`, p.iv]));
  return takeE.map((e) => takeS.map((k) => lookup.get(`${e}|${k}`) ?? 0));
}

async function renderSurface() {
  const body = $("surf-body");
  if (!body) return;
  try {
    const s = await getJson("/api/surface");
    set("surf-note", `${s.note ?? ""} · ${s.n ?? 0} pts · ${s.underlying} ${fmt(s.under_price, 2)}`);
    body.textContent = "";
    for (const r of s.by_expiry ?? []) {
      const tr = document.createElement("tr");
      for (const cell of [
        r.expiry,
        String(r.dte),
        fmt(r.atm_iv, 3),
        r.skew == null ? "—" : fmt(r.skew, 3),
        String(r.n),
      ]) {
        const td = document.createElement("td");
        td.textContent = cell;
        tr.appendChild(td);
      }
      body.appendChild(tr);
    }
    if (!(s.by_expiry ?? []).length) {
      body.innerHTML = '<tr><td colspan="5" class="dim">no IV points</td></tr>';
    }
    const canvas = $("surf-canvas");
    if (canvas && s.points) drawHeatmap(canvas, ivGridFromPoints(s.points));
  } catch (e) {
    if (body) body.innerHTML = `<tr><td colspan="5" class="err">${e.message}</td></tr>`;
  }
}

function paintMapPanels(s) {
  const canvas = $("map-canvas");
  const note = $("map-note");
  const feat = s?.features ?? state.lastFeatures;
  const stale = Boolean(feat?.stale) || !feat;
  const lean = state.lastSuggest;
  if (note && s) {
    note.textContent = `${s.symbol} · ${s.kind ?? "heatmap"} · ${s.note ?? ""}`;
  }
  if (canvas && state.mapGrid?.length) drawHeatmap(canvas, state.mapGrid);
  kvRows(
    $("map-math"),
    feat
      ? [["symbol", feat.symbol ?? state.focused], ...featureRows(feat)]
      : [
          ["symbol", state.focused],
          ["grid", `${(state.mapGrid ?? []).length}×${(state.mapGrid?.[0] ?? []).length}`],
        ],
  );
  kvRows($("map-gate"), [
    ["name", state.focused, "up"],
    ["picked", "tab 7 Names — already focused, no re-pick"],
    ["bars → features", feat ? (stale ? "STALE → HOLD" : "ok") : "loading…", stale ? "dn" : "up"],
    ["rsi_14", feat ? fmt(feat.rsi_14, 1) : "—"],
    ["range_pos", feat ? fmt(feat.range_pos, 2) : "—"],
    ["lean (not an order)", lean ? `${lean.side ?? "HOLD"} conf ${fmt(lean.conf, 2)}` : "from pane D"],
    ["Broker::submit", "blocked — Map never sends"],
  ]);
}

async function renderMap() {
  const q = $("map-q");
  if (q && document.activeElement !== q) q.value = state.focused;
  set("map-sym", state.focused);
  if (state.mapSym === state.focused && state.mapGrid?.length) paintMapPanels({
    symbol: state.focused,
    kind: "return_abs_heatmap",
    features: state.lastFeatures,
    note: "cached from tab 7",
  });
  else {
    kvRows($("map-gate"), [
      ["name", state.focused, "up"],
      ["status", "loading surface…"],
    ]);
  }
  try {
    const s = await getJson(`/api/names/${state.focused}/surface`);
    if (s.symbol && s.symbol !== state.focused) return;
    state.mapGrid = s.grid ?? [];
    state.mapSym = s.symbol ?? state.focused;
    if (s.features) state.lastFeatures = s.features;
    paintMapPanels(s);
  } catch (e) {
    const note = $("map-note");
    if (note) note.textContent = `map: ${e.message}`;
    kvRows($("map-gate"), [
      ["name", state.focused],
      ["error", e.message, "dn"],
      ["hint", "name must be in the tab 7 universe"],
    ]);
  }
}

async function loadFocusedPanes(sym) {
  set("nm-chart-stats", "loading…");
  kvRows($("nm-feat"), [["—", "loading…"]]);
  kvRows($("nm-sug"), [["side", "loading…"]]);
  set("nm-why", "");
  try {
    const b = await getJson(`/api/names/${sym}/bars`);
    if (sym === state.focused) renderChart(b.bars ?? []);
  } catch (e) {
    if (sym === state.focused) set("nm-chart-stats", `chart: ${e.message}`);
  }
  try {
    const f = await getJson(`/api/names/${sym}/features`);
    if (sym === state.focused) {
      state.lastFeatures = f;
      renderFeatures(f);
    }
  } catch (e) {
    if (sym === state.focused) kvRows($("nm-feat"), [["error", e.message, "dn"]]);
  }
  try {
    const res = await fetch(`/api/names/${sym}/suggest`, { method: "POST", cache: "no-store" });
    const body = await res.json();
    if (sym === state.focused) {
      if (res.ok) state.lastSuggest = body;
      renderSuggest(body, res.ok);
    }
  } catch (e) {
    if (sym === state.focused) kvRows($("nm-sug"), [["error", e.message, "dn"]]);
  }
  try {
    const s = await getJson(`/api/names/${sym}/surface`);
    if (sym === state.focused) {
      state.mapGrid = s.grid ?? [];
      state.mapSym = s.symbol ?? sym;
      if (s.features) state.lastFeatures = s.features;
    }
  } catch {
    /* Map will retry on its own tab. */
  }
  if (sym === state.focused && currentRoute() === "map") void renderMap();
}

function focusRow(sym) {
  state.focused = String(sym || "AAPL").toUpperCase();
  writeFocused(state.focused);
  const row = state.universe?.rows?.find((r) => r.symbol === state.focused);
  set("nm-sym", state.focused);
  if (row) {
    kvRows($("nm-last"), [
      ["last", `$${fmt(row.last, 2)}`],
      ["volume", row.volume?.toLocaleString() ?? "—"],
      ["exchange", row.exchange],
    ]);
  }
  renderNames();
  void loadFocusedPanes(state.focused);
}

function renderNames() {
  const u = state.universe;
  const body = $("uni-body");
  if (!body) return;
  if (!u?.rows) {
    body.innerHTML = '<tr><td colspan="4" class="dim">universe: unavailable</td></tr>';
    set("uni-n", "");
    return;
  }
  const q = ($("uni-q")?.value ?? "").trim().toUpperCase();
  const rows = q
    ? u.rows.filter((r) => r.symbol.includes(q) || r.name?.toUpperCase().includes(q))
    : u.rows;
  set("uni-n", `showing ${rows.length} of ${u.rows.length}`);
  body.textContent = "";
  if (!rows.length) {
    body.innerHTML = '<tr><td colspan="4" class="dim">no match</td></tr>';
    return;
  }
  const fragment = document.createDocumentFragment();
  for (const r of rows) {
    const tr = document.createElement("tr");
    tr.className = r.symbol === state.focused ? "on" : "";
    for (const cell of [r.symbol, fmt(r.last, 2), r.volume?.toLocaleString() ?? "—", r.exchange]) {
      const td = document.createElement("td");
      td.textContent = cell;
      tr.appendChild(td);
    }
    tr.addEventListener("click", () => focusRow(r.symbol));
    fragment.appendChild(tr);
  }
  body.appendChild(fragment);
}

function renderAll() {
  renderChrome();
  const r = currentRoute();
  if (r === "overview") renderOverview();
  if (r === "chain") renderChain();
  if (r === "surface") {
    void renderSurface();
    afterLayout(() => void renderSurface());
  }
  if (r === "blotter") renderBlotter();
  if (r === "agents" || r === "settings") renderAgents();
  if (r === "names") {
    renderNames();
    afterLayout(() => {
      if (state.bars) renderChart(state.bars);
    });
  }
  if (r === "map") {
    void renderMap();
    afterLayout(() => {
      if (state.mapGrid?.length) drawHeatmap($("map-canvas"), state.mapGrid);
    });
  }
  if (r !== "agents") stopAgentGraph();
}

window.addEventListener("resize", () => {
  const r = currentRoute();
  if (r === "names" && state.bars) renderChart(state.bars);
  if (r === "map" && state.mapGrid?.length) drawHeatmap($("map-canvas"), state.mapGrid);
  if (r === "surface") void renderSurface();
});

/* ---------- data ---------- */

async function refreshAll() {
  try {
    state.snap = await getJson("/api/snapshot");
    renderChrome();
  } catch (e) {
    msg(`snapshot: ${e.message}`, "err");
  }
  try {
    state.chain = await getJson("/api/chain");
    state.top20 = await getJson("/api/top20");
    if (currentRoute() === "chain") renderChain();
    if (currentRoute() === "overview") renderOverview();
  } catch (e) {
    msg(`chain: ${e.message}`, "err");
  }
  try {
    state.broker = await getJson("/api/broker");
    renderChrome();
    if (currentRoute() === "overview") renderOverview();
  } catch (e) {
    msg(`broker: ${e.message}`, "err");
  }
  try {
    state.blotter = await getJson("/api/blotter");
    if (currentRoute() === "blotter") renderBlotter();
  } catch (e) {
    msg(`blotter: ${e.message}`, "err");
  }
  try {
    state.policy = await getJson("/api/policy");
    state.agents = await getJson("/api/agents");
    renderChrome();
    if (currentRoute() === "agents" || currentRoute() === "settings") renderAgents();
  } catch (e) {
    msg(`policy/agents: ${e.message}`, "err");
  }
  try {
    state.universe = await getJson("/api/universe");
    const inUni = state.universe?.rows?.some((r) => r.symbol === state.focused);
    if (!inUni && !state.focused) {
      state.focused = state.universe?.rows?.[0]?.symbol ?? "AAPL";
      writeFocused(state.focused);
    }
    if (currentRoute() === "names") {
      renderNames();
      void loadFocusedPanes(state.focused);
    }
    if (currentRoute() === "map") void renderMap();
    if (currentRoute() === "surface") void renderSurface();
  } catch (e) {
    msg(`universe: ${e.message}`, "err");
  }
}

async function doHedge() {
  const btn = $("hedge");
  if (btn) btn.disabled = true;
  try {
    const res = await fetch("/api/hedge", { method: "POST", cache: "no-store" });
    const body = await res.json();
    await refreshAll();
    if (body.ok && body.duplicate) {
      msg(`already submitted ${body.occ ?? ""} — same client_order_id (idempotent)`, "ok");
    } else if (body.ok) {
      msg(`paper submit ${body.occ} qty ${body.qty} (${body.quant})`, "ok");
    } else {
      msg(`hedge rejected: ${body.reject ?? res.status}`, "err");
    }
  } catch (e) {
    msg(`hedge failed: ${e.message}`, "err");
  } finally {
    if (btn) btn.disabled = false;
  }
}

async function doKill() {
  $("kill").disabled = true;
  try {
    await fetch("/api/kill", { method: "POST" });
    await refreshAll();
    msg("KILL ENGAGED — kernel refuses new tickets until restart", "err");
  } catch (e) {
    msg(`kill failed: ${e.message}`, "err");
  } finally {
    $("kill").disabled = false;
  }
}

/* ---------- events ---------- */

window.addEventListener("hashchange", () => {
  applyRoute();
  renderAll();
});

document.addEventListener("keydown", (ev) => {
  if (ev.target instanceof HTMLInputElement) return;
  const idx = Number(ev.key);
  if (idx >= 1 && idx <= PAGES.length) {
    ev.preventDefault();
    goto(PAGES[idx - 1]);
  } else if (ev.key === "k" || ev.key === "K") {
    ev.preventDefault();
    void doKill();
  }
});

$("kill")?.addEventListener("click", () => void doKill());
$("hedge")?.addEventListener("click", () => void doHedge());

for (const id of ["f-dte-min", "f-dte-max", "f-delta-min", "f-delta-max"]) {
  $(id)?.addEventListener("input", () => {
    state.filters[id.slice(2).replace(/-(\w)/g, (_, ch) => ch.toUpperCase())] = $(id).value.trim();
    renderChain();
  });
}

$("f-top20")?.addEventListener("click", () => {
  state.filters.top20Only = !state.filters.top20Only;
  $("f-top20").classList.toggle("on", state.filters.top20Only);
  renderChain();
});

$("uni-q")?.addEventListener("input", () => renderNames());

$("map-q")?.addEventListener("keydown", (ev) => {
  if (ev.key !== "Enter") return;
  const sym = ev.target.value.trim().toUpperCase();
  if (!sym) return;
  focusRow(sym);
  void renderMap();
});

void (async () => {
  applyRoute();
  await refreshAll();
  if (state.focused) void loadFocusedPanes(state.focused);
  renderAll();
  setInterval(() => void refreshAll(), 30000);
})();


/* ---------- Agent node interactions ---------- */

window._activeAgent = null;

const AGENT_TITLES = {
  ceo: "CEO",
  strategist: "Strategist (LLM)",
  quant: "Quant (LLM)",
  risk: "Risk (Rust)",
  exec: "Exec (Rust)",
};

function flagOn(v) {
  return v ? "on" : "off";
}

function agentCopy(agentId) {
  const a = state.agents ?? {};
  const p = state.policy ?? a.policy ?? {};
  const pick = state.top20?.rows?.[0];
  const orders = state.blotter?.orders ?? [];
  const hist = a.decide_hist ?? {};
  const killed = isKilled();
  const spy = state.snap?.under_price;
  switch (agentId) {
    case "ceo":
      return {
        pad: [
          "Seat: posture only. Does not emit tickets.",
          `kernel: ${killed ? "KILLED — no new tickets" : "armed"}`,
          `paper: ${flagOn(a.paper ?? true)}  claude: ${flagOn(a.claude_configured)}`,
          `SPY last ${fmt(spy, 2)}  source ${state.snap?.source ?? "—"}`,
          `names focused ${state.focused} — lean only, never submit`,
        ].join("\n"),
        board: [
          `snapshot ${state.snap?.snapshot_id ?? "—"}`,
          `n_contracts ${state.snap?.n_contracts ?? "—"}`,
          `universe ${state.universe?.n ?? state.universe?.rows?.length ?? 0}`,
          `decide_ms n=${hist.n ?? 0} sum=${hist.sum_ms ?? 0}`,
        ].join("\n"),
        audit: killed
          ? "KILL engaged — kernel refuses Broker::submit until restart"
          : `live · overlay clock running · names desk ${state.focused}`,
      };
    case "strategist":
      return {
        pad: [
          `LLM_STRATEGIST=${flagOn(a.llm_strategist)}  claude=${flagOn(a.claude_configured)}`,
          "Slow clock only. Never on the tick path. Never computes Greeks/IV/size.",
          a.llm_strategist && a.claude_configured
            ? `last-good policy_id=${p.policy_id ?? "—"} regime=${p.regime ?? "unknown"}`
            : "flag off or no key → file-default policy, last-good held",
          p.reason ? `reason (log-only): ${p.reason}` : "reason: file-default",
        ].join("\n"),
        board: JSON.stringify(
          {
            policy_id: p.policy_id,
            regime: p.regime,
            dte: [p.dte_min, p.dte_max],
            put_delta: [p.delta_min, p.delta_max],
            lambda_eff: p.lambda_eff,
            max_premium_cents: p.max_premium_cents,
          },
          null,
          2,
        ),
        audit: "emit_policy bound to snapshot+policy. fail-closed: bad JSON does not overwrite last-good.",
      };
    case "quant": {
      const n = state.top20?.rows?.length ?? 0;
      return {
        pad: [
          `LLM_QUANT=${flagOn(a.llm_quant)}  (default off until bit-7 vectors green)`,
          `funnel top20 n=${n}`,
          pick
            ? `head ${pick.contract?.occ}  Δ=${fmt(pick.greeks?.delta, 3)}  U=${fmt(pick.utility, 4)}`
            : "funnel empty — no ticket",
          "emit_ticket only on band breach. qty/limit recomputed by kernel (V5).",
        ].join("\n"),
        board: pick
          ? JSON.stringify(
              {
                occ: pick.contract?.occ,
                dte: pick.contract?.dte,
                strike: pick.contract?.strike,
                delta: pick.greeks?.delta,
                utility: pick.utility,
              },
              null,
              2,
            )
          : "no top-of-funnel row",
        audit:
          orders.length > 0
            ? `last blotter ${orders[orders.length - 1].occ} ${orders[orders.length - 1].state}`
            : "no emit_ticket this session — waiting for $Δ band or HEDGE",
      };
    }
    case "risk":
      return {
        pad: [
          "Deterministic Rust gate. Fail closed.",
          `killed=${killed}  rth_only=${flagOn(a.rth_only)}  paper=${flagOn(a.paper ?? true)}`,
          "stale data / bad JSON / id mismatch → no new ticket",
          "side v1 BUY overlay only. qty u32. money i64 cents.",
        ].join("\n"),
        board: JSON.stringify(
          {
            inhibit: killed,
            llm_strategist: a.llm_strategist,
            llm_quant: a.llm_quant,
            llm_names: a.llm_names,
            paper: a.paper,
            rth_only: a.rth_only,
          },
          null,
          2,
        ),
        audit: "gate before send. audit append before Broker::submit. names lean cannot reach EMS.",
      };
    case "exec": {
      const last = orders[orders.length - 1];
      return {
        pad: [
          "EMS: Alpaca paper. Nothing calls Broker::submit except post-gate.",
          `blotter rows=${state.blotter?.rows ?? orders.length}`,
          last
            ? `last ${last.client_order_id?.slice(0, 12)}… ${last.occ} qty ${last.qty} ${last.state}`
            : "idle — no paper order this process",
        ].join("\n"),
        board: orders.length
          ? orders
              .slice(-5)
              .map((o) => `${o.state} ${o.occ} x${o.qty} ${o.client_order_id?.slice(0, 10)}`)
              .join("\n")
          : "empty blotter",
        audit: last
          ? `client_order_id=${last.client_order_id}\nstate=${last.state} occ=${last.occ}`
          : "no submit. press HEDGE on chrome to run gate→paper (SPY put overlay only).",
      };
    }
    default:
      return { pad: "unknown seat", board: "—", audit: "—" };
  }
}

function updateAgentMemory(agentId) {
  set("mem-title", AGENT_TITLES[agentId] || agentId);
  const { pad, board, audit } = agentCopy(agentId);
  set("mem-pad", pad);
  set("mem-board", board);
  set("mem-audit", audit);
}

function selectAgent(id) {
  window._activeAgent = id;
  document.querySelectorAll(".node").forEach((n) => {
    n.classList.toggle("active", n.dataset.agent === id);
  });
  updateAgentMemory(id);
}

function paintAgentNodes() {
  const a = state.agents ?? {};
  const orders = state.blotter?.orders ?? [];
  const hot = {
    ceo: true,
    strategist: Boolean(a.llm_strategist && a.claude_configured),
    quant: Boolean((state.top20?.rows ?? []).length),
    risk: true,
    exec: orders.length > 0,
  };
  document.querySelectorAll(".node").forEach((n) => {
    n.classList.toggle("hot", Boolean(hot[n.dataset.agent]));
  });
}

document.querySelectorAll(".node").forEach((n) => {
  n.addEventListener("click", () => selectAgent(n.dataset.agent));
});

/* ---------- Canvas Graph Animation ---------- */
const agentCanvas = document.getElementById("agent-canvas");
const agentCtx = agentCanvas ? agentCanvas.getContext("2d") : null;
let animFrame = null;
let dashOffset = 0;

function stopAgentGraph() {
  if (animFrame) cancelAnimationFrame(animFrame);
  animFrame = null;
}

function startAgentGraph() {
  afterLayout(() => {
    if (currentRoute() !== "agents") return;
    if (!animFrame) drawAgentGraph();
  });
}

function drawAgentGraph() {
  if (currentRoute() !== "agents" || !agentCanvas || !agentCtx) {
    animFrame = null;
    return;
  }
  const container = agentCanvas.parentElement;
  const w = Math.max(container?.clientWidth || 0, 240);
  const h = Math.max(container?.clientHeight || 0, 180);
  agentCanvas.width = w;
  agentCanvas.height = h;
  agentCtx.clearRect(0, 0, w, h);

  const nodes = {};
  document.querySelectorAll(".node").forEach((n) => {
    nodes[n.dataset.agent] = {
      x: n.offsetLeft + n.offsetWidth / 2,
      y: n.offsetTop + n.offsetHeight / 2,
    };
  });

  const edges = [
    ["ceo", "strategist"],
    ["ceo", "quant"],
    ["strategist", "risk"],
    ["quant", "risk"],
    ["risk", "exec"],
  ];

  agentCtx.lineWidth = 2;
  agentCtx.strokeStyle = "rgba(255, 159, 28, 0.5)";
  agentCtx.shadowColor = "#ff9f1c";
  agentCtx.shadowBlur = 10;
  agentCtx.setLineDash([10, 10]);
  agentCtx.lineDashOffset = -dashOffset;

  edges.forEach(([u, v]) => {
    if (nodes[u] && nodes[v]) {
      agentCtx.beginPath();
      agentCtx.moveTo(nodes[u].x, nodes[u].y);
      agentCtx.lineTo(nodes[v].x, nodes[v].y);
      agentCtx.stroke();
    }
  });

  dashOffset += 0.5;
  animFrame = requestAnimationFrame(drawAgentGraph);
}

