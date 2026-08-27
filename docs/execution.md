# Execution plan — 8 bits

> Spec: `docs/plan.md`. Design: `docs/hld.md`, `docs/lld.md`. This file is the **build order**. Each bit is independently testable. Do not start bit N+1 until bit N’s exit gate is green.

**Goal:** Paper-first SPY options overlay kernel in this Cargo workspace, then a thin HTTP + 3-page terminal, then Claude policy behind a **fail-closed validator**.

**Architecture:** Two clocks. Rust owns ingest → Greeks → funnel → Δ band → risk → Alpaca paper. Claude may only emit **clamped JSON**. Schema-valid ≠ trade-valid ([arXiv:2607.18261](https://arxiv.org/abs/2607.18261)).

**Stack:** Rust 2024 workspace already in `crates/*`. Tests: `cargo test --workspace`. Benches: `criterion` on fixture chains. Claude: Messages API + **strict tool use**. No Jenkins, no GKE, no live money.

**Global constraints**

- LLM never on the tick path; never computes Greeks or size.
- `ALPACA_PAPER` defaults true; live requires `ALPACA_PAPER=false` **and** `ALLOW_LIVE=1`.
- Secrets from env / Secret Manager; never logs.
- `decide_ms` timer excludes HTTP.
- Kernel **single writer** (one process).
- Feature flags: `llm_strategist`, `llm_quant` default **off** until bit 7.
- Every Claude object is bound to `snapshot_id` + `policy_id`.

---

## LLM validation (applies to bit 7; tests start in bit 1 as types)

Provider constrained decoding is **layer 0 only**. Official Claude docs: `strict: true` / JSON schema outputs guarantee **shape**. They do **not** guarantee the OCC symbol exists, the qty is safe, or the snapshot is current.

**Workflow `llm-validation-survey` (confirmed):** Anthropic grammars / SDKs **strip `minimum` / `maximum`**. Re-apply numeric ranges in Rust. Do not auto-repair `occ_symbol`, `side`, `qty`, or `limit`. Optional compile-time `jsonschema` 0.50.x with `default-features = false` (no HTTP `$ref`). Measure audit **pass^k** (same inputs → same reject/accept), not pass@k. Claude is a **proposer**, not a live tool agent to the broker.

Papers:

| Paper | Use |
|-------|-----|
| [When JSON Is Not Enough](https://arxiv.org/abs/2607.18261) (OrderBench) | 100% schema-valid can still be ~20% semantically wrong; **domain verification + fail-closed** |
| [Replayable Financial Agents / DFAH](https://arxiv.org/abs/2601.15322) | schema-first + `T=0` + audit replay |
| [The Constraint Tax](https://arxiv.org/abs/2605.26128) | constrain **late**; reason free, package into schema last (we still use tools; kernel is the late constraint) |
| Claude [strict tool use](https://platform.claude.com/docs/en/agents-and-tools/tool-use/strict-tool-use) | `strict: true`, `additionalProperties: false`, all fields in `required` for the subset |

### Pipeline (V0–V6)

```
Claude T=0, strict tool
        │
        ▼
V0  provider grammar          (syntax)
V1  extract tool_use.input    (one named tool)
V2  serde deny_unknown_fields (types)
V3  re-apply min/max + finite  (provider does **not** enforce numeric ranges)
V4  bind snapshot_id, policy_id, occ ∈ top20
V5  kernel recomputes qty & limit; model values are proposals
V6  append audit; only then may Exec run
```

Fail at any V → structured `Reject { code, field, got }`. **One** retry: same prefix + `Reject` in the user turn. Second fail → **no ticket** (Quant) or **do not overwrite last-good** (Strategist). Never `parse::<f64>()` on `"two"`. Never coerce percent vs fraction.

### Tools Claude is allowed (and nothing else)

**`emit_policy`** (Strategist, minutes clock):

```json
{
  "name": "emit_policy",
  "strict": true,
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": [
      "policy_id", "regime", "dte_min", "dte_max",
      "delta_min", "delta_max", "max_premium_cents",
      "lambda_svi", "lambda_pca", "lambda_eff", "reason"
    ],
    "properties": {
      "policy_id": { "type": "string", "minLength": 8, "maxLength": 64 },
      "regime": { "type": "string", "enum": ["calm", "vol_expanding", "stress", "unknown"] },
      "dte_min": { "type": "integer", "minimum": 1, "maximum": 365 },
      "dte_max": { "type": "integer", "minimum": 1, "maximum": 365 },
      "delta_min": { "type": "number" },
      "delta_max": { "type": "number" },
      "max_premium_cents": { "type": "integer", "minimum": 0, "maximum": 100000000 },
      "lambda_svi": { "type": "number" },
      "lambda_pca": { "type": "number" },
      "lambda_eff": { "type": "number" },
      "reason": { "type": "string", "maxLength": 240 }
    }
  }
}
```

**`emit_ticket`** (Quant, breach only):

```json
{
  "name": "emit_ticket",
  "strict": true,
  "input_schema": {
    "type": "object",
    "additionalProperties": false,
    "required": [
      "snapshot_id", "policy_id", "occ_symbol",
      "side", "qty", "limit_cents", "tif", "why"
    ],
    "properties": {
      "snapshot_id": { "type": "string", "minLength": 8, "maxLength": 64 },
      "policy_id": { "type": "string", "minLength": 8, "maxLength": 64 },
      "occ_symbol": { "type": "string", "minLength": 5, "maxLength": 32 },
      "side": { "type": "string", "enum": ["BUY"] },
      "qty": { "type": "integer", "minimum": 1, "maximum": 1000 },
      "limit_cents": { "type": "integer", "minimum": 1, "maximum": 100000000 },
      "tif": { "type": "string", "enum": ["IOC", "FOK"] },
      "why": { "type": "string", "maxLength": 240 }
    }
  }
}
```

v1 **BUY-only** index puts. No SELL. Money in **integer cents**. Qty integer. `reason`/`why` log-only.

### Semantic checks (V3–V5) — always in Rust

| Code | Rule |
|------|------|
| `RANGE_DTE` | `dte_min <= dte_max` |
| `RANGE_DELTA` | both in `[-1, 0]` for puts; `delta_min <= delta_max` |
| `LAMBDA` | each λ finite and `>= 0`; no NaN/Inf |
| `PREMIUM_CAP` | `max_premium_cents <= equity_cents * risk_limit_per_trade` |
| `STALE_SNAP` | `ticket.snapshot_id == live.snapshot_id` |
| `STALE_POLICY` | `ticket.policy_id == live.policy_id` |
| `NOT_IN_TOP20` | `occ_symbol` exact match in current top-20 |
| `QTY_RECOMPUTE` | kernel size wins; if model qty > kernel qty → reject (do not silently shrink on retry 0; shrink only if you choose explicit policy — **default reject**) |
| `LIMIT_AWAY` | `limit_cents` within `mid_cents * (1 + max_slippage)` (default 3%) |
| `RTH` | no new risk outside regular hours (config) |

Prompt cache: static system + tool schemas as Anthropic cacheable prefix; dynamic suffix is `{dollar_delta, vix, band, top20[], policy}`. Prefix version `PROMPT_STRATEGIST_v1` / `PROMPT_QUANT_v1` in audit.

---

## File map (new vs existing)

| Path | Bits |
|------|------|
| `crates/domain/src/{ids,option,greeks,ticket,policy,stamps}.rs` | 1 |
| `crates/config/src/lib.rs` | 1, 7 |
| `crates/market-data/src/{snapshot,ingest,cache,fixture}.rs` | 2 |
| `crates/ml-core/src/{iv,funnel,utility,band}.rs` | 3 |
| `crates/execution/src/{risk.rs,ticket.rs,idempotency.rs,alpaca.rs}` | 4, 5 |
| `crates/observe/` (new crate) **or** `crates/neural-router/src/observe.rs` | 6 |
| `crates/policy/` (new crate) `src/{claude,validate,last_good}.rs` | 7 |
| `crates/neural-router/src/{main.rs,http.rs}` | 8 |
| `testdata/spy_chain.json` | 2–4, 7 |
| `frontend/` pages 1, 2, 4 | 8 |

Prefer a `policy` crate over stuffing HTTP into `ml-core`.

---

## Bit 1 — Domain, stamps, config

**Exit:** types compile; tests for ids, deny-unknown policy JSON, redacted Debug.

**Produces:**

```rust
pub struct SnapshotId(pub String); // ulid or uuid string
pub struct PolicyId(pub String);
pub struct OccSymbol(pub String);

pub struct Stamps {
    pub snapshot_id: SnapshotId,
    pub policy_id: PolicyId,
    pub asof_unix_ms: i64,
    pub exchange_ts_ms: Option<i64>,
    pub delayed: bool,
    pub source: &'static str,
}

pub struct OptionContract { /* occ, strike, right, dte, bid, ask, oi, volume */ }
pub struct Greeks { pub delta: f64, pub gamma: f64, pub theta: f64, pub vega: f64, pub iv: f64 }
pub struct Policy { /* fields matching emit_policy + stamps */ }
```

- [ ] Add modules under `crates/domain`; `#[serde(deny_unknown_fields)]` on `Policy` / ticket DTOs.
- [ ] Config: `max_data_age_ms`, `rth_only`, `max_slippage`, `allow_live`, feature flags default false.
- [ ] Tests: extra JSON field fails; NaN λ fails a `finite()` helper; Debug redacts keys (already exists).
- [ ] `cargo test -p neural-router-domain --lib`

---

## Bit 2 — Snapshot ring, ingest, fixtures, L0/L1 cache

**Exit:** load `testdata/spy_chain.json` into ring; `data_age` stamped; 429 backoff unit-tested without network.

**Produces:** `SnapshotRing::push(validated) -> SnapshotId`, `current() -> Option<&ChainSnapshot>`, ingest trait `ChainSource`.

- [x] Fixture: ≥200 SPY puts/calls, bids/asks, OI. Mark `delayed: true`.
- [x] Validator: uncrossed, positive prices, OCC parse.
- [x] L0 ring size 2–4; refuse mix of desks in v1 (SPY only).
- [x] L1: TTL ≤ 15 min; negative cache on 429.
- [x] Ready: empty ring → `STALE_DATA`, no tickets.
- [x] Tests use fixture only. `cargo test -p neural-router-data`

---

## Bit 3 — IV/Greeks, funnel, dollar-Δ band

**Exit:** fixture chain → top-20 + `Hold` or `Breach`. `criterion` p99 `decide_ms` on fixture (no HTTP).

**Produces:** `funnel(chain, policy) -> Top20`, `dollar_delta(holdings, greeks) -> f64`, `band_status(delta, band) -> Hold | Breach`.

- [x] BS/IV in Rust (port or thin `quant-opts` / local). European BS documented; DTE < 7 excluded in funnel.
- [x] Funnel layers 1–5 + 7 (utility). Layer 6 SVI **skip** if no surface (`surface_svi` flag off).
- [x] Default policy file (not LLM): DTE 30–60, put Δ [−0.50, −0.20].
- [x] Feature `llm_quant=false` → `argmax(utility)` as the pick (still a proposal; bit 4 gates).
- [x] Bench: `crates/ml-core/benches/funnel.rs`. Record p50/p99 in the PR notes.
- [x] `cargo test -p neural-router-ml` && `cargo bench -p neural-router-ml --bench funnel`

Bench (`funnel_fixture`, criterion sample-size 40, no HTTP): mean **495 µs** (95% CI 479–512 µs). In-RAM kernel SLA is 50 ms.

---

## Bit 4 — Risk, ticket, idempotency, breaker

**Exit:** 1%/5%/over-hedge/IOC tests; `client_order_id` stable; 3 rejects trip breaker.

**Produces:** `gate(ticket_proposal, risk_state, top20, policy) -> Result<NewOrder, Reject>`.

```rust
client_order_id = blake3(snapshot_id, policy_id, occ, qty, side)
```

- [x] Reuse `RiskManager` in `crates/execution/src/risk.rs`; extend for options premium notional.
- [x] `rejection_count` window; ≥3 → breaker; panic path = **no LLM**, either skip or hardcoded ATM put **off** until explicitly flagged `panic_hedge`.
- [x] Default `panic_hedge=false` (flat / no new orders).
- [x] Tests: qty that blows 1%; daily pnl −5%; symbol not in top20; id mismatch.
- [x] `cargo test -p neural-router-execution`

---

## Bit 5 — Alpaca paper

**Exit:** dry-run against **mock** HTTP; optional live-paper behind env. Fail closed without keys.

**Produces:** `Broker::submit` with `client_order_id`; position GET recon.

- [x] Mock server or `wiremock`-style fixture for 200/403/409 duplicate.
- [x] Duplicate `client_order_id` → treat as already submitted, not a second order.
- [x] Recon: blotter vs positions; mismatch → `STALE_POS`.
- [x] Paper URL only unless `ALLOW_LIVE=1`.
- [x] Do **not** call real Alpaca in CI.
- [x] `cargo test -p neural-router-execution --features paper-mock`

---

## Bit 6 — Observe: metrics, logs, audit

**Exit:** `/metrics` has `nr_decide_ms`; one reject writes an audit row; logs have no secrets.

**Produces:** `nr_*` histograms; append-only `audit.jsonl` (or SQLite).

- [x] `tracing` JSON. Histogram buckets include 1, 5, 10, 25, 50, 100 ms for `decide_ms`.
- [x] Audit fields: time, role, tool, raw JSON, `Reject` or accept, snapshot_id, policy_id, prompt_version, model.
- [x] Tests: redaction; audit append is monotonic; `/metrics` contains `nr_decide_ms`.
- [x] No Cloud yet.

---

## Bit 7 — Claude policy + validator (the agentic bit)

**Exit:** golden fixtures for V0–V6 without network; mock Claude; optional one real call locally (never CI).

**Produces:** `crates/policy` — `validate_policy`, `validate_ticket`, `last_good`, HTTP client.

- [x] **Golden files** under `crates/policy/tests/vectors/`:
  - `policy_ok.json`
  - `policy_extra_field.json` → V2 fail
  - `policy_nan_lambda.json` → V3
  - `ticket_not_in_top20.json` → `NOT_IN_TOP20`
  - `ticket_stale_snapshot.json` → `STALE_SNAP`
  - `ticket_qty_too_big.json` → `QTY_RECOMPUTE`
  - `ticket_sell.json` → enum fail
- [x] Mock: inject tool_use JSON; assert last-good **unchanged** on Strategist fail.
- [x] Quant: one retry then hold.
- [x] Token budget counter; on exhaust, last-good only.
- [x] Prompt cache headers if using Anthropic cache breakpoints on the static prefix.
- [x] `cargo test -p neural-router-policy`
- [x] Manual (optional): `ANTHROPIC_API_KEY` + fixture top20 → must still pass V4–V5.

Do not enable `llm_quant` in the default CLI until these vectors are green.

---

## Bit 8 — HTTP + terminal (pages 1, 2, 4)

**Exit:** `GET /api/snapshot` ETag = snapshot_id; Overview + Chain + Blotter; **no-store** on blotter; no CDN on APIs.

**Produces:** axum (or similar) in `neural-router`; frontend reads JSON.

- [x] Routes: `/api/snapshot`, `/api/top20`, `/api/blotter`, `/api/metrics`, `POST /api/kill`.
- [x] Auth: loopback bind default `127.0.0.1`; optional `UI_TOKEN`.
- [x] Kill must work even when `STALE_DATA`.
- [x] Pages 3 (surface), 5 (agents), 6 (settings) **stub links** only unless time left.
- [x] Manual: DELAYED badge visible; kill stops submits.

---

## Order and parallelism

```
1 → 2 → 3 → 4 → 5
              ↘ 6  (observe can start after 4)
                 7 after 4 (validator needs Top20 + Risk)
                 8 after 5+6 (needs blotter + metrics)
```

Bits 6 and 7 can overlap after 4. Bit 8 last.

---

## Done definition (all 8)

| Bit | Gate |
|-----|------|
| 1 | `cargo test -p neural-router-domain` |
| 2 | fixture snapshot in ring; stale test |
| 3 | funnel test + bench note |
| 4 | gate tests + breaker |
| 5 | mock broker tests |
| 6 | metrics + audit tests |
| 7 | **all validation vectors green**; flags still default off |
| 8 | snapshot API + 3 pages locally |

Then: turn `llm_strategist` on with last-good; then `llm_quant` on paper only.

---

## Explicitly not in these 8 bits

GKE, Jenkins, Redis, SVI/PC2 live, Quant as 34-agent meeting, live Alpaca, DRL, page 3 surface mesh, CEO/CRO LLM seats (stubs only).

---

## Verify this doc vs `plan.md`

| Plan item | Bit |
|-----------|-----|
| Domain / stamps / cache L0 | 1–2 |
| Funnel / Δ band / 50 ms bench | 3 |
| Risk / IOC / 1%/5% | 4 |
| Alpaca paper / recon / idempotency | 5 |
| OTel / audit ≠ logs | 6 |
| 2 Claude + V0–V6 + prompt cache | 7 |
| 6 pages (3 shipped, 3 stub) | 8 |
| 34-seat org | not coded; seats are functions |
| GCP Cloud Run | after bit 8, deploy-only |
