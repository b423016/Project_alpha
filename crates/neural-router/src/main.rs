use anyhow::Result;
use clap::{Parser, Subcommand};
use neural_router::http::{AppState, serve};
use neural_router_config::Settings;
use neural_router_execution::{AlpacaOverlay, LIVE_BASE, PAPER_BASE};
use neural_router_policy::ClaudeClient;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "neural-router",
    about = "Paper options overlay kernel",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Loopback HTTP: snapshot, top20, blotter, metrics, kill
    Serve,
    /// Check paper Alpaca + whether Claude key is present. Never prints secrets.
    Probe,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();
    let settings = Settings::from_env()?;
    match cli.command {
        Command::Serve => {
            let state = AppState::from_settings(&settings);
            tracing::info!(
                bind = %settings.bind,
                paper = settings.alpaca_paper,
                alpaca = state.broker.is_some(),
                claude = state.claude_configured,
                "overlay http"
            );
            serve(&settings.bind, state).await?;
        }
        Command::Probe => {
            probe(&settings)?;
        }
    }
    Ok(())
}

fn probe(settings: &Settings) -> Result<()> {
    let base = if settings.alpaca_paper {
        PAPER_BASE
    } else {
        LIVE_BASE
    };
    println!("paper={}", settings.alpaca_paper);
    println!("allow_live={}", settings.allow_live);
    println!("base={base}");
    println!("llm_strategist={}", settings.llm_strategist);
    println!("llm_quant={}", settings.llm_quant);
    match AlpacaOverlay::from_settings(settings) {
        Ok(b) => match b.account() {
            Ok(a) => {
                println!("alpaca=ok");
                println!("status={}", a.status);
                println!("equity={}", a.equity);
                println!("account={}", a.account_tail);
            }
            Err(e) => println!("alpaca_http={e}"),
        },
        Err(e) => println!("alpaca={e}"),
    }
    match ClaudeClient::from_settings(settings) {
        Ok(_) => println!("claude=configured"),
        Err(_) => println!("claude=missing"),
    }
    Ok(())
}
