# Plan: bounded-agentic options overlay terminal

**Status:** design (not implemented)  
**Product:** paper-first portfolio options overlay (hedge / defined-risk), not 0DTE gambling and not colocated HFT.

This repo stays a Cargo workspace. Mental model shifts from “neural L2 router” to **options risk engine with a slow LLM policy layer**.

---

## 1. Problem

A long equity book has large positive dollar delta. A macro shock still hits names you like. Humans cannot recompute Greeks, hunt a liquid put, and rebalance all day without fat-fingers.

Build an autonomous **risk officer**:

- Watch holdings, prices, VIX, pending tickets.
- When dollar delta (or a vol-regime rule) leaves a band, buy index protection (SPY first; later QQQ/IWM/SPX paper).
- LLMs set **policy**. Rust does **numbers, gates, and orders**.

---

## 2. Constraints

- Solo-buildable, spiral delivery.
- Prototype: **free data + Alpaca paper + Claude keys**.
- LLM never computes Greeks, IV, or size.
- LLM never sits on the quote/tick path.
- Fail closed on risk. Missing credentials / bad JSON / stale policy → last-good lambdas or no trade, never “guess.”
- Paper fills are **not** a fill, impact, or latency model.
- Delayed data must be labeled `DELAYED` in the UI. Paper labeled `PAPER`.
- 4-pane grid is **one page**, not the product.

---

## 3. Latency SLA (honest)

**Promise:** p99 ≤ 50 ms for the **in-process kernel** once a snapshot is in RAM:

`chain → IV/Greeks → funnel → utility → risk gates → serialized TradeTicket`

**Do not promise:** 10–50 ms tick-to-fill through Yahoo, Massive delayed, or Alpaca REST.

Measure three clocks and show all three:

| Clock | Typical (prototype) | Owner |
|-------|---------------------|--------|
| `data_age_ms` | 15 min delayed, or broker RTT | feed |
| `decide_ms` | target p99 ≤ 50 ms | Rust kernel |
| `broker_rtt_ms` | ~80–200 ms REST (Alpaca) | broker |

### Papers / systems (what they actually show)

| Source | Claim | Use for us |
|--------|--------|------------|
| Ayatollahi 2026, *Orchestrated Intelligence* | Conceptual MAS; target 5–10 ms vs >50 ms “traditional”; **no empirical guarantee** | Safety gates on the orchestrator; agents off the hot path |
| Zalani 2025 (JCSTS) | GPU batch 1,024 option scenarios **&lt;15 ms** | In-process pricing throughput, not e2e fill |
| Horvath et al. 2021 | Millisecond IV-surface calibration | Funnel / surface class of work |
| Cao/Du *Relaver* (arXiv:2505.12465) | Realistic **non-colocated** MM lag **30–80 ms** | Alpaca’s world |
| Bilokon & Gunduz (arXiv:2309.04259) | C++ cache / Disruptor / compile-time tricks; [imperial_hft](https://github.com/0burak/imperial_hft) | Rust hot-path techniques |
| HFT practice (FPGA, colocation) | 10 ms is **not** HFT (they want µs/ns) | Out of scope |

**Load-bearing assumption:** quotes live in a Rust snapshot (preallocated ring / Disruptor-style). If every loop hits HTTP, the 50 ms SLA is false.

**Falsifier:** p99 `decide_ms` > 50 ms on a 2–5k SPY chain, or Claude is on that path.

---

## 4. Why options (not stocks)

- Options is the right **domain**: Greeks, surfaces, defined-risk overlays, audit.
- Directional stock picking is the generic agent demo already rejected.
- Long-term **skill** is options. Long-term **PnL** only if we stay defined-risk (long puts / collars), not short-vol lottery tickets.
- L2 GNN / 500 µs horizon is a **different product**. Defer. Do not mix “pick a put from L2 features” in one loop.

---

## 5. Architecture: two clocks

```
[delayed chain / paper quotes]
        │
        ▼
┌─────────────────── FAST (Rust) ───────────────────┐
│ Ingest → snapshot → funnel/Greeks → Δ band        │
│        → risk gates → Alpaca paper (if ticket)    │
│ Cadence: on new snapshot (seconds, broker-bound)  │
└───────────────────────────────────────────────────┘
        ▲ last-good {regime, λ, bounds}
        │
┌─────────────────── SLOW (Claude) ─────────────────┐
│ Strategist every 5–15 min or on VIX/news jump     │
│ Quant only when band is breached and top-20 ready │
│ Cadence: minutes; stale → last-good; never block   │
│          the fast loop                             │
└───────────────────────────────────────────────────┘
```

Crate mapping (existing workspace):

| Crate | Owns |
|-------|------|
| `neural-router-domain` | `OrderBookSnapshot` stays; add `OptionContract`, `Greeks`, `TradeTicket`, `Regime`, `LambdaWeights` |
| `neural-router-config` | env, paper flag, bands, 1%/5%, poll intervals; Debug redacts secrets |
| `neural-router-data` | ingest adapters (yfinance/Massive delayed, later OPRA), validator |
| `neural-router-ml` | funnel, IV/BS, optional SVI/PC2 (minutes clock), utility; **not** a GNN in v1 |
| `neural-router-execution` | `Broker`, risk gates, Alpaca paper |
| `neural-router` | CLI + later HTTP for UI; **not** the LLM |
| `frontend/` | 6-page terminal |

LLM client lives in a thin `crates/policy` (or `neural-router` bin module) that **writes JSON to a file/IPC** the kernel reads. Graph frameworks (LangGraph) are optional later; v1 can be “one structured Claude call + clamp.”

---

## 6. Agents: v1 hedge loop (2 LLMs + 4 machines)

The **firm graph** is 34 seats (5 departments × 6 + 3 MDs + CEO). That is an org chart, not 34 Claude processes. See §13.

v1 of the live hedge loop is still six roles; two speak English. Do **not** restore a 4-LLM swarm on the ticket path.

| # | Role | Type | Cadence | Does | Does not |
|---|------|------|---------|------|----------|
| 1 | **Ingest** | Rust | poll / WS | Pull chain, drop NaN/crossed, stamp `data_age` | Think |
| 2 | **Macro Strategist** | Claude JSON | 5–15 min or VIX/news | `{regime, dte_range, delta_range, budget, λ_svi, λ_pca, λ_eff}` | Touch strikes or qty |
| 3 | **Funnel + surface** | Rust | on snapshot | Filters, BS/IV, optional SVI/PC2 on **minutes** clock, z-score utility, top 20 | Call Claude |
| 4 | **Quant** | Claude JSON | band breach only | Pick 1 of 20 → `TradeTicket` | Compute Greeks |
| 5 | **Compliance / risk** | Rust validators | every ticket | Premium, DTE, 1%/5%, over-hedge, IOC/FOK | Approve in prose |
| 6 | **Executor** | Rust → Alpaca paper | after pass | Submit, PENDING/FILL/CANCEL | Retry-spam |

**HITL (operator):** kill switch, raise/lower bands, later authorize new strategy class. Not an LLM.

**Claude usage:** Strategist + Quant only, `temperature=0`, strict JSON schema. Failed/stale call → **last-good lambdas**; fast loop keeps running.

**Not in v1:** Researcher-as-LLM, Compliance-as-LLM, Vision, EOD Architect/RLM.

### 7-layer funnel (Rust, deterministic)

1. DTE in Strategist range (default 30–60).
2. Moneyness / delta in range (default put Δ ∈ [−0.50, −0.20]).
3. Liquidity: bid/ask spread ≤ 5% (configurable).
4. Event horizon: skip earnings/FOMC window when calendar says so.
5. Open interest / volume floor.
6. SVI residual / PCA PC2 **when surface job has run**; else skip factor (do not fake).
7. Z-scored utility with λ weights; emit top 20.

### Risk gates (Rust, fail closed)

- Size ≤ `equity * risk_limit_per_trade` (default 1%).
- Halt new risk if `daily_pnl ≤ −equity * max_daily_loss` (default 5%).
- Reject over-hedge (would flip dollar Δ through zero by more than a slack).
- Limit + IOC/FOK only. No market orders in v1.
- `rejection_count >= 3` in a window → circuit breaker (panic **deterministic** hedge policy or flat, never LLM argument).

---

## 7. Prototype APIs (free)

| Need | Free now | UI badge |
|------|----------|----------|
| Equity + delayed options chain | yfinance and/or Massive/Polygon Options Basic ($0, 15-min delayed) | `DELAYED` |
| Paper orders | Alpaca paper | `PAPER` |
| Events | Finnhub free calendar or static FOMC/earnings file | |
| Brain | Claude (existing keys) | off hot path |
| Greeks | this repo (BS/IV in Rust) | not vendor Greeks |

Later paid (not prototype): Alpaca Algo Trader Plus `feed=opra`, Polygon/Massive options realtime. Do not size or mark P&L off **indicative** OPRA.

Secrets: env only. `.env` gitignored. `Settings` Debug redacts keys.

---

## 8. Terminal: 6 pages

Same chrome on every page: NAV, dollar Δ vs band, VIX, `DELAYED`/`PAPER` badges, `data_age_ms` / `decide_ms`, kill (`k`).

Body swaps per tab. **No chatbot.**

| # | Page | Body |
|---|------|------|
| 1 | **Overview** | 2×2: (TL) NAV / Δ / VIX / posture; (TR) mini chain / selected hedge; (BL) holdings + per-name ΔΓΘ; (BR) pre/post-hedge PnL sketch + efficiency scatter |
| 2 | **Chain** | Full SPY chain (underlying switch later), filter chips = Strategist bounds, highlight top 20, bid/ask, IV, OI, utility |
| 3 | **Surface** | 3D IV mesh + PCA/SVI residual heatmap; **own HTTP**; never in Claude context |
| 4 | **Ticket / blotter** | Draft ticket, gate failures, paper orders, limit vs fill, FOK/IOC |
| 5 | **Agents** | Last Strategist JSON, last Quant pick, `rejection_count`, last-good vs live λ, `decide_ms` histogram |
| 6 | **Risk / settings** | 1%/5%, max DTE, paper toggle, kill, data source, `data_age` alerts |

Nav: tabs `1–6`. Monospace numbers, 1px grid.

Not v1: Scenario lab, Night audit, Strategy architect.

---

## 9. Implementation spirals

Build order with files, tests, and LLM validation: **`docs/execution.md`** (8 bits). Do not start bit N+1 until that bit’s exit gate is green.

Design: **`docs/hld.md`** (IEEE 1016-style HLD), **`docs/lld.md`** (contracts, OMS state machine, reject catalog).

### Spiral 1 — kernel (no LLM, no pretty UI)

1. Domain types: contract, greeks, ticket, snapshot.
2. Delayed chain ingest + validator.
3. BS/IV + 7-layer funnel (layers 6–7 stub-ok if no surface yet).
4. Dollar-Δ vs band; no-trade inside band.
5. 1%/5% + IOC ticket struct.
6. `criterion` bench: p99 `decide_ms` on SPY-sized chain.

**Exit:** `cargo test --workspace` green; bench logged; CLI prints top 20 + hold/hedge.

### Spiral 2 — paper + Claude policy

1. Alpaca paper `Broker` (fail closed without keys).
2. Strategist JSON → clamped `LambdaWeights` (file or stdin is enough).
3. Quant JSON on breach only.
4. Blotter state machine PENDING/FILL/CANCEL/REJECT.

**Exit:** one paper ticket in a dry run with mocked Claude; real Claude optional.

### Spiral 3 — terminal

1. HTTP snapshot API from Rust (no secrets in responses).
2. Pages 1, 2, 4 (Overview, Chain, Blotter).
3. Page 5 Agents (read-only JSON).
4. Page 3 Surface from `/matrix`.
5. Page 6 settings.

### Spiral 4 — only after kernel is honest

- SVI residual + frozen PC2 on minutes clock.
- Paid OPRA / Polygon.
- Optional frozen DRL overlay (never live-finetune).
- Extra underlyings.

---

## 10. Verify

- Unit tests: funnel, spread, Δ band, 1%/5%, schema clamp, redacted Debug.
- Mock Claude: invalid JSON rejected; last-good used.
- Benches: `decide_ms` p50/p99; never fold HTTP into that timer.
- Manual: Overview shows `DELAYED`; kill stops new tickets; no live orders.

---

## 11. Risks

- Delayed marks look like alpha.
- European BS on American equity options near expiry.
- Paper unlimited size / random partials ≠ market.
- Claude describes a hedge while qty is wrong — validators fail closed (TraderBench-class gap).
- Polling too hard → 429 / empty chain.
- Mixing L2-GNN “router” into the hedge loop.

---

## 12. Decision log

| Decision | Choice |
|----------|--------|
| Product | Options overlay / hedge terminal |
| Firm graph | 34 **seats** (5 depts × 6 + 3 MDs + CEO); not 34 LLMs |
| LLM budget | 5 roles in v1; hard cap **8** |
| v1 hedge loop | 2 Claude (Strategist, Quant) + 4 Rust |
| Latency | p99 ≤ 50 ms **kernel only** |
| Prototype data | Delayed free + Alpaca paper |
| UI | 6 pages; Overview is the 4-pane |
| LLM framework | Not required in v1; structured Claude + clamp |
| GNN / 500 µs L2 | Deferred, separate product |
| Scale | New **desks** (QQQ, …) copy workers; do not clone Claudes |
| Observability | OpenTelemetry → stdout JSON + histograms; audit ledger ≠ logs |
| Cloud | GCP; **Cloud Run** default; GKE only when the kernel is proven |
| CI | GitHub Actions; **not Jenkins** |
| Region | `asia-south1` (Mumbai) unless the broker/data PoP says otherwise |
| Secrets | GCP Secret Manager in cloud; never in images or log lines |
| Cache | In-process snapshot + last-good policy first; Redis only if kernel and policy split; never cache tickets or audit |
| Execution | `docs/execution.md` — 8 bits; Claude `strict: true` then Rust V2–V5 semantic gates |

---

## 13. Firm graph: 34 seats, 5–8 brains

A hedge fund org chart is the right metaphor: departments, veto, private pads, scheduled IC. It is the wrong implementation if every box is an LLM.

**Rule:** 34 is **capacity** (typed seats). Claude count is a **budget**. Real funds put most people in recon, data, engineering, and junior research. The IC is 5–8 humans once a day, not 34 people on every quote.

LLM “simulate a firm” papers stay small: TradingAgents ~7, HedgeAgents 3 experts + 1 manager, QuantAgent 4. Huge reported Sharpes on those stacks are often short bull-window artifacts. Hierarchical routing beats all-to-all debate on cost vs quality.

34 Claude calls per hedge: minutes of latency, O(n²) chat, context soup, dead 50 ms kernel. Rejected.

```
                    CEO / CIO  (LLM, rare)
                           │
          ┌────────────────┼────────────────┐
          │                │                │
     MD Markets        MD Risk/CRO      MD Ops/COO
     (LLM, slow)       (mostly Rust +   (Rust + LLM
                        LLM exception)    on incidents)
          │                │                │
     5 departments, each: Head + 5 workers  (= 30 seats)
```

**5 departments × 6 seats = 30**, plus **3 MDs + CEO = 34**.

---

## 14. Departments (6 seats each)

| Dept | Head | 5 workers (code) |
|------|------|------------------|
| **1. Macro / Research** | Strategist LLM | calendar, VIX trigger, news fetch, FOMC/earnings gate, RSS digest |
| **2. Portfolio / Quant** | Quant LLM (band breach only) | funnel, IV/BS, SVI job, PC2 job, z-score utility |
| **3. Trading** | Head = **Rust** (no LLM) | router, limit/IOC, blotter, positions, Alpaca adapter |
| **4. Risk / Compliance** | CRO exception LLM | 1%/5%, over-hedge, schema clamp, circuit breaker, stress grid |
| **5. Platform / Ops** | COO incident LLM | ingest, `data_age`, validator, audit log, UI API |

**6 seats per dept is the unit. 1 brain + 5 machines, not 6 Claudes.**

Adding a worker later is a crate with `Input → blackboard slot`. Adding an underlying is a new **desk** that reuses these worker types.

---

## 15. LLM budget (who actually calls Claude)

Hard cap: **8** concurrent LLM roles. v1 uses **5**.

| Seat | Cadence | Authority |
|------|---------|-----------|
| CEO / CIO | daily IC + on kill | authorize strategy class, risk appetite, halt firm |
| MD Markets | 5–15 min / VIX jump | `{regime, λ, DTE, Δ}` |
| MD Risk | only on reject / exception | **veto**; cannot enlarge size |
| Head Quant | band breach, **one shot** | pick among top-20 |
| MD Ops | data incidents | stale feed / 429 / recon break |
| Night auditor | EOD (later, within cap) | postmortem, no orders |

Trading head is never an LLM. Risk **numbers** are never an LLM.

v1 hedge loop still only **Strategist + Quant** on the live path (maps to MD Markets + Head Quant). CEO, CRO-exception, Ops-incident can stay stubs until Spiral 2–3.

---

## 16. Authority (veto ladder, not “senior talks more”)

```
Worker  →  Dept head  →  MD  →  CEO
                │
                └── Risk MD can veto anyone below CEO
```

- CEO cannot silently bypass Risk on **size**. CEO can change **appetite** (bands) or **kill**.
- Quant LLM **proposes** a ticket. Risk **machine** accepts/rejects (inequalities).
- CRO LLM writes a **memo** if it wants a policy change; it does not `qty *= 2`.
- CEO never sees 4,980 strikes. CEO sees posture, dollar Δ, rejects, PnL, `data_age`.

That is segregation of duties: the PM does not mark their own homework.

---

## 17. Memory (four ledgers + audit)

Private write-by-owner pads, department share, and an executive meeting layer. Not a 34-way group chat.

| Ledger | Who writes | Who reads | Contents |
|--------|------------|-----------|----------|
| **Private pad** | only that seat | owner + audit hash | working notes, last prompt, mistakes |
| **Dept blackboard** | workers write **typed slots** only | head + workers of that dept | `top20`, `vix`, `rejects[]` — not free prose |
| **Floor brief** | 3 MDs | MDs + CEO | 1-page JSON: regime, risk, ops health |
| **CEO vault** | CEO + IC secretary (ops) | CEO, MDs | authorized strategies, appetite, IC minutes |
| **Firm audit** | **append-only, all** | compliance, operator | every ticket, every reject, every LLM JSON |

Rules:

1. Owner-write on private pads. No overwriting another seat’s pad.
2. No raw options chain on the floor brief or CEO vault (same split as the 3D surface vs LLM).
3. Blackboard is **slots**, not Slack. `policy.lambda_pca = 0.8`, not “I feel we should…”.
4. Downward copy is **policy**. Upward copy is a **brief**. Workers do not read the CEO vault.
5. Desks do not read each other’s pads (multi-PM rule). Shared across desks: risk + positions, not ideas.
6. No shared RAG blob across 34 agents (anchoring / experience-following).

---

## 18. Meeting flow (scheduled IC, not perpetual debate)

Never all-hands on a quote. Four conference types:

| Meeting | Who | When | Output |
|---------|-----|------|--------|
| **Morning IC** | CEO + 3 MDs (dept heads may attach **read-only briefs**) | session open | clamped appetite + regime |
| **Breach huddle** | Quant head + Risk machine (+ CRO if rejected) | dollar Δ out of band | one ticket or hold |
| **Extreme** | CRO + CEO + **panic Rust** | VIX / gap / breaker | panic hedge or flatten; no debate |
| **EOD** | COO + heads’ summaries | close | audit pack; no new risk |

Quant vs Strategist do **not** argue in a loop. One Quant shot; optional **one** retry with the validator error string; then circuit-break.

---

## 19. Scaling

| Do | Do not |
|----|--------|
| Add a **desk** (QQQ) that reuses worker types | Spawn 6 new Claudes per underlying |
| Add a worker crate that writes a new blackboard slot | Give every worker a prompt and a vote |
| Raise LLM budget only with a named seat in §15 | “Just add agents until 30” |
| Keep IC membership fixed as desks grow | Put every desk head in every breach huddle |

Target shape at 10 underlyings: still **≤8 LLM roles**, N × 5 worker types, same 4 ledgers. One kernel binary (or two: kernel + policy job), not a pod per seat.

---

## 20. Why not 6 LLMs per department

| 6 Claudes × 5 depts | Effect |
|---------------------|--------|
| Latency | 30 × (2–15 s) before a put is legal |
| Cost | rate-limit on a quiet Tuesday |
| Context | 6 pads “shared with the head” recreates the summarizer/RAG mess already rejected |
| Authority | 6 opinions, one kernel = a committee |
| 50 ms kernel | dead |

**6 seats per dept is correct. 1 brain + 5 machines is the scalable unit.**

Target shape at 10 underlyings: still **≤8 LLM roles**, N × 5 worker types, same 4 ledgers. One kernel binary (or two: kernel + policy job), not a pod per seat.

---

## 21. Scalability: metrics, logs, GCP, k8s, CI

Do not stand up GKE + Jenkins so the paper hedge “looks production.” Observability is a **three-clock + audit** problem. Compute is a **warm kernel vs bursty UI/LLM** problem. Those want different platforms.

### 21.1 Four pipes (do not mix)

| Pipe | Job | Retention | Backend (later) |
|------|-----|-----------|-----------------|
| **Metrics** | p50/p99, rates, gauges | 15–90 days | OTel histograms → GCP Managed Prometheus / Cloud Monitoring |
| **Logs** | debug, incidents | ~30 days | `tracing` JSON stdout → Cloud Logging |
| **Traces** | one snapshot’s ingest→funnel→risk→broker | 7–14 days | OTel → Cloud Trace |
| **Audit ledger** | tickets, rejects, LLM JSON, IC minutes | years | **app-owned** append-only (SQLite → Postgres/GCS). Not log sampling. |

Logs can drop. Metrics can downsample. **Audit cannot.** That is §17.

Never log: API keys, `.env`, `Authorization`, Alpaca secrets. Redact like `Settings` Debug (`[REDACTED]`). Do not dump full Claude prompts that contain book size unless they go only to the audit ledger with access control.

### 21.2 Metric catalog

Namespace: `nr_`. Histograms for anything in the 50 ms story. Instrument with **OpenTelemetry** in Rust; export OTLP. Do not couple application code to a GCP SDK.

| Name | Type | Labels | Why |
|------|------|--------|-----|
| `nr_data_age_ms` | histogram | `source` | DELAYED vs live honesty |
| `nr_decide_ms` | histogram | | kernel SLA (HTTP **not** inside this timer) |
| `nr_funnel_ms` `nr_greeks_ms` `nr_utility_ms` `nr_risk_ms` | histogram | | where 50 ms is spent |
| `nr_broker_rtt_ms` | histogram | `op` | Alpaca; not folded into `decide_ms` |
| `nr_tickets_total` | counter | `result=accept\|reject\|breaker` | |
| `nr_rejection_count` | gauge | | circuit breaker |
| `nr_dollar_delta` | gauge | | vs band |
| `nr_band_breach` | gauge | `0\|1` | |
| `nr_circuit_breaker` | gauge | `0\|1` | |
| `nr_policy_stale` | gauge | `0\|1` | last-good lambdas |
| `nr_llm_ms` | histogram | `role` | Strategist/Quant/CEO; **slow clock** |
| `nr_llm_tokens` | counter | `role, dir=in\|out` | cost |
| `nr_llm_schema_fail_total` | counter | `role` | fail-closed |
| `nr_alpaca_errors_total` | counter | `code` | |
| `nr_http_429_total` | counter | `source` | yfinance/Massive |

Alerts (when cloud exists): `decide_ms` p99 > 50 ms for 5 min; `data_age` too old; breaker=1; schema_fail rate; 429s.

v1 (laptop): `/metrics` Prometheus text + `tracing` to stderr. `criterion` is the lab bench; live histograms are the production bench.

Hot path: no sidecar shipper, no “log every contract.” Top-20 and tickets → audit. Surface XYZ → never in logs.

### 21.3 Compute on GCP (phased)

Region: **`asia-south1`** unless Alpaca/data PoP is clearly US-only (then `us-central1` and accept RTT). Cloud Run cold start is hundreds of ms to seconds — that **breaks** the kernel SLA unless **min instances = 1** (or a small always-on VM).

| Spiral | Kernel (fast) | UI + HTTP API | LLM policy (slow) | Jobs (IC, EOD, surface) |
|--------|---------------|---------------|-------------------|-------------------------|
| 1–2 local | `cargo run` | later | Claude from the box | in-process timers |
| 3 | **Cloud Run**, **min instances = 1**, CPU always allocated | Cloud Run (may scale to 0) | Cloud Run **job** or same service off the tick | **Cloud Scheduler** → jobs |
| 4+ | GCE n2 (pin CPU) **or** GKE **only if** you already need k8s | Cloud Run | Cloud Run | Scheduler + jobs |

**Cloud Run** is the default: HTTP API, Claude proxy, Next UI. Google’s split: Run for APIs; GKE for custom networking, GPUs, stateful workers.

**GKE:** not until ≥3 long-running workers, custom HPA on `nr_band_breach`, or a GPU surface. Autopilot if you ever go. Do not put the 50 ms kernel on a burst-empty node pool.

**Compute Engine:** allowed as a **tiny always-on kernel VM** if Run min-instance jitter is worse. Later CPU pinning. Not a 34-pod mesh.

**Do not use (prototype):** Dataflow, Anthos, mesh, Cloud Functions per agent.

### 21.4 GCP products that belong

| Product | Use |
|---------|-----|
| **Artifact Registry** | images |
| **Secret Manager** | Alpaca, Claude, later OPRA. Workload identity; no keys in YAML |
| **Cloud Logging** | stdout JSON |
| **Cloud Monitoring** + **Managed Prometheus** | `nr_*` + SLO on `decide_ms` |
| **Cloud Trace** | OTLP |
| **Cloud Scheduler** | morning IC, EOD, surface minutes-clock |
| **Cloud Storage** | audit archives, surface meshes, criterion reports |
| **Cloud SQL Postgres** | when SQLite is too small: blotter, IC minutes |
| **Pub/Sub** (later) | `ticket.accepted` / `breaker.tripped` to UI — not 34 agent chats |
| **Memorystore Redis** (later) | last-good `Policy` if kernel and policy split |

IAM: kernel SA reads secrets + writes audit bucket. UI SA cannot read Alpaca secrets. LLM job SA gets Claude secret only.

### 21.5 Kubernetes

K8s is an orchestration tax. One binary + one UI does not need a cluster.

If GKE happens: kernel Deployment with `requests=limits`, no CPU burst, OTLP from the process (not a log sidecar on that pod). Policy/LLM Deployment **separate**, may scale to 0. Never co-locate Claude on the kernel pod. Use [Managed OTel for GKE](https://docs.cloud.google.com/kubernetes-engine/docs/concepts/managed-otel-gke) only once you are on GKE.

Until then: **zero cluster YAML.**

### 21.6 CI/CD — GitHub Actions, not Jenkins

The repo is git. Jenkins is another server, agents, and identity store. Skip unless a firm already runs it.

| Workflow | What |
|----------|------|
| `pr` | `cargo fmt`, `clippy -D warnings`, `cargo test --workspace` |
| `bench` (nightly / main) | `criterion` on a fixture chain; fail if p99 `decide_ms` regresses >20% |
| `image` | docker → Artifact Registry (only when deploying) |
| `deploy` | Cloud Run from `main`; manual approve when secrets are involved |

No `--no-verify`. No secrets in Actions logs.

### 21.7 What actually scales

| Axis | How | Infra |
|------|-----|--------|
| Desks / underlyings | copy workers, same LLM budget | more ingest QPS, still 1 kernel |
| Chain 5k → 20k | Rust + benches | CPU; still not k8s by itself |
| UI users | 1 operator → a few | Cloud Run UI |
| Audit years | tickets forever | GCS + Postgres, not log sinks |
| LLM roles | cap 8 | Cloud Run jobs + Scheduler |

The firm graph does **not** scale as “a Deployment per seat.” ~26 workers are functions in one binary. 5–8 brains are **jobs**.

### 21.8 Spiral mapping

1. **Now:** `tracing` JSON, `/metrics`, criterion. No cloud.  
2. **First deploy:** Cloud Run (kernel min=1) + Secret Manager + Artifact Registry + GitHub Actions.  
3. **Graphs:** Cloud Monitoring + Trace.  
4. **GKE / Redis / Pub/Sub / Cloud SQL:** after paper tickets and the 50 ms histogram are real.

---

## 22. Caching

The 50 ms SLA is a **cache hit on RAM**. Last-good policy is a **cache**. Claude’s static prefix is a **prompt cache**. Mixing those with “cache the ticket” or “CDN the dollar Δ” is how you hedge yesterday.

Every cache has: **key, TTL, stamp (`asof`, `snapshot_id`), who may read, invalidation, fail-closed behavior.**

### 22.1 Layers

| Layer | Key | TTL / freshness | Where (v1 → later) | Invalidation | Fail closed |
|-------|-----|-----------------|---------------------|--------------|-------------|
| **L0 Snapshot ring** | `desk` (SPY) | replaced on ingest; `data_age_ms` stamped | in-process ring (2–4 slots) | new validated snapshot | if `data_age > max_age` → no new tickets (`STALE_DATA`) |
| **L1 HTTP ingest** | URL + query | **≤ delayed interval** (15 min for free Massive/Yahoo). SWR: serve last good while refresh | process map; later Memorystore | 429/5xx keep last good; **do not** hammer | empty + too old → halt ingest tickets |
| **L1b Negative** | host | exponential backoff 1s→60s on 429 | process | success clears | count `nr_http_429_total` |
| **L2 Calendar / FOMC / earnings** | date, ticker | **until next session** (hours–days) | file / GCS | daily job | missing calendar → treat as “event unknown”, skip event-horizon skip **or** skip trades that day (pick one; default: skip event filter, log `calendar_missing`) |
| **L3 Greeks / IV** | `(occ, spot_bucket, T_bucket, r, iv_seed)` | **same snapshot only** (`snapshot_id`) | compute-on-snapshot, no cross-snapshot reuse of Δ | new snapshot_id | never reuse Greeks from a previous snapshot |
| **L4 Funnel / top-20** | `snapshot_id + policy_id` | same snapshot | process | policy or snapshot change | Quant may only pick from this set |
| **L5 Surface mesh** | `desk + minute_bucket` | **minutes clock** (e.g. 5–15 min) | process + GCS object | scheduler job | if missing, funnel **skips** SVI/PC2 (do not fake) |
| **L6 Policy last-good** | `desk` | until new **clamped** Strategist JSON | file / SQLite → Redis | new policy_id; schema fail **does not** overwrite | kernel runs on last-good; `nr_policy_stale=1` |
| **L7 Positions / blotter** | account | poll 1–5 s or WS | process; source of truth = broker | fill/cancel | never invent a fill from cache |
| **L8 Claude prompt cache** | static prefix | Anthropic prompt cache (hours) | vendor | prompt version bump | if cache miss, still call; do not skip Quant |
| **L9 UI snapshot** | `ETag = snapshot_id` | 200–500 ms poll or WS | browser memory | ETag change | **no** Cloud CDN on `/api/*` |
| **L10 Static UI** | hashed assets | long TTL | Cloud CDN / Cloud Storage | content hash | fine |
| **L11 CI / build** | Cargo + docker layers | Actions cache | GitHub | lockfile change | n/a |

**Memorystore Redis** only when kernel and policy **split processes**. One Cloud Run min=1 kernel: **in-process is enough**. Redis as a second snapshot of the book is extra RTT and extra stale.

### 22.2 Stamps (without these, cache is a bug)

Every cached object carries:

```text
snapshot_id | policy_id | surface_id | asof_unix_ms | source | delayed: bool
```

Kernel refuses to mix `top20.snapshot_id != book.snapshot_id` or `top20.policy_id != policy.policy_id`. Quant output is bound to those ids; if they drifted, reject.

### 22.3 What you must never cache

- Risk **decisions** (recompute every ticket).
- Order acks / fills (broker is source of truth).
- Audit rows (append-only; not a TTL map).
- Secrets.
- Dollar Δ on Cloud CDN or a public HTTP cache.
- Claude **answers** as if they were still valid after a new snapshot (last-good **policy** is allowed; last-good **ticket** is not).
- Indicative quotes as “OPRA.”

### 22.4 Prompt caching (slow clock, money)

Match the original Gemini “KV cache” idea without stuffing the chain into the prompt.

- **Stable prefix (cacheable):** system prompt, JSON schema, role, veto rules. Mark `cache_control: ephemeral` (Anthropic) so input tokens after the first call are cheap.
- **Tiny dynamic suffix:** `{regime, λ, band, dollar_Δ, VIX, top20[]}` only. Not 5k contracts.
- **Quant retry:** same prefix + validator error; still one retry max.
- Version the prefix (`PROMPT_STRATEGIST_v3`). Bumping the version is the invalidation.

Do **not** use a vector RAG over pads as a cache. That is §17 rule 6.

### 22.5 HTTP / CDN

| Path | Cache |
|------|--------|
| `GET /api/snapshot` | private, `ETag`, max-age=0 or 1s; **no** shared CDN |
| `GET /api/matrix` | private; max-age aligned with surface minutes clock |
| `GET /api/blotter` | no-store |
| `POST /api/kill` | no-store |
| `/assets/*` | CDN, immutable, hashed filenames |

Cloud CDN in front of the **API** is how you show a 15-minute-old Δ to two browsers and think you hedged.

### 22.6 Multi-instance

Kernel Cloud Run **max instances = 1** (or a single GCE VM) until there is a single-writer story. Two kernels + two snapshot caches = two tickets.

If you ever scale out: one writer for tickets (`client_order_id` idempotency + DB unique), snapshots in Redis with `snapshot_id`. That is spiral 4+, not v1.

### 22.7 Warm-up

On process start:

1. Load last-good policy from disk.
2. Load last snapshot if `data_age` still legal; else ingest once **before** serving ready.
3. Precompute Greeks for that snapshot (cache warming; same idea as the low-latency C++ repo).
4. Readiness probe **fails** until (1)+(2) or explicit `STALE_DATA`.

Never send a ticket on a cold empty ring.

### 22.8 Metrics

Add: `nr_cache_hit_total{layer}`, `nr_cache_miss_total{layer}`, `nr_cache_stale_total{layer}` for L0–L6. Alert if L0 miss rate is high (means you are deciding on HTTP).

---

## 23. Gap check (what was still missing)

Caching was the hole. These were also not spelled out; they are now in-scope for the plan.

| Gap | Decision |
|-----|----------|
| **Order idempotency** | Every submit has `client_order_id = hash(snapshot_id, policy_id, occ, qty, side)`. Alpaca duplicate = not a second fill. |
| **Single writer** | One kernel instance until Redis+DB uniqueness exists. |
| **Market hours** | No new risk outside RTH (config). Overnight: flatten policy is human/CEO, not Quant. |
| **Time source** | Stamp `exchange_ts` from feed and `local_ts`. `data_age = now - exchange_ts`. NTP on the VM. |
| **Rate budget** | Token bucket per vendor (Yahoo/Massive/Alpaca). Ingest yields to the bucket; never a tight loop. |
| **Position recon** | On start and every N minutes, GET positions; blotter vs broker mismatch → `STALE_POS`, no new tickets. |
| **Readiness / liveness** | Ready = snapshot+policy loaded (or stale flagged). Live = process up. Kill switch is **not** “unhealthy” (must still accept `POST /kill`). |
| **UI auth** | Even paper: loopback or a shared token. No open `/api` on the internet. |
| **Claude cost cap** | Daily token budget; on exhaust, last-good policy only. |
| **Prompt versioning** | Prefix versions in audit. |
| **Feature flags** | `llm_quant`, `llm_strategist`, `surface_svi` — default Quant off in spiral 1 (`argmax` utility). |
| **Session calendar** | US equity holidays file; do not poll 365×24. |
| **Backoff + jitter** | 429/5xx; full jitter. |
| **Fixture cache for tests** | Golden SPY chain on disk; benches never hit network. |
| **DR / restart** | Policy file + last snapshot file in GCS/SQLite; secrets only in Secret Manager. |
| **PII / prompt leak** | Book notionals in audit, not in Cloud Logging. |
| **American vs European** | Documented risk; near expiry extra slack or exclude DTE < N. |
| **Paper ≠ live flag** | Impossible to “accidentally” point kernel at live URL without `ALPACA_PAPER=false` **and** a second explicit env. |
| **WebSocket vs poll** | v1 poll + L1 cache. WS later; same snapshot ring. |
| **Connection pool** | Reuse TLS to Alpaca/Claude; that is not a data cache. |
| **Build cache** | Actions `Swatinem/rust-cache` or sccache; docker layer cache. |

Still **out of scope** (do not pretend they are designed): multi-region active-active kernel, FIX, FPGA, live DRL, per-seat Deployments.

---

## 24. Cache + infra spirals

1. L0 ring + L6 policy file + fixture cache + `client_order_id`.  
2. L1 ingest TTL + 429 backoff + Anthropic prompt cache + feature flags.  
3. L5 surface on minutes clock + UI ETag + Cloud Run min=1.  
4. Redis only if policy service splits; CDN **static only**.
