/* Overlay terminal. Zero-build plain JS served from the binary (include_str!).
   Fail-closed in the UI too: render errors, never fabricate rows. */
"use strict";

const fmt = (x, d) => (typeof x === "number" && Number.isFinite(x) ? x.toFixed(d) : "—");

async function getJson(path) {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${path} -> ${res.status}`);
  return res.json();
}

function set(id, text) {
  const el = document.getElementById(id);
  if (el) el.textContent = text;
}

function setStatus(text, isError) {
  const el = document.getElementById("status");
  if (!el) return;
  el.textContent = text;
  el.classList.toggle("err", Boolean(isError));
}

async function refresh() {
  try {
    const snap = await getJson("/api/snapshot");
    set("badge", snap.delayed_badge ?? "DELAYED");
    set("ov-snapshot", snap.snapshot_id ?? "—");
    set("ov-under-price", fmt(snap.under_price, 2));
    set("ov-n-contracts", String(snap.n_contracts ?? "—"));
    set("ov-feed", snap.delayed ? "DELAYED" : "LIVE");
    set("ov-killed", snap.killed ? "KILLED" : "armed");
  } catch (e) {
    setStatus(`snapshot: ${e.message}`, true);
  }

  try {
    const top = await getJson("/api/top20");
    const body = document.querySelector("#chain-body");
    if (!body) return;
    body.textContent = "";
    for (const r of top.rows ?? []) {
      const c = r.contract;
      const g = r.greeks;
      const tr = document.createElement("tr");
      for (const cell of [
        c.occ,
        c.expiry,
        String(c.dte),
        fmt(c.strike, 1),
        fmt(g?.delta, 3),
        fmt(g?.iv, 3),
        fmt((c.bid + c.ask) / 2, 2),
        fmt(r.utility, 4),
        String(c.oi),
      ]) {
        const td = document.createElement("td");
        td.textContent = cell;
        tr.appendChild(td);
      }
      body.appendChild(tr);
    }
    if (!(top.rows ?? []).length) {
      body.innerHTML = '<tr><td colspan="9">empty funnel</td></tr>';
    }
  } catch (e) {
    const body = document.querySelector("#chain-body");
    if (body) body.innerHTML = `<tr><td colspan="9" class="err">${e.message}</td></tr>`;
  }

  try {
    const blot = await getJson("/api/blotter");
    const body = document.querySelector("#blotter-body");
    if (!body) return;
    body.innerHTML =
      `<tr><td>rows</td><td>${blot.rows}</td><td>${blot.killed ? "yes" : "no"}</td></tr>`;
    set("ov-killed", blot.killed ? "KILLED" : "armed");
  } catch (e) {
    const body = document.querySelector("#blotter-body");
    if (body) body.innerHTML = `<tr><td colspan="3" class="err">${e.message}</td></tr>`;
  }
}

document.getElementById("kill")?.addEventListener("click", async () => {
  document.getElementById("kill").disabled = true;
  try {
    await fetch("/api/kill", { method: "POST" });
    await refresh();
    setStatus("kill switch engaged — no new tickets");
  } finally {
    const btn = document.getElementById("kill");
    if (btn) btn.disabled = false;
  }
});

void refresh();
