//! The `timemd` command model.
//!
//! Kept apart from `main.rs` so the argument surface — the part with rules worth
//! checking — is a library that tests can parse against, leaving the binary a
//! shim that only wires logging, dispatch and an exit code together.

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "timemd", version, about, long_about = None)]
pub struct Cli {
    /// Root of the markdown data tree.
    #[arg(long, env = "TIMEMD_DATA", default_value = "./data", global = true)]
    pub data: PathBuf,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run the web app and JSON API.
    Serve {
        /// Address to bind. Defaults to every interface, which is safe only
        /// because access is expected to be gated by Tailscale or a LAN.
        #[arg(long, env = "TIMEMD_ADDR", default_value = "0.0.0.0:8080")]
        addr: SocketAddr,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_model_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn serve_defaults_to_every_interface_on_8080() {
        let cli = Cli::try_parse_from(["timemd", "serve"]).expect("parses");
        let Command::Serve { addr } = cli.command;
        assert_eq!(addr.to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn serve_accepts_an_explicit_address() {
        let cli =
            Cli::try_parse_from(["timemd", "serve", "--addr", "127.0.0.1:9000"]).expect("parses");
        let Command::Serve { addr } = cli.command;
        assert_eq!(addr.to_string(), "127.0.0.1:9000");
    }

    #[test]
    fn rejects_an_unparseable_address() {
        assert!(Cli::try_parse_from(["timemd", "serve", "--addr", "not-an-address"]).is_err());
    }

    #[test]
    fn the_data_root_defaults_and_can_be_overridden() {
        let cli = Cli::try_parse_from(["timemd", "serve"]).expect("parses");
        assert_eq!(cli.data, std::path::Path::new("./data"));

        let cli =
            Cli::try_parse_from(["timemd", "--data", "/srv/timemd", "serve"]).expect("parses");
        assert_eq!(cli.data, std::path::Path::new("/srv/timemd"));
    }

    #[test]
    fn requires_a_subcommand() {
        assert!(Cli::try_parse_from(["timemd"]).is_err());
    }
}
