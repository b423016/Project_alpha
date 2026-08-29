# Paper session snapshot + next work

Stopped local overlay at **2026-08-29** (listener on `127.0.0.1:8080` killed). No secrets in this file.

Git: kernel base is **`master`**. Website work is **`website`** (`origin/website`).

---

## Paper order (Alpaca paper, not live)

| Field | Value |
|-------|--------|
| Venue | `https://paper-api.alpaca.markets` (`ALPACA_PAPER=true`, `ALLOW_LIVE` off) |
| Account | ACTIVE, equity `100000` USD, id tail `***XDTL` |
| Side | BUY (v1 only) |
| OCC | `SPY260930P00744000` |
| Expiry / strike | 2026-09-30 / 744 |
| Qty | 1 |
| OMS TIF | IOC (EMS maps to Alpaca options `day`) |
| State (in-process blotter) | Submitted, filled_qty 0 |
| `client_order_id` | `b71b7418dd0e3f944c869efa0c8fa98c7b3caf0f547f2abe267e41961f48e98b` |

Idempotency: second HEDGE returned `ok: true`, `duplicate: true`. Alpaca `422` / `40010001` `client_order_id must be unique` is **duplicate**, not `BRAIN_DOWN`.

Same hash as LLD: `hex(blake3(snapshot_id ‖ policy_id ‖ occ ‖ qty_le ‖ side))`. Same pick → same id → no second fill.

---

## Market snapshot at submit

| Field | Value |
|-------|--------|
| `snapshot_id` | `snap-al1788000329119` |
| Source | `alpaca-indicative` |
| Badge | LIVE (indicative quotes, not OPRA) |
| Underlying | SPY `769.28` |
| Contracts in ring | 2053 |
| `asof_unix_ms` | `1788000329119` |
| Top-20 pick | `SPY260930P00744000` (DTE 31, K 744) |

Expired fixture `SPY260417P00500000` is **not** used on this path. 422 `asset not found` was that April OCC.

---

## Claude policy at submit

| Field | Value |
|-------|--------|
| `policy_id` | `spy_put_overlay_al1788000329119` |
| Regime | unknown |
| DTE | 30–60 |
| Put Δ | −0.50 … −0.20 |
| Premium cap | 100_000 cents ($1,000) |
| λ svi / pca / eff | 0.5 / 0.5 / 0.5 |
| Quant path on hedge | `argmax` (Claude ticket failed V4–V5; kernel pick used) |
| Flags | `LLM_STRATEGIST=true`, `LLM_QUANT=true`, `RTH_ONLY=false` (weekend paper) |

---

## What this is (product)

Defined-risk **SPY put overlay** on a long book. Two clocks: Rust kernel (Greeks, funnel, gate, EMS); Claude proposes `{regime, λ, bands}` and optionally a ticket. LLM never computes IV, Δ, or size. Paper fills are not a fill/impact model.

Chrome: SPY last, PAPER, LIVE/DELAYED, AGE, decide_ms, ALPACA, CLAUDE, HEDGE, KILL (`k`).

---

## Next: full terminal (spiral 3) on `website`

Same 6 pages as `docs/plan.md` §8. No chatbot. No 34-LLM swarm.

| # | Page | Build |
|---|------|--------|
| 1 | Overview | Book $Δ vs band from Alpaca positions; VIX; holdings ΔΓΘ; PnL sketch; auto-hedge **on breach** (HEDGE = override) |
| 2 | Chain | Filters from live Strategist policy; SPY only v1 |
| 3 | Surface | Own `/api/surface`; quoted-IV grid first; SVI/PC2 spiral 4 |
| 4 | Blotter | Poll Alpaca: PENDING → FILL/PARTIAL/CANCEL/REJECT; limit vs fill; gate-reject log |
| 5 | Agents | Last Strategist JSON vs last-good λ; last Quant pick; `rejection_count`; histogram |
| 6 | Risk/Set | Writable 1%/5%, DTE, kill, data source, `data_age` alert; live still needs both live flags |

Kernel loop to finish E2E:

```
poll chain → Greeks/funnel → $Δ band
  hold inside band
  on breach: Quant 1 shot + 1 retry → gate → paper submit → blotter poll
Strategist every 5–15 min
```

**Not this spiral:** SVI/PC2 live, news/Twitter (regime only if ever), CEO/CRO LLM seats, GKE/Redis, live money, QQQ desks, DRL.

Suggested order: (1) OMS poll fills, (2) positions → $Δ band, (3) breach-only Quant, (4) settings that persist, (5) agents last-good, (6) IV grid, (7) VIX + honest age, (8) CI on `master`.

---

## Restart (no keys in the command)

```text
git checkout website
cargo run -p neural-router -- serve
```

Open `http://127.0.0.1:8080/`. Keys stay in gitignored `.env`.
