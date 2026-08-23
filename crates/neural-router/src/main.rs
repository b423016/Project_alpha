use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use neural_router_config::Settings;
use neural_router_data::collect;
use neural_router_execution::{AlpacaClient, RiskManager};
use neural_router_ml::{NeuralOrderBookModel, TrainConfig, train};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "neural-router",
    about = "Neural order book execution router",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Pull L2 history/stream into data/
    Collect {
        #[arg(long)]
        symbol: Option<String>,
    },
    /// Train the GNN-transformer
    Train {
        #[arg(long, default_value_t = 50)]
        epochs: u32,
        #[arg(long, default_value_t = 1024)]
        batch_size: usize,
    },
    /// Serve snapshot inference
    Predict,
    /// Risk-gate signals and route to the broker
    Execute,
    /// Replay a model over historical books
    Backtest {
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        end: Option<String>,
    },
}

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let settings = Settings::from_env()?;

    match cli.command {
        Command::Collect { symbol } => {
            let symbol = symbol.unwrap_or_else(|| settings.symbol.clone());
            collect(&settings, &symbol)?;
        }
        Command::Train { epochs, batch_size } => {
            let mut model = NeuralOrderBookModel::from_settings(&settings);
            train(&mut model, TrainConfig { epochs, batch_size })?;
        }
        Command::Predict => {
            bail!("not implemented: gnn_transformer_inference");
        }
        Command::Execute => {
            let _broker = AlpacaClient::from_settings(&settings)?;
            let _risk = RiskManager::from_settings(&settings);
            bail!("not implemented: live execution loop");
        }
        Command::Backtest { start, end } => {
            tracing::info!(?start, ?end, "backtest");
            bail!("not implemented: historical replay");
        }
    }

    Ok(())
}
