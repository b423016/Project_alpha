# High-level design (HLD)

**Document type:** Software Design Description, IEEE 1016-style viewpoints  
**System:** Neural Router — paper-first options overlay kernel  
**Status:** design  
**Parents:** `docs/plan.md`, `docs/execution.md`  
**Child:** `docs/lld.md`

This is a **pre-AI-era SDD**: context, composition, information, interfaces, NFRs, failure. Claude is an **external proposer**, not a component on the critical path. Schema-valid JSON is not a fill.

---

## 1. Purpose and design principles

**Purpose.** Maintain a long equity book’s dollar delta inside a band by buying defined-risk index puts (SPY first) on Alpaca **paper**, with a 50 ms in-process decision budget **after** a snapshot is in RAM.

**Principles (old systems, not agent demos)**

| Principle | Source | How it shows up |
|-----------|--------|-----------------|
| Information hiding | Parnas 1972 | Broker, feed, Claude behind traits; rest of the kernel never sees HTTP |
| Single writer / unshared mutable state | LMAX (Fowler 2011, Disruptor) | One kernel thread mutates book, policy, blotter |
| Design by contract | Meyer | Pre: valid snapshot + policy. Post: ticket or typed `Reject`. Invariant: no order without V5 |
| Pre-trade risk on the wire path | Classic OMS | Risk **before** `Broker::submit`; post-trade recon after fills |
| Journal before mutate | LMAX input disruptor | Audit append **before** send |
| Fail closed | Safety-critical / exchange risk | Missing data, bad JSON, stale ids → no new risk |
| Separate OMS and EMS | Industry OMS/EMS split | OMS = ticket lifecycle + blotter. EMS = Alpaca adapter only |
| Two clocks | This spec | Fast: Rust. Slow: Claude + Scheduler |

**Not principles:** “more agents,” LLM on the tick, CDN for dollar Δ, one Deployment per firm-graph seat.

---

## 2. Context viewpoint (system boundary)

```
                    ┌─────────────────────────────────────────┐
  yfinance /        │              THIS SYSTEM                │
  Massive delayed ──►  MDG   Kernel (single writer)   OMS     │
  (15 min)          │   │         │                    │      │
                    │   ▼         ▼                    ▼      │
  Finnhub/calendar─►│ Ingest → Funnel/Greeks/Band → Gate → EMS│──► Alpaca paper
                    │         ▲              │                │
  Claude Messages ──► Policy ─┘              │                │
  (minutes)         │  V0–V6                 │                │
                    │                        ▼                │
  Operator browser◄─│ HTTP (127.0.0.1) + 6-page UI            │
                    │ audit.jsonl  /metrics                   │
                    └─────────────────────────────────────────┘
```

**External actors**

| Actor | Direction | Trust |
|-------|-----------|--------|
| Delayed chain vendor | in | Untrusted; validate; stamp `delayed` |
| Alpaca paper | in/out | Untrusted; mock in CI; source of truth for **fills** |
| Claude | in (JSON proposals) | Untrusted; V0–V6 |
| Operator | in (kill, flags) | Trusted on localhost; optional `UI_TOKEN` |
| Calendar file | in | Trusted if we ship it; missing → documented degrade |

Nothing outside the box may call `submit` except Gate.

---

## 3. Composition viewpoint (containers)

v1 is a **modular monolith** (one process, many crates). That is the HFT/OMS pattern for the hot path: shared memory, no RPC on `decide_ms`.

| Container | Crate | Role (classic name) |
|-----------|-------|---------------------|
| Domain | `neural-router-domain` | Shared kernel types; no I/O |
| Config | `neural-router-config` | Process settings; secrets redacted |
| MDG | `neural-router-data` | Market-data gateway: ingest, L0/L1 cache, validate |
| Analytics | `neural-router-ml` | IV/Greeks, funnel, utility, Δ band (not a live GNN) |
| Risk + OMS | `neural-router-execution` | Pre-trade gate, blotter, idempotency, EMS/Alpaca |
| Policy | `neural-router-policy` (**new**, bit 7) | Claude client + V0–V6; last-good file |
| Binary | `neural-router` | CLI, later HTTP, wiring |
| UI | `frontend/` | Read-only views + kill; no math |

**Firm graph (34 seats)** maps to **functions and timers inside these crates**, not 34 processes.

---

## 4. Logical viewpoint — two pipelines

### 4.1 Fast path (kernel, single thread)

Event: new **validated** snapshot (or timer if snapshot unchanged — still no HTTP inside `decide_ms`).

```
L0 current snapshot
    → Greeks/IV (this snapshot_id only)
    → funnel(policy) → Top20
    → dollar_Δ vs band
         Hold  → stop
         Breach → proposal (argmax or Quant JSON already validated)
              → Gate (1%/5%/OCC/ids/slippage/RTH)
              → journal audit
              → EMS submit (HTTP; **not** in decide_ms)
              → blotter state machine
```

`decide_ms` = Greeks + funnel + band + Gate. Stops before EMS.

### 4.2 Slow path (policy)

Event: wall clock 5–15 min, VIX jump, or operator.

```
Claude emit_policy / emit_ticket
    → V0…V6
    → last-good policy file XOR Reject
```

Slow path **never** blocks the fast path. Missing/stale brain → `policy_stale` + last-good.

### 4.3 Classic OMS order lifecycle

```
NEW ─► PENDING ─► SUBMITTED ─┬─► PARTIAL ─► FILLED
                             ├─► CANCELLED / EXPIRED
                             └─► REJECTED (broker or gate)
```

Illegal transitions are program errors (assert in debug; reject in release). Duplicate `client_order_id` → already SUBMITTED, not a second NEW.

---

## 5. Information viewpoint

| Store | Owner | Durability | Notes |
|-------|-------|------------|--------|
| Snapshot ring (2–4 slots) | MDG | process memory | L0; stamp `snapshot_id` |
| Last-good `Policy` | Policy | file / SQLite | survives restart |
| Top20 | Analytics | derived, per snapshot | never mix ids |
| Blotter | OMS | SQLite later; memory v1 | broker is fill truth |
| `audit.jsonl` | Observe | append-only disk | not log sampling |
| Fixture `testdata/spy_chain.json` | tests | git | no network in CI |
| Secrets | env | not in git | |

**Cache policy:** HLD-level — RAM snapshot + last-good policy. No shared CDN on `/api`. See `plan.md` §22.

---

## 6. Interface viewpoint (external)

| Interface | Protocol | Auth | SLA |
|-----------|----------|------|-----|
| Chain vendor | HTTPS poll v1 | vendor key | delayed; L1 TTL ≤ 15 min |
| Alpaca | HTTPS REST paper | key+secret | `broker_rtt_ms`; not in `decide_ms` |
| Claude | HTTPS Messages | API key | seconds; off hot path |
| Operator HTTP | HTTP 127.0.0.1 | optional token | kill even if STALE |
| `/metrics` | Prometheus text | localhost | |

Internal crate APIs: see LLD. **No** Claude tools that fetch or trade.

---

## 7. Non-functional requirements

| ID | NFR | Measure |
|----|-----|---------|
| NFR-1 | Kernel p99 ≤ 50 ms on fixture SPY chain | `nr_decide_ms`, criterion |
| NFR-2 | Single writer | `max instances = 1` |
| NFR-3 | Fail closed | no ticket on STALE/BRAIN_DOWN/schema miss |
| NFR-4 | Audit completeness | every Gate result and every Claude payload hashed |
| NFR-5 | No secrets in logs | redaction tests |
| NFR-6 | Paper by default | double flag for live |
| NFR-7 | Replay | same snapshot+policy+proposal → same `Reject`/`NewOrder` (pass^k) |
| NFR-8 | CI hermetic | no Alpaca/Claude/Yahoo in `cargo test` |

---

## 8. Deployment viewpoint

```
Laptop / later Cloud Run min=1
   neural-router (kernel+HTTP)
   frontend static files
   audit.jsonl, last_good_policy.json
   testdata/
```

GCP (after bit 8): Cloud Run + Secret Manager + Artifact Registry. **Not** GKE. Region `asia-south1` unless broker PoP says otherwise.

---

## 9. Control and failure

| Fault | Detection | Action |
|-------|-----------|--------|
| Feed timeout / 429 | L1 negative cache | keep last snapshot; if `data_age > max` → STALE_DATA, no tickets |
| Claude 429/timeout/bad JSON | transport | BRAIN_DOWN; last-good policy |
| Schema extra field | V2 | Reject; no overwrite last-good |
| OCC not in top20 | V4 | Reject; one retry |
| Broker 409 duplicate | EMS | treat as idempotent success |
| Blotter ≠ positions | recon | STALE_POS |
| Process crash | restart | load last-good + last snapshot if still fresh; else STALE |
| Operator kill | HTTP | inhibit submit; process stays up |

**Panic hedge** default **off** (`plan.md`). Breaker → no new orders.

---

## 10. Requirements trace (to execution bits)

| Bit | HLD piece |
|-----|-----------|
| 1 | Domain, stamps, contracts |
| 2 | MDG, L0/L1 |
| 3 | Analytics pipeline |
| 4 | Gate + OMS ids |
| 5 | EMS |
| 6 | Journal + metrics |
| 7 | Policy container + V0–V6 |
| 8 | Operator HTTP + UI |

---

## 11. What this HLD refuses

- LLM in the Disruptor/kernel thread  
- Multi-instance kernel without a uniqueness store  
- Treating paper fills as TCA  
- 34 LLM processes  
- Caching risk decisions or tickets  
