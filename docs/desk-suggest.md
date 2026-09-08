# Desk: equity suggest (page 7) — iterate this, don’t mix into the overlay kernel

**Status:** design only. Not in bits 1–8. Not auto-submit.

This is a **second desk** on the same terminal: “should I lean long/short **this name**?”  
The SPY-put overlay stays the **insurance robot**. Do not put AAPL buy/sell on the overlay tick path.

If this file fights [`plan.md`](plan.md) / [`hld.md`](hld.md) / [`lld.md`](lld.md) / [`AGENTS.md`](../AGENTS.md): **those win**. LLM still never computes size; gate still owns send.

---

## What companies actually do (not YouTube “AI BUY”)

| Who | What they ship | What they do **not** do |
|-----|----------------|-------------------------|
| **TD Securities** | ChatGPT on **their own research corpus** (RAG). Citations. “Five trade *ideas*.” | Not a live order from the model. They said they spend time stopping hallucination. |
| **Thinkorswim / TradingView** | 4+ panes: chart, watchlist, news, book. “Signals” = **rules** (EMA, RSI, ORB) with filters (ADX). | The honest scripts say: visualization, **not** a strategy, **not** a performance claim. |
| **Bloomberg Launchpad** | Operator tiles a workstation. Analytics are **priced/measured**. | No LLM on the matching engine. |
| **BlackRock SpiderRock / Aptus / Parametric** | **Managed overlay** (they trade the hedge). | Not “AI said sell Tesla.” |
| **Academic LOB models** | DeepLOB, TLOB: milliseconds–seconds **mid move** from the order book. | Needs **L2/L3**. Alpaca paper is **not** that feed. Edge dies after costs. |

Papers to steal **math** from, not to paste as a swarm:

| Paper | Use |
|-------|-----|
| Zhang et al. **DeepLOB** (2019) | CNN+LSTM on limit-order book → up/flat/down |
| **TLOB** (arXiv:2502.15757) | Transformer on LOB; also: predictability has **fallen** over time |
| Kolm et al. order-book DL (arXiv:2211.13777) | Short-horizon predictability is real **and** not the same as a tradable signal |
| PLR + CNN-LSTM (IEEE Access 2023) | Label **peaks/valleys** → 3-class Buy/Sell/Hold |
| **TradingAgents** (arXiv:2412.20138) | Org-chart of analysts — **do not** copy onto the order wire |
| **TradeTrap** (arXiv:2512.02261) | LLM trading agents can be **systematically fooled**; phantom positions |
| Async critique / hard gate (Itoflow) | Advice is ignored; a **HOLD until checks pass** is what changes behavior |
| HSTR (MDPI 2026) | Heavy LLM **off** the latency path; reconstruct state, then decide |

**Lesson:** numbers first, English second, **submit never** from the model. Same as overlay.

---

## Two pages

| Key | Page | Job |
|-----|------|-----|
| **7 Names** | 4-tile desk + scrollable universe (200–500) | Pick a name, see chart / features / bounded lean |
| **8 Map** | How the math works + 3D/heatmap | IV surface or feature space for the **same** focused name |

Same chrome. Hash `#names` / `#map`. Keys `7` / `8`.

## Page 7 — Names (4 tiles + list)

Left rail or top strip: searchable table of **hundreds** of Alpaca names (symbol, last, %chg, vol, Rust score). Click a row → four panes fill.

Same chrome (SPY, PAPER/LIVE, AGE, KILL). Body is a **2×2** like Overview.

```
┌─────────────────────┬─────────────────────┐
│  A  Chart + levels  │  B  Book / 3D map   │
│  candles, S/R,      │  heatmap of bid/ask │
│  labeled patterns   │  or IV slice        │
├─────────────────────┼─────────────────────┤
│  C  Features (Rust) │  D  Bounded suggest │
│  RSI, ret, imb,     │  HOLD / lean long / │
│  vol, regime        │  lean short + why   │
└─────────────────────┴─────────────────────┘
```

- **A** — TradingView-class chart (we can embed lightweight-charts). Patterns = **rules** (HH/HL, break of range), drawn by Rust/JS, not “AI saw a head-and-shoulders.”
- **B** — Honest 3D/heatmap: **quoted IV vs strike/expiry** for that name’s options, or a **2D book heatmap** if we ever have L2. Fake 3D on last price is decoration; skip it.
- **C** — Frozen feature vector from papers (returns, range, volume z, optional imbalance). **Rust.** Timestamped. No future bars.
- **D** — Claude (slow) sees **only C for the focused name**. Emits `{name, side: LONG|SHORT|HOLD, horizon_bars, conf, why}`. Rust: deny_unknown, enums, **name ∈ live Alpaca tradable set**, conf finite, **no qty**. Display only. Overlay gate does not see this.

### Universe: hundreds from Alpaca, not 6 hardcoded

Alpaca already is the master list: `GET /v2/assets?status=active&asset_class=us_equity`. Each row has `symbol`, `name`, `exchange`, `tradable`, `shortable`, `easy_to_borrow`, `fractionable`, `attributes` (e.g. `has_options`). Paper and live use the same shape.

v1 filter (so the list is 100s of **real** names, not 8k OTC shells):

- `status=active` and `tradable=true`
- exchange ∈ `{NYSE, NASDAQ, ARCA, AMEX}` — drop OTC
- optional: `has_options` if we want IV pane B
- rank by **dollar volume** from batched `GET /v2/stocks/snapshots` (≤100 symbols per call) — keep **top 200–500**
- search box over that list (type “TESL” → TSLA)

That is “100s of stocks from Alpaca.” We do **not** call Claude 500 times a second. Rust scores the list (vol, range, RSI). **One** focused symbol gets pane A/C/D (and page 8).

Claude bound: `name` must equal the focused symbol **and** sit in the Alpaca tradable set. A hallucinated ticker is `NOT_IN_UNIVERSE`.

---

## Page 8 — Map (math + 3D)

Not a second buy/sell button. **Show the work** for the name selected on page 7.

```
┌──────────────────────────┬──────────────────────────┐
│  3D / heatmap            │  Feature readout         │
│  IV vs strike vs expiry  │  each number in C,       │
│  or vol surface slice    │  with formula in English │
├──────────────────────────┴──────────────────────────┤
│  Pipeline: bars → features → (optional frozen model) → Claude suggest
│  “stale / missing → HOLD” drawn as a real gate, not a slogan
└─────────────────────────────────────────────────────┘
```

- **3D:** Plotly (or similar) **quoted IV** grid for that name’s options if `has_options`; else 2D heatmap of last 20×20 returns / vol. No fake spinning cube on one last price.
- **Math panel:** each feature: name, value, units, one-line “what it means” (e.g. RSI 72 = “pushed up vs last 14 bars”).
- Still **no** `Broker::submit` from this page.

---

## Maths we actually implement (v1)

Keep it boring. Class-8 meaning: “is this stretched or calm?”

1. **Returns** — 1, 5, 20 bar log-returns.  
2. **Vol** — realized σ, compare to its median (regime).  
3. **Range position** — close vs 20-bar high/low (0–1).  
4. **Simple technicals** — RSI-14, EMA fast/slow cross (same idea as TOS “console” scripts).  
5. **Label for later** — PLR turning points → Buy/Sell/Hold for **offline** fit. Do **not** live-train.  
6. **Optional later** — DeepLOB-class model **only** if we buy a real book feed. Until then it is a **lie** on this broker.

Claude’s job: turn **C** into a sentence + HOLD/LONG/SHORT. If C is missing/stale → pane D shows **HOLD / STALE**, no model call.

---

## Bounds (copy overlay, don’t invent a third religion)

```
V0 grammar (enum side) → V1 tool_use extract → V2 serde deny_unknown
→ V3 finite conf, horizon in {5,20,60} bars
→ V4 name ∈ watchlist
→ V5 Rust ignores any qty/limit the model sends
→ V6 audit  (suggest only — Broker::submit not on this path)
```

- Side v1 for **this desk**: LONG / SHORT / HOLD. **No market order from D.**  
- Operator may copy a name into a **manual** ticket later; overlay HEDGE stays SPY puts.  
- `llm_names=false` default until golden vectors exist (`crates/policy/tests/vectors/suggest_*`).  
- Kill switch blanks D the same as it blanks new overlay tickets.
- V4: `name` ∈ `{focused} ∩ Alpaca tradable ∩ listed universe`.

---

## Iterate in 5 slices (stop after each; look at the screen)

| Slice | Ship | Proof |
|-------|------|--------|
| **S0** | Page 7 shell + page 8 stub; keys 7/8 | Routes exist; overlay pages unchanged |
| **S1** | `GET /api/universe` from Alpaca assets + snapshots; table of **≥200** tradable NYSE/NASDAQ/ARCA names | Search AAPL/TSLA; OTC junk absent |
| **S2** | Click row → pane A chart (Alpaca bars) | Chart is that symbol |
| **S3** | Pane C: `/api/names/{sym}/features` | Stale → HOLD; RSI tests |
| **S4** | Pane D: Claude on **focused** name only; goldens | Hallucinated ticker rejected; qty ignored |
| **S5** | Page 8: IV heatmap + feature formulas for focused name | Surface **not** in Claude context |
| **S6** | (later) Offline PLR vs Claude; never auto-trade | Costs in backtest; HOLD baseline |

Do **not** start S5 until S3 goldens are green. Do **not** wire D to `Broker::submit`.

---

## What we will tell users (so we don’t lie)

- “Lean long/short” ≠ “the system bought Tesla.”  
- Pattern boxes are **geometry on the chart**, not prophecy.  
- 3D is **IV or book**, not a video-game stock.  
- Paper still paper. Indicative still not OPRA.

---

## Open choices (when we start S0)

1. Universe size: **200** vs **500** (same API; 500 = more snapshot batches). Recommend **200** first, cap 500.  
2. Pane B on page 7: mini IV slice vs “open page 8.” Recommend page 7 pane B = last/volume spark; **3D lives on page 8**.  
3. Shorts: only if `easy_to_borrow`; else SHORT → HOLD + reason.
