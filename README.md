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

### Key Data Flows

- **Market Data Ingestion:** Real-time L2 data via WebSocket
- **Prediction Pipeline:** Order book → Feature extraction → Model inference
- **Execution Loop:** Signal → Risk check → Order routing → Confirmation
- **Monitoring:** Real-time metrics → Dashboard visualization

---

## System Components

### 1. Data Pipeline
- **Sources:** [Polygon.io](https://polygon.io/)
- **Storage:** 
  - DuckDB for analytics
  - MongoDB Atlas for operations
- **Processing:**
  - Order book normalization
  - Feature engineering (order imbalance, spread volatility)
  - Data validation

### 2. Machine Learning Core
- **Model:**
  - 3-layer Graph Convolutional Network (GCN)
  - Transformer temporal encoder
  - Physics-informed constraints
- **Training:** Kaggle GPU notebooks
- **Serving:** FastAPI prediction service

### 3. Execution Engine
- **Components:**
  - Signal generator
  - Risk manager (exposure & position limits)
  - Order router (Alpaca API)
- **Decision Logic:**
  ```python
  if spread_widening_prob > 0.7: 
      execute_market_order('BUY', quantity=risk_adjusted_size)
  ```

### 4. Frontend Dashboard
**Tech:** TypeScript, WebSockets, CSS

### Visuals
- **Order Book Heatmap**
- **Prediction Probability Gauge**
- **Execution History**
- **P&L Performance**
- **Paper Trading Toggle & Manual Controls**

---

## Installation
### Prerequisites
- Python 3.10+
- Node.js 18+
- DuckDB
- MongoDB Atlas account
- GitHub Student Pack (recommended)

### Setup
```bash
# Clone repository
git clone https://github.com/yourusername/neural-router.git
cd neural-router

# Create Python environment
python -m venv .venv
# Linux/Mac
source .venv/bin/activate
# Windows
.venv\Scripts\activate

# Install Python dependencies
pip install -r requirements.txt

# Frontend setup
cd frontend
npm install
npm run build
```

---

## Configuration
Create a `.env` file in the root directory:
```ini
POLYGON_API_KEY="your_polygon_api_key"
ALPACA_API_KEY="your_alpaca_key"
ALPACA_SECRET_KEY="your_alpaca_secret"
ALPACA_PAPER=true
MONGO_URI="mongodb+srv://user:password@cluster.mongodb.net/db"
PREDICTION_HORIZON=500
ORDER_BOOK_LEVELS=10
RISK_LIMIT_PER_TRADE=0.01
```

Update `config.py` with paths:
```python
DATA_DIR = "data/"
MODEL_DIR = "ml_core/models/"
LOG_DIR = "logs/"
```

---

## Workflows
### Data Collection
```bash
python scripts/data_collector.py --symbol SPY --frequency 100ms
```

### Model Training
```bash
python ml_core/train.py --epochs 50 --batch_size 1024 --use_gpu
```

### Start System
```bash
# Terminal 1
python ml_core/prediction_service.py

# Terminal 2
python execution_engine/main.py

# Terminal 3
cd frontend
npm run dev
```

### Backtesting
```bash
python scripts/backtest.py --start_date 2024-01-01 --end_date 2024-03-01
```

---

## Testing & Validation
| Test Type      | Tools           | Frequency      |
| -------------- | -------------- | ------------- |
| Unit Tests     | pytest         | Pre-commit    |
| Integration    | Docker Compose | Weekly        |
| Backtesting    | VectorBT       | Per model ver |
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
- MongoDB Atlas (M0 free tier)

### Production Setup
```bash
pm2 start ml_core/prediction_service.py --name prediction
pm2 start execution_engine/main.py --name execution
```

### Frontend Deployment
```bash
cd frontend
vercel --prod
```

### Monitoring Tools
- Datadog for metrics
- Sentry for error logging

---

## Roadmap
### Phase 1: MVP (Weeks 1–8)
- Data ingestion pipeline
- GNN prototype
- Execution logic
- Paper trading
- Dashboard v1

### Phase 2: Scaling (Weeks 9–16)
- Multi-asset support (e.g., QQQ, BTC)
- RL-based decision module
- Enhanced risk checks
- Compliance-ready reports
- Tier 4 client onboarding

### Phase 3: Production (Months 5–8)
- FIX protocol support
- Tier 1–2 integration
- Regulatory compliance (e.g., MiFID II)
- ASIC/FPGA acceleration

---

## Contributing
1. **Fork** the repo
2. **Create a feature branch:**
   ```bash
   git checkout -b feature/your-feature
   ```
3. **Commit with semantic messages:**
   ```bash
   git commit -m "feat: add new prediction module"
   ```
4. **Push and open a PR** with:
   - Description of change
   - Validation results
   - Expected impact

---

## License
This project is licensed under the Apache License 2.0.

**Contact:** For enterprise or institutional inquiries, please reach out via GitHub or submit an issue.

---

*Let me know if you'd like a version split into multiple markdown files (for a docs site) or auto-generated from code/docstrings.*







