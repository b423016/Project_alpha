# Neural Order Book Execution Router – Code Documentation

## Project Structure

Cargo workspace. Libraries expose a small public API from `lib.rs`; binaries stay thin.

```mermaid
graph TD
    A[neural-router workspace] --> B[crates/domain]
    A --> C[crates/config]
    A --> D[crates/market-data]
    A --> E[crates/ml-core]
    A --> F[crates/execution]
    A --> G[crates/neural-router]
    A --> H[frontend]
    A --> I[data/raw processed]
    A --> J[models]
    A --> K[.env.example]

    D --> D1[loader.rs]
    D --> D2[preprocessor.rs]
    D --> D3[validator.rs]

    E --> E1[model.rs]
    E --> E2[train.rs]
    E --> E3[predict.rs]
    E --> E4[constraints.rs]

    F --> F1[alpaca.rs]
    F --> F2[router.rs]
    F --> F3[risk.rs]

    G --> G1[src/main.rs]
```

Dependency rule: `domain` and `config` have no crate-graph children. I/O lives behind traits (`L2Source`, `Broker`).

---

## Core Crates

### `crates/domain` (`neural-router-domain`)

Shared types only: `OrderBookSnapshot`, `PriceLevel`, `Prediction`, `Side`.

No I/O, no config, no broker types.

### `crates/config` (`neural-router-config`)

`Settings::from_env()` and `Settings::default()`.

API keys are optional at load time so tests do not need credentials. `Debug` redacts secrets. Brokers must fail closed if keys are missing.

### `crates/market-data` (`neural-router-data`)

| Module | Role |
|--------|------|
| `loader` | `L2Source` trait; `collect` (Polygon ingest not wired) |
| `preprocessor` | order imbalance |
| `validator` | depth, ordering, uncrossed spread |

```bash
cargo run -- collect --symbol SPY
```

### `crates/ml-core` (`neural-router-ml`)

| Module | Role |
|--------|------|
| `model` | `NeuralOrderBookModel` handle |
| `train` | training entrypoint (not wired) |
| `predict` | inference entrypoint (not wired) |
| `constraints` | spread ≥ 0; probabilities in `[0, 1]` |

```bash
cargo run -- train --epochs 100 --batch-size 1024
```

### `crates/execution` (`neural-router-execution`)

| Module | Role |
|--------|------|
| `alpaca` | `Broker` trait; credentials required at construct |
| `router` | widen > 0.7 buy; narrow > 0.7 sell |
| `risk` | 1% size, 5% daily loss halt |

```python
# equivalent decision (Rust: decide())
if signal.spread_widening_prob > 0.7:
    buy(risk_adjusted_size)
elif signal.spread_narrowing_prob > 0.7:
    sell(risk_adjusted_size)
```

### `crates/neural-router` (binary)

Clap subcommands: `collect`, `train`, `predict`, `execute`, `backtest`.

Loads `.env` in the binary only, then `Settings::from_env()`.

### Frontend

TypeScript dashboard. Not part of the Cargo graph.

---

## Configuration

See `.env.example`. Defaults:

- `ORDER_BOOK_LEVELS=10`
- `PREDICTION_HORIZON=500` (microseconds)
- `RISK_LIMIT_PER_TRADE=0.01`
- `MAX_DAILY_LOSS=0.05`
- `ALPACA_PAPER=true`
- `SYMBOL=SPY`

---

## Development Workflow

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run -- --help
```

I/O commands currently exit with `not implemented: …` until Polygon, training, Alpaca transport, and replay are written.

---

## Architectural Decisions

- **Workspace, not a mega-crate.** Each crate has one reason to change.
- **Vertical data flow:** raw snapshot → validate → features → predict → risk → route.
- **Traits at I/O edges.** Polygon and Alpaca are replaceable.
- **Fail closed.** Missing broker credentials and broken books are errors, not defaults to trade.
- **Physics constraints** live in `ml-core`, independent of the model backend.
- **Progressive enhancement:** SPY-only, paper trading, then live.

---

## Maintenance Notes

- Run `cargo test --workspace` on every change to domain/risk/router.
- Rotate API keys quarterly; they must never appear in source, logs, or `Debug`.
- Retrain on a schedule once the training adapter exists.
- Track implementation shortfall, adverse selection rate, and prediction accuracy once execution is live.
