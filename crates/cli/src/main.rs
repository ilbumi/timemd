//! `timemd` — one binary for the whole system.
//!
//! `serve` runs the web app; the other subcommands drive the markdown tree
//! directly, so an agent in a shell can log time whether or not the server is
//! up. Keeping them in a single artifact is what makes the Tailscale deploy
//! story "copy one file".

use std::process::ExitCode;
use std::sync::Arc;

use clap::Parser;
use timemd::{Cli, Command};
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
        Err(message) => {
            eprintln!("timemd: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let store = Arc::new(timemd::open(&cli.data));

    match cli.command {
        Command::Serve { addr } => {
            let listener = TcpListener::bind(addr)
                .await
                .map_err(|error| error.to_string())?;
            let bound = listener.local_addr().map_err(|error| error.to_string())?;
            tracing::info!(addr = %bound, data = ?cli.data, "timemd listening");

            let state = AppState::new(store, Clock::System);
            timemd_server::serve(listener, state)
                .await
                .map_err(|error| error.to_string())
        }
        other => {
            let now = timemd::local_now(&store).map_err(|error| error.to_string())?;
            let output = timemd::run(&store, other, now).map_err(|error| error.to_string())?;
            println!("{output}");
            Ok(())
        }
    }
}
