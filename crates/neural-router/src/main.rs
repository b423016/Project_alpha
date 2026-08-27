use anyhow::Result;
use clap::{Parser, Subcommand};
use neural_router::http::{AppState, serve};
use neural_router_config::Settings;
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
            let state = AppState::from_fixture();
            tracing::info!(bind = %settings.bind, "overlay http");
            serve(&settings.bind, state).await?;
        }
    }
    Ok(())
}
