# Low-level design (LLD)

**Parent:** `docs/hld.md`  
**Build order:** `docs/execution.md`  
**Style:** IEEE 1016 logical + information views; Meyer contracts; OMS state machine.

This document specifies **types, invariants, transitions, reject codes, and call order**. Implementation follows these names unless a bit’s tests force a synonym (then update this file).

---

## 1. Threading and control

| Thread / task | Allowed to mutate | May block on HTTP |
|---------------|-------------------|-------------------|
| **Kernel** (1) | ring, top20, blotter, risk_state, breaker | **no** (except EMS *after* `decide_ms` stopwatch) |
| **Ingest worker** (1) | only produces a candidate `ChainSnapshot`; kernel `push`es | yes (vendor) |
| **Policy worker** (0–1) | last-good file via kernel mailbox | yes (Claude) |
| **HTTP** | kill flag, read snapshots | no kernel lock across vendor I/O |

Ingest/policy post **messages** into a bounded queue (capacity 4). Kernel is the only consumer that writes shared state. This is LMAX “business logic single-threaded,” not a full Disruptor crate in v1 — a `std::sync::mpsc` or `crossbeam` array-queue is enough.

**Ready:** ring non-empty **or** explicit STALE; last-good policy loaded or default file policy.

**Live:** process up. Kill does **not** fail liveness.

---

## 2. Data dictionary (kernel types)

All money that can hit a broker: **integer cents** (`i64`). Qty: `u32`. Deltas/λ: `f64` but must be finite; NaN/Inf is `Reject`.

| Type | Fields (logical) | Invariants |
|------|------------------|------------|
| `SnapshotId` | string 8–64 | unique per successful `push` |
| `PolicyId` | string 8–64 | unique per successful Strategist accept **or** `default` for file policy |
| `OccSymbol` | string | parsed OCC; v1 regex `[A-Z0-9]{1,6}\d{6}[CP]\d{8}` |
| `Stamps` | snapshot_id, policy_id, asof_unix_ms, exchange_ts_ms?, delayed, source | `data_age_ms = now - exchange_ts` else `now - asof` |
| `PriceLevel` | price f64, size f64 | existing domain |
| `OptionContract` | occ, underlying, expiry, right, strike, dte, bid, ask, last?, oi, volume | bid≤ask, bid>0, ask>0 |
| `Greeks` | delta, gamma, theta, vega, iv | finite; put delta ∈ [-1,0] after compute |
| `Enriched` | contract + greeks + utility f64 | same snapshot_id |
| `Top20` | snapshot_id, policy_id, `[Enriched; ≤20]` | ids match live |
| `Policy` | regime enum, dte_min/max, delta_min/max, max_premium_cents, λ×3, reason | dte_min≤dte_max; put deltas in [-1,0]; λ≥0 finite |
| `TicketProposal` | snapshot_id, policy_id, occ, side=BUY, qty, limit_cents, tif, why | V4–V5 |
| `NewOrder` | client_order_id, occ, side, qty, limit_cents, tif, snapshot_id, policy_id | only from Gate |
| `Reject` | code, field, got, message | `message` log-only |
| `RiskState` | equity_cents, daily_pnl_cents, rejection_count, breaker | equity>0 to trade |
| `BlotterRow` | client_order_id, state, occ, qty, filled_qty, … | state machine §4 |

`client_order_id = hex(blake3(snapshot_id || policy_id || occ || qty_le || side))` (32-byte hex).

---

## 3. Module contracts

### 3.1 `neural-router-data`

```text
trait ChainSource {
    fn fetch(&mut self) -> Result<RawChain, DataError>;
}

fn validate_chain(raw: RawChain, now_ms: i64) -> Result<ChainSnapshot, DataError>
fn SnapshotRing::push(s: ChainSnapshot) -> Result<SnapshotId, DataError>
fn SnapshotRing::current() -> Option<&ChainSnapshot>
```

**Pre:** `RawChain` from fixture or vendor.  
**Post:** snapshot has stamps; invalid rows dropped or whole fetch rejected (pick: **whole fetch reject** if < N valid puts, N=20).  
**Error:** `InvalidSnapshot`, `InsufficientDepth`, `Stale`.

L1 cache key: `(underlying, expiry_set_hash)`. TTL `min(vendor_delay, config.ingest_ttl_ms)`.

### 3.2 `neural-router-ml`

```text
fn greeks_put(S, K, T, r, q, sigma) -> Result<Greeks, MlError>
fn implied_vol_put(...) -> Result<f64, MlError>
fn funnel(chain: &ChainSnapshot, policy: &Policy) -> Top20
fn dollar_delta(holdings, under_price) -> f64
fn band_status(delta, lo, hi) -> Hold | Breach
```

**Pre:** chain validated; policy clamped.  
**Post:** every `Top20` row has same `snapshot_id`/`policy_id`. Layer 6 SVI skipped if flag off.  
**T excluded from `decide_ms`:** none (pure CPU).  
**European BS:** skip `dte < 7`.

Default file policy when LLM off: DTE 30–60, Δ [-0.50, -0.20], λ = 1,1,1.

### 3.3 `neural-router-execution` — Gate

```text
fn gate(
    proposal: TicketProposal,
    top20: &Top20,
    policy: &Policy,
    risk: &RiskState,
    now_ms: i64,
    rth: bool,
) -> Result<NewOrder, Reject>
```

**Pre:** proposal already V2-parsed if from LLM; argmax path builds `TicketProposal` internally.  
**Post:** `NewOrder` ready for EMS **or** `Reject`.  
**Does not** call HTTP.

### 3.4 `Broker` (EMS)

```text
trait Broker {
    fn submit(&self, order: &NewOrder) -> Result<SubmitAck, ExecutionError>;
    fn position(&self, symbol: &str) -> Result<i64, ExecutionError>; // shares or contracts
}
```

**Pre:** `NewOrder` from `gate` only.  
**Post:** ack contains broker id or duplicate-of `client_order_id`.

### 3.5 `neural-router-policy`

```text
fn validate_policy(raw: &str, risk: &RiskState) -> Result<Policy, Reject>
fn validate_ticket(raw: &str, live: LiveRefs) -> Result<TicketProposal, Reject>
struct LiveRefs<'a> { snapshot: &'a ChainSnapshot, top20: &'a Top20, policy: &'a Policy, risk: &'a RiskState }
```

Claude HTTP is behind `trait Llm { fn complete(&self, req: LlmReq) -> Result<RawToolJson, PolicyError> }` so tests never network.

---

## 4. Order state machine

```
        submit() ok
NEW ──────────────► SUBMITTED ──fill──► PARTIAL ──remain=0──► FILLED
  │                     │                  │
  │                     │ cancel/expire    │ cancel
  │                     ▼                  ▼
  └──gate fail──► REJECTED            CANCELLED
```

| From | Event | To |
|------|-------|-----|
| — | `gate` Ok | NEW then immediately SUBMITTED after EMS returns (v1 may skip NEW in memory if submit is sync) |
| SUBMITTED | full fill | FILLED |
| SUBMITTED | partial | PARTIAL |
| SUBMITTED | cancel/IOC leftover | CANCELLED |
| SUBMITTED | broker reject | REJECTED |
| PARTIAL | fill remainder | FILLED |
| any terminal | anything | **ignore** (idempotent) |

v1 paper: treat IOC leftover as CANCELLED, not a sitting GTC.

---

## 5. Kernel loop (pseudocode)

```text
loop {
    drain mailbox (ingest snapshots, policy updates, kill)
    let snap = ring.current()
    let policy = last_good_or_file()
    if snap is None or age(snap) > max_age { set STALE; sleep; continue }
    if !rth { continue }

    start decide_clock
    let g = greeks_all(snap)
    let top = funnel(snap, policy)
    let d = dollar_delta(...)
    let status = band_status(d, policy.band)
    stop decide_clock  // record nr_decide_ms

    if status == Hold { publish snapshot to HTTP; continue }

    let proposal = if flag llm_quant {
        take mailbox ticket if ids match else skip this cycle
    } else {
        argmax_utility(top, d, policy)
    }
    match gate(proposal, ...) {
        Ok(order) => { audit(accept); broker.submit(order); blotter.insert }
        Err(r) => { audit(r); bump rejection_count; maybe breaker }
    }
}
```

Quant is **asynchronous**: Policy worker may have left a ticket in the mailbox. If ids mismatch, drop (STALE_SNAP), do not send.

---

## 6. Reject catalog (stable codes)

| Code | Stage | Meaning |
|------|-------|---------|
| `PARSE` | V1–V2 | not JSON / extra field / type |
| `RANGE_DTE` | V3 | dte_min > dte_max or out of 1..=365 |
| `RANGE_DELTA` | V3 | not puts band |
| `LAMBDA` | V3 | non-finite or < 0 |
| `PREMIUM_CAP` | V3/V5 | over 1% equity |
| `STALE_SNAP` | V4 | id ≠ live |
| `STALE_POLICY` | V4 | id ≠ live |
| `NOT_IN_TOP20` | V4 | OCC |
| `QTY_RECOMPUTE` | V5 | model qty > kernel qty (default reject) |
| `LIMIT_AWAY` | V5 | limit vs mid+slippage |
| `RTH` | Gate | outside session |
| `DAILY_LOSS` | Gate | 5% |
| `BREAKER` | Gate | rejection_count |
| `STALE_DATA` | MDG | age |
| `STALE_POS` | OMS | recon |
| `BRAIN_DOWN` | Policy | transport |
| `MISSING_CREDS` | EMS | fail closed |
| `NOT_PAPER` | EMS | live without ALLOW_LIVE |

HTTP/UI maps these to the blotter `reason` column. Do not invent new codes in bits without updating this table.

---

## 7. Validation sequence (bit 7)

```
Policy worker                 Kernel
    |                            |
    | complete(Claude)           |
    | V0–V3                      |
    | mailbox Policy or Ticket   |
    |                            | drain
    |                            | V4–V5 vs live refs
    |                            | V6 audit
    |                            | gate / last-good
```

**Retry:** only Policy worker, once, with `Reject` JSON in the next user message. Kernel does not retry Claude.

**pass^k:** `validate_ticket(same_bytes, same LiveRefs)` is a pure function. Test k=3.

---

## 8. HTTP surface (bit 8)

| Method | Path | Cache | Notes |
|--------|------|-------|-------|
| GET | `/api/snapshot` | ETag = snapshot_id, max-age=0 | 304 if match |
| GET | `/api/top20` | ETag | |
| GET | `/api/blotter` | no-store | |
| GET | `/metrics` | no-store | Prometheus |
| POST | `/api/kill` | no-store | inhibit; 204 |

Bind `127.0.0.1:8080` default. Optional `UI_TOKEN` header `X-NR-Token`.

---

## 9. Config keys (additive to existing `Settings`)

| Key | Default | Meaning |
|-----|---------|---------|
| `MAX_DATA_AGE_MS` | `900000` (15 min) | STALE_DATA |
| `INGEST_TTL_MS` | `900000` | L1 |
| `MAX_SLIPPAGE` | `0.03` | LIMIT_AWAY |
| `RTH_ONLY` | `true` | |
| `LLM_STRATEGIST` | `false` | |
| `LLM_QUANT` | `false` | |
| `PANIC_HEDGE` | `false` | |
| `ALLOW_LIVE` | unset | must be `1` with paper=false |
| `UI_TOKEN` | empty | |
| `AUDIT_PATH` | `logs/audit.jsonl` | |
| `LAST_GOOD_POLICY_PATH` | `logs/last_good_policy.json` | |
| `BIND` | `127.0.0.1:8080` | |

---

## 10. Test matrix (must stay hermetic)

| Test | Bit | Assert |
|------|-----|--------|
| extra JSON field | 1,7 | `PARSE` |
| fixture → ≥1 put in top20 | 3 | |
| Δ inside band → no submit | 3–4 | |
| qty vs 1% | 4 | `PREMIUM_CAP` or `QTY_RECOMPUTE` |
| OCC not in top20 | 4,7 | `NOT_IN_TOP20` |
| duplicate client_order_id | 5 | one broker POST |
| audit has reject code | 6 | |
| last-good unchanged on bad policy | 7 | |
| `decide_ms` histogram recorded | 6 | |

No test in CI opens a real socket to Alpaca, Anthropic, or Yahoo.

---

## 11. Crate dependency DAG

```
domain
  ↑
config
  ↑
data ──────────────► ml
  ↑                   ↑
execution ◄───────────┘
  ↑
policy  (depends on domain, config; takes LiveRefs from ml/data at call site)
  ↑
neural-router (binary wires all)
```

`policy` must **not** depend on `execution`’s Alpaca module (only `Reject` / `TicketProposal` types — put those in **domain** to avoid a cycle).

---

## 12. Open items (explicit, not TBD)

| Item | Resolution |
|------|------------|
| rust_decimal vs i64 cents | **i64 cents** in v1 |
| blake3 vs sha256 | **blake3** if crate cost ok; else sha256 |
| axum vs hyper | **axum** in bit 8 |
| full Disruptor crate | **no** until profiling says mpsc is the bottleneck |
| American exercise | **exclude DTE<7**; document residual risk |

---

## 13. Mapping to IEEE viewpoints

| Viewpoint | Where |
|-----------|--------|
| Context | HLD §2 |
| Composition | HLD §3, LLD §11 |
| Logical | HLD §4, LLD §5–7 |
| Information | HLD §5, LLD §2 |
| Interface | HLD §6, LLD §3, §8 |
| Dependency | LLD §11 |
| Interaction | LLD §5, §7 |
