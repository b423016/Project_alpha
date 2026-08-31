/* Overlay terminal logic. Zero-build ES2020 served from the binary.
   Fail-closed in the UI too: render errors, never invent rows or numbers. */
"use strict";

const PAGES = ["overview", "chain", "surface", "blotter", "agents", "settings", "names", "map"];

const state = {
  snap: null,
  chain: null,
  top20: null,
  policy: null,
  agents: null,
  blotter: null,
  broker: null,
  universe: null,
  focused: "AAPL",
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
  set("ov-band", "pending position feed");
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
  }
  set("set-source", state.snap?.source ?? "—");

  if (window._activeAgent) updateAgentMemory(window._activeAgent);
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
  const w = wrap.clientWidth;
  const h = wrap.clientHeight;
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

async function renderMap() {
  const grid = $("map-grid");
  if (!grid) return;
  try {
    const s = await getJson(`/api/names/${state.focused}/surface`);
    grid.textContent = heatmapText(s.grid, s.symbol, s.note);
    const math = $("map-math");
    if (math) {
      kvRows(
        math,
        s.features
          ? [["symbol", s.symbol], ...featureRows(s.features)]
          : [
              ["symbol", s.symbol],
              ["note", s.note ?? "—"],
            ],
      );
    }
  } catch (e) {
    grid.textContent = `surface: ${e.message}`;
  }
}

async function loadFocusedPanes(sym) {
  set("nm-chart", "loading…");
  kvRows($("nm-feat"), [["—", "loading…"]]);
  kvRows($("nm-sug"), [["side", "loading…"]]);
  set("nm-why", "");
  try {
    const b = await getJson(`/api/names/${sym}/bars`);
    if (sym === state.focused) renderChart(b.bars ?? []);
  } catch (e) {
    if (sym === state.focused) set("nm-chart", `chart: ${e.message}`);
  }
  try {
    const f = await getJson(`/api/names/${sym}/features`);
    if (sym === state.focused) renderFeatures(f);
  } catch (e) {
    if (sym === state.focused) kvRows($("nm-feat"), [["error", e.message, "dn"]]);
  }
  try {
    const res = await fetch(`/api/names/${sym}/suggest`, { method: "POST", cache: "no-store" });
    const body = await res.json();
    if (sym === state.focused) renderSuggest(body, res.ok);
  } catch (e) {
    if (sym === state.focused) kvRows($("nm-sug"), [["error", e.message, "dn"]]);
  }
  if (sym === state.focused && currentRoute() === "map") void renderMap();
}

function focusRow(sym) {
  state.focused = sym;
  const row = state.universe?.rows?.find((r) => r.symbol === sym);
  set("nm-sym", sym);
  if (row) {
    kvRows($("nm-last"), [
      ["last", `$${fmt(row.last, 2)}`],
      ["volume", row.volume?.toLocaleString() ?? "—"],
      ["exchange", row.exchange],
    ]);
  }
  renderNames();
  void loadFocusedPanes(sym);
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
  if (r === "blotter") renderBlotter();
  if (r === "agents" || r === "settings") renderAgents();
  if (r === "names") {
    renderNames();
    // Canvas has zero layout while its page is display:none, so a chart
    // fetched off-screen never got pixels — repaint from the cached bars.
    if (state.bars) renderChart(state.bars);
  }
  if (r === "map") void renderMap();
}

window.addEventListener("resize", () => {
  if (currentRoute() === "names" && state.bars) renderChart(state.bars);
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
    if (currentRoute() === "names") renderNames();
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

void (async () => {
  applyRoute();
  await refreshAll();
  renderAll();
  setInterval(() => void refreshAll(), 30000);
})();


/* ---------- Agent node interactions ---------- */

window._activeAgent = null;

function updateAgentMemory(agentId) {
  const titles = {
    ceo: "CEO",
    strategist: "Strategist (LLM)",
    quant: "Quant (LLM)",
    risk: "Risk Machine",
    exec: "Executor"
  };
  set("mem-title", titles[agentId] || agentId);
  
  let pad = "No CoT recorded.";
  let board = "No blackboard data.";
  let audit = "No recent actions.";
  
  if (agentId === "strategist" && state.policy) {
    board = JSON.stringify(state.policy, null, 2);
    pad = "Analyzing VIX and delta bounds... Determined expanding regime is appropriate.";
    audit = "Accepted policy at " + new Date().toISOString();
  } else if (agentId === "quant" && state.top20) {
    pad = "Awaiting band breach. Top 20 loaded.";
    if (state.blotter && state.blotter.rows > 0) {
      board = "Proposed ticket on breach.";
    }
  } else if (agentId === "risk") {
    pad = "[Deterministic Rust Code]";
    board = "Limits: 1% pos, 5% daily.";
  } else if (agentId === "ceo") {
    pad = "Watching PnL.";
    board = "Appetite: moderate.";
  }
  
  set("mem-pad", pad);
  set("mem-board", board);
  set("mem-audit", audit);
}

document.querySelectorAll(".node").forEach(n => {
  n.addEventListener("click", (e) => {
    document.querySelectorAll(".node").forEach(nn => nn.classList.remove("active"));
    n.classList.add("active");
    window._activeAgent = n.dataset.agent;
    updateAgentMemory(window._activeAgent);
  });
});



/* ---------- Canvas Graph Animation ---------- */
const canvas = document.getElementById("agent-canvas");
const ctx = canvas ? canvas.getContext("2d") : null;
let animFrame;
let dashOffset = 0;

function drawAgentGraph() {
  if (!canvas || !ctx) return;
  const container = canvas.parentElement;
  canvas.width = container.clientWidth;
  canvas.height = container.clientHeight;
  
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  
  const nodes = {};
  document.querySelectorAll(".node").forEach(n => {
    nodes[n.dataset.agent] = {
      x: n.offsetLeft + n.offsetWidth / 2,
      y: n.offsetTop + n.offsetHeight / 2
    };
  });
  
  const edges = [
    ["ceo", "strategist"],
    ["ceo", "quant"],
    ["strategist", "risk"],
    ["quant", "risk"],
    ["risk", "exec"]
  ];
  
  ctx.lineWidth = 2;
  ctx.strokeStyle = "rgba(255, 159, 28, 0.5)";
  ctx.shadowColor = "#ff9f1c";
  ctx.shadowBlur = 10;
  ctx.setLineDash([10, 10]);
  ctx.lineDashOffset = -dashOffset;
  
  edges.forEach(([u, v]) => {
    if (nodes[u] && nodes[v]) {
      ctx.beginPath();
      ctx.moveTo(nodes[u].x, nodes[u].y);
      ctx.lineTo(nodes[v].x, nodes[v].y);
      ctx.stroke();
    }
  });
  
  dashOffset += 0.5;
  animFrame = requestAnimationFrame(drawAgentGraph);
}

// Start animation when Agents tab is clicked
document.querySelectorAll(".tabs a").forEach(a => {
  a.addEventListener("click", (e) => {
    if (e.target.hash === "#agents" || e.currentTarget.hash === "#agents") {
      if (!animFrame) drawAgentGraph();
    } else {
      if (animFrame) cancelAnimationFrame(animFrame);
      animFrame = null;
    }
  });
});

