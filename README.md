# Neural Order Book Execution Router

**AI-powered trading system for real-time order book prediction and optimized execution.**

![License](https://img.shields.io/badge/license-Apache%202.0-blue)

---

## Table of Contents
- [Concept Overview](#concept-overview)
- [Architecture](#architecture)
- [System Components](#system-components)
- [Installation](#installation)
- [Configuration](#configuration)
- [Workflows](#workflows)
- [Testing & Validation](#testing--validation)
- [Deployment](#deployment)
- [Roadmap](#roadmap)
- [Contributing](#contributing)
- [License](#license)

---

## Concept Overview

Neural Order Book Execution Router is an AI-powered trading system that:

- Processes Level 2 market data in real-time
- Predicts microsecond-scale order book movements using Graph Neural Networks (GNNs)
- Executes trades to avoid adverse selection and toxic flow
- Optimizes order routing using AI signals
- Provides real-time performance monitoring

**Core Value Proposition:** Saves **1–3 basis points** per trade for institutional clients via superior execution timing.

---

## Architecture

This is a Cargo workspace. Library crates own domain logic; the `neural-router` binary is a thin CLI.

```text
handlers (CLI) → services (ml / execution / market-data) → domain types → external I/O
```

Crate dependency direction:

```text
neural-router (bin)
  → neural-router-data
  → neural-router-ml
  → neural-router-execution
       ↘         ↘          ↙
         neural-router-config
         neural-router-domain
```

### Key Data Flows

- **Market Data Ingestion:** Real-time L2 data via WebSocket (Polygon; not wired yet)
- **Prediction Pipeline:** Order book → Feature extraction → Model inference
- **Execution Loop:** Signal → Risk check → Order routing → Confirmation
- **Monitoring:** Real-time metrics → Dashboard visualization

---

## System Components

### 1. Data Pipeline (`crates/market-data`)
- **Sources:** [Polygon.io](https://polygon.io/)
- **Storage:** DuckDB for analytics (planned)
- **Processing:**
  - Order book validation
  - Feature engineering (order imbalance)
  - `L2Source` trait at the I/O boundary

### 2. Machine Learning Core (`crates/ml-core`)
- **Model:** GCN + temporal transformer (weights not trained yet)
- **Constraints:** spread conservation, probability unit interval
- **Serving:** `predict` API behind the CLI

### 3. Execution Engine (`crates/execution`)
- **Broker:** Alpaca adapter (`Broker` trait). Paper by default. Fails closed without credentials.
- **Risk:** 1% of equity per trade, 5% daily loss circuit breaker
- **Router:** widen > 0.7 → buy; narrow > 0.7 → sell; else hold

### 4. Frontend Dashboard
**Tech:** TypeScript, WebSockets, CSS (unchanged)

### Visuals
- **Order Book Heatmap**
- **Prediction Probability Gauge**
- **Execution History**
- **P&L Performance**
- **Paper Trading Toggle & Manual Controls**

---

## Installation

### Prerequisites
- Rust 1.85+ (`rustup`)
- Node.js 18+ (frontend)
- GitHub Student Pack (recommended)

### Setup
```bash
git clone https://github.com/yourusername/neural-router.git
cd neural-router

cargo build --workspace

cd frontend
npm install
npm run build
```

---

## Configuration

Copy `.env.example` to `.env`. Do not commit `.env`.

```ini
POLYGON_API_KEY=your_polygon_api_key_here
ALPACA_API_KEY=your_alpaca_key_here
ALPACA_SECRET_KEY=your_alpaca_secret_here
ALPACA_PAPER=true
PREDICTION_HORIZON=500
ORDER_BOOK_LEVELS=10
RISK_LIMIT_PER_TRADE=0.01
MAX_DAILY_LOSS=0.05
SYMBOL=SPY
```

---

## Workflows

```bash
cargo run -- collect --symbol SPY
cargo run -- train --epochs 50 --batch-size 1024
cargo run -- predict
cargo run -- execute
cargo run -- backtest --start 2024-01-01 --end 2024-03-01
```

Frontend:

```bash
cd frontend
npm run dev
```

I/O adapters (Polygon ingest, GNN train/serve, Alpaca transport, historical replay) return `not implemented` until those layers are written. Domain, config, validation, risk, and routing already have unit tests.

---

## Testing & Validation

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

| Test Type      | Tools           | Frequency      |
| -------------- | -------------- | ------------- |
| Unit Tests     | cargo test     | Pre-commit    |
| Integration    | Docker Compose | Weekly        |
| Backtesting    | `backtest` bin | Per model ver |
| Paper Trading  | Alpaca Sandbox | Continuous    |

**Key Metrics:**
- Prediction Accuracy: >62%
- Shortfall: Benchmark reduction
- Toxic Flow Avoidance: High detection rate
- Sharpe Ratio: >1.5

---

## Deployment

### Infrastructure
- DigitalOcean droplet (~$40/mo)

### Production Setup
```bash
cargo build --release -p neural-router
```

Run the release binary with env vars injected at process start. Do not bake secrets into the binary.

### Frontend Deployment
```bash
cd frontend
vercel --prod
```

---

## Roadmap
### Phase 1: MVP
- Data ingestion pipeline
- GNN prototype
- Execution logic
- Paper trading
- Dashboard v1

### Phase 2: Scaling
- Multi-asset support (e.g., QQQ, BTC)
- RL-based decision module
- Enhanced risk checks

### Phase 3: Production
- FIX protocol support
- Regulatory compliance
- ASIC/FPGA acceleration

---

## Contributing
1. Fork the repo
2. Create a feature branch
3. Run `cargo test --workspace` and `cargo clippy --workspace`
4. Open a PR with description, validation, and expected impact

---

## License
This project is licensed under the Apache License 2.0.
