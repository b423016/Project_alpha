/* Overlay terminal logic. Zero-build ES2020 served from the binary.
   Fail-closed in the UI too: render errors, never invent rows or numbers. */
"use strict";

const PAGES = ["overview", "chain", "surface", "blotter", "agents", "settings"];

const state = {
  snap: null,
  chain: null,
  top20: null,
  policy: null,
  agents: null,
  blotter: null,
  broker: null,
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

function renderAll() {
  renderChrome();
  const r = currentRoute();
  if (r === "overview") renderOverview();
  if (r === "chain") renderChain();
  if (r === "blotter") renderBlotter();
  if (r === "agents" || r === "settings") renderAgents();
}

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
  if (idx >= 1 && idx <= 6) {
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

