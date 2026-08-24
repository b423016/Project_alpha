# Agent instructions — Neural Router

Paper-first **options overlay kernel** (SPY puts, dollar-Δ band). Rust owns numbers, gates, and orders. Claude may only **propose** JSON. This is not an L2 GNN, not HFT, not a 34-LLM swarm.

## Read this first

| Doc | Role |
|-----|------|
| `docs/hld.md` | Architecture (IEEE 1016). Trust this over README/code_readme for product shape. |
| `docs/lld.md` | Types, crate contracts, OMS states, **reject codes**. Use these names. |
| `docs/execution.md` | **Build order.** 8 bits. Do not start bit N+1 until bit N’s exit gate is green. |
| `docs/plan.md` | Product, firm graph, cache, GCP. Do not re-derive. |

If code and LLD disagree, **stop and update LLD** (or fix code). Do not invent a third name.

## Non-negotiables

1. **Two clocks.** Fast: ingest → Greeks → funnel → Δ band → Gate → EMS. Slow: Claude. LLM never on the tick path, never computes Greeks, IV, or size.
2. **Fail closed.** Stale data, bad JSON, missing keys, id mismatch → **no new ticket**. Last-good **policy** only. Never last-good ticket.
3. **Single writer.** One kernel mutates ring, top20, blotter, risk, breaker. Ingest/Claude post on a bounded mailbox. Kernel Cloud Run `max instances = 1`.
4. **Gate before send.** Nothing calls `Broker::submit` except post-`gate`. Audit append **before** submit.
5. **Schema ≠ semantic.** Claude `strict: true` is V0. Rust V2–V5 still run. Provider **strips min/max** — re-check ranges. No auto-repair of `occ_symbol` / `side` / `qty` / `limit`. No `"two"` → `2`.
6. **Money:** `i64` cents. **Qty:** `u32`. **Side v1:** `BUY` only. Bind every proposal to `snapshot_id` + `policy_id`.
7. **Paper default.** Live requires `ALPACA_PAPER=false` **and** `ALLOW_LIVE=1`. No real Alpaca/Claude/Yahoo in `cargo test`.
8. **`decide_ms` excludes HTTP.** Folding broker RTT into the 50 ms SLA is a bug.
9. **No secrets in logs/Debug.** Redact. Never commit `.env`.
10. Feature flags `LLM_STRATEGIST` / `LLM_QUANT` default **off** until bit 7 vectors are green.

## Crate map (keep DAG)

```
domain          types only — no I/O
config          env; Debug redacts
market-data     ChainSource, validate, snapshot ring, L0/L1 cache
ml-core         IV/Greeks, funnel, utility, Δ band (not a live GNN)
execution       gate, blotter, idempotency, Broker/Alpaca
policy          (add in bit 7) Claude + V0–V6; depends on domain+config only
neural-router   binary wires all; HTTP in bit 8
```

`policy` must **not** import Alpaca. Put `Reject`, `TicketProposal`, `Policy` in **domain** if that avoids a cycle.

34 firm-graph **seats** = functions/timers in these crates, not 34 processes.

## Code style

- Edition **2024**, `thiserror` in libs, `anyhow` only in the binary.
- Small public API from `lib.rs`; modules private unless LLD exports them.
- `#[serde(deny_unknown_fields)]` on Policy/ticket DTOs. No `#[serde(default)]` on required fields.
- Reject codes: **only** the catalog in `docs/lld.md` §6. New code → update LLD in the same change.
- `client_order_id = hex(blake3(snapshot_id || policy_id || occ || qty_le || side))`.
- Comments: **why** (invariants, fail-closed). No narration, no commented-out blocks.
- Do not `unwrap`/`expect` on vendor/LLM/broker paths. Tests may unwrap fixtures.
- Do not add GKE, Jenkins, Redis, LangGraph, CrewAI, or a chatbot unless a later bit says so.
- Do not implement SVI/PC2, live DRL, or page-3 surface until bits 1–8 are done (`docs/execution.md` “not in these 8 bits”).

## LLM path (bit 7)

Tools: `emit_policy`, `emit_ticket` only. No fetch, no broker.

```
V0 grammar → V1 extract tool → V2 serde deny_unknown
→ V3 min/max + finite → V4 ids + OCC∈top20
→ V5 kernel recomputes qty/limit → V6 audit → gate
```

One retry with structured `Reject { code, field, got }`. Second fail: Quant → no ticket; Strategist → do not overwrite last-good. `reason`/`why` are log-only.

## Tests

- Hermetic: fixtures under `testdata/` (add in bit 2). Golden JSON under `crates/policy/tests/vectors/` (bit 7).
- After a bit: that crate’s tests + `cargo test --workspace` if you touched shared types.
- `cargo fmt` + `cargo clippy --workspace --all-targets -- -D warnings` when the toolchain has them.
- Claim “done” only with command output. Do not weaken tests to go green.

## When implementing

1. Identify the **bit** in `docs/execution.md`. Implement **only** that bit plus required LLD types.
2. Match `docs/lld.md` names (`SnapshotId`, `Stamps`, `gate`, `SnapshotRing`, …).
3. Prefer extending existing modules (`execution` risk/alpaca, `market-data` validator) over parallel files.
4. If a GNN/500µs/order-book router leftover blocks the overlay, replace it in place; do not keep two products in one crate.

## Product one-liner

Defined-risk **index put overlay** on a long book, Alpaca paper, delayed data labeled `DELAYED`. 50 ms is **in-RAM kernel**, not tick-to-fill.
