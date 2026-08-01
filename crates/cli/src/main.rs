//! `timemd` — one binary for the whole system.
//!
//! `serve` runs the web app; later milestones add `mcp` (stdio Model Context
//! Protocol) alongside the direct operations agents and humans reach for from a
//! shell. Keeping them in a single artifact is what makes the Tailscale deploy
//! story "copy one file".

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use timemd::{Cli, Command};
use timemd_core::Store;
use timemd_server::state::{AppState, Clock};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("TIMEMD_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> std::io::Result<()> {
    let state = AppState::new(Arc::new(Store::new(&cli.data)), Clock::System);

    match cli.command {
        Command::Serve { addr } => {
            let listener = TcpListener::bind(addr).await?;
            tracing::info!(addr = %listener.local_addr()?, data = ?cli.data, "timemd listening");
            timemd_server::serve(listener, state).await
        }
    }
}
