//! `timemd` — one binary for the whole system.
//!
//! `serve` runs the web app; later milestones add `mcp` (stdio Model Context
//! Protocol) alongside the direct operations agents and humans reach for from a
//! shell. Keeping them in a single artifact is what makes the Tailscale deploy
//! story "copy one file".

use std::net::SocketAddr;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "timemd", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the web app and JSON API.
    Serve {
        /// Address to bind. Defaults to every interface, which is safe only
        /// because access is expected to be gated by Tailscale or a LAN.
        #[arg(long, env = "TIMEMD_ADDR", default_value = "0.0.0.0:8080")]
        addr: SocketAddr,
    },
}

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
    match cli.command {
        Command::Serve { addr } => {
            let listener = TcpListener::bind(addr).await?;
            tracing::info!(addr = %listener.local_addr()?, "timemd listening");
            timemd_server::serve(listener).await
        }
    }
}
