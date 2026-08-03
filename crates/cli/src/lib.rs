//! The `timemd` command model and the operations behind it.
//!
//! Commands talk to `timemd-core` directly rather than to a running server, so
//! an agent in a shell can log time whether or not the web app is up. The store
//! serialises its own writes, which is what makes that safe.
//!
//! Operations return the text to print rather than printing it, so they stay
//! testable without capturing stdout.
//!
//! This file owns the command model and the dispatch; the operations behind
//! each group live beside it, one module per thing being operated on.

pub mod ntfy;
pub mod project;
pub mod schedule;
pub mod session;
pub mod settings;
pub mod timer;

#[cfg(test)]
pub(crate) mod testing;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
use clap::{Parser, Subcommand};
use timemd_core::report::{self, GroupBy};
use timemd_core::{DateRange, Error, Project, ProjectSlug, Result, Store};

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

    /// Speak the Model Context Protocol on stdio, for agents.
    Mcp,

    /// Start a focus session.
    Start {
        /// Project slug to track against.
        project: Option<String>,
        /// What you are working on.
        #[arg(short, long, default_value = "")]
        note: String,
        /// Length, e.g. `25m` or `1h30m`. Defaults to the configured focus length.
        #[arg(short, long)]
        duration: Option<String>,
    },

    /// Stop the running session, logging the time worked.
    Stop,

    /// Discard the running session without logging it.
    Cancel,

    /// Show what is running, if anything.
    Status,

    /// Show a day's sessions and total. Defaults to today.
    Today {
        #[arg(long)]
        date: Option<NaiveDate>,
    },

    /// Log time that was not tracked live.
    Log {
        /// Project slug.
        project: Option<String>,
        /// Start time, `HH:MM`.
        #[arg(long)]
        from: NaiveTime,
        /// End time, `HH:MM`. Earlier than `from` means it crossed midnight.
        #[arg(long)]
        to: NaiveTime,
        #[arg(short, long, default_value = "")]
        note: String,
        #[arg(long)]
        date: Option<NaiveDate>,
    },

    /// List projects.
    Projects,

    /// Total time over a range. Defaults to the last seven days.
    Report {
        #[arg(long)]
        from: Option<NaiveDate>,
        #[arg(long)]
        to: Option<NaiveDate>,
        /// `project` or `day`.
        #[arg(long, default_value = "project")]
        group_by: String,
    },

    // The verbs above are one word because they are typed constantly. The
    // groups below are nouns operated on rarely, and flattening them would put
    // seventeen more entries in one `--help` list.
    /// Read, create, change and delete projects.
    Project {
        #[command(subcommand)]
        operation: project::ProjectCommand,
    },

    /// Add, tick, retitle, reorder and delete a project's milestones.
    Milestone {
        #[command(subcommand)]
        operation: project::MilestoneCommand,
    },

    /// Amend or remove time already logged.
    Session {
        #[command(subcommand)]
        operation: session::SessionCommand,
    },

    /// Plan, amend and remove one-off blocks on a day.
    Block {
        #[command(subcommand)]
        operation: schedule::BlockCommand,
    },

    /// The weekly repeating schedule, and skipping it for a day.
    Repeat {
        #[command(subcommand)]
        operation: schedule::RepeatCommand,
    },

    /// What is planned over a range. Defaults to today.
    Schedule {
        #[arg(long)]
        from: Option<NaiveDate>,
        #[arg(long)]
        to: Option<NaiveDate>,
    },

    /// Pomodoro lengths and the reminder default. With no flags, prints them.
    Settings {
        #[arg(long)]
        focus: Option<String>,
        #[arg(long)]
        short_break: Option<String>,
        #[arg(long)]
        long_break: Option<String>,
        #[arg(long)]
        remind_before: Option<String>,
    },

    /// Where notifications go on a phone. With no flags, prints the config.
    ///
    /// A flag given empty clears it: `--topic ''` turns the channel off.
    Ntfy {
        /// Base URL of the ntfy server. Defaults to https://ntfy.sh.
        #[arg(long)]
        server: Option<String>,
        /// Topic to publish to. Anyone who knows it can read your
        /// notifications, so pick one nobody would guess.
        #[arg(long)]
        topic: Option<String>,
        /// Bearer token, for a topic that is access-controlled.
        #[arg(long)]
        token: Option<String>,
        /// Where this app answers from outside, so a notification is tappable.
        #[arg(long)]
        app_url: Option<String>,
    },
}

/// Runs everything except `serve`, which needs the async runtime the binary owns.
pub fn run(store: &Store, command: Command, now: NaiveDateTime) -> Result<String> {
    match command {
        // Both are handled by the binary, which owns the async runtime they
        // need; reaching here would be a wiring mistake.
        Command::Serve { .. } | Command::Mcp => Ok(String::new()),

        Command::Start {
            project,
            note,
            duration,
        } => timer::start(store, project, note, duration, now),

        Command::Stop => timer::stop(store, now),

        Command::Cancel => timer::cancel(store),

        Command::Status => timer::status(store, now),

        Command::Today { date } => session::today(store, date, now),

        Command::Log {
            project,
            from,
            to,
            note,
            date,
        } => session::log(store, project, from, to, note, date, now),

        Command::Projects => project::list(store),

        Command::Project { operation } => project::run(store, operation, now.date()),

        Command::Milestone { operation } => project::milestone(store, operation),

        Command::Session { operation } => session::run(store, operation, now),

        Command::Block { operation } => schedule::block(store, operation, now),

        Command::Repeat { operation } => schedule::repeat(store, operation, now),

        Command::Schedule { from, to } => schedule::range(store, from, to, now),

        Command::Settings {
            focus,
            short_break,
            long_break,
            remind_before,
        } => settings::run(store, focus, short_break, long_break, remind_before),

        Command::Ntfy {
            server,
            topic,
            token,
            app_url,
        } => ntfy::run(store, server, topic, token, app_url),

        Command::Report { from, to, group_by } => {
            let to = to.unwrap_or_else(|| now.date());
            let from = from.unwrap_or_else(|| to - chrono::TimeDelta::days(6));
            // Core validates the span, so the CLI inherits the same bound the
            // HTTP API and the MCP server enforce.
            let range = DateRange::new(from, to)?;

            let report = report::build(store, range, group_by.parse::<GroupBy>()?)?;
            let mut lines = vec![format!(
                "{from} → {to} — {} total · {} planned",
                report.total, report.planned
            )];
            for bucket in &report.buckets {
                // `to_string` first on both: `Minutes` writes through `write!`, so
                // a width specifier does nothing to the value itself.
                lines.push(format!(
                    "  {:<24} {:>8} {:>8}  {} session(s)",
                    bucket.key.as_deref().unwrap_or("-"),
                    bucket.tracked.to_string(),
                    bucket.planned.to_string(),
                    bucket.sessions,
                ));
            }
            Ok(lines.join("\n"))
        }
    }
}

/// Opens the markdown tree at `root`.
pub fn open(root: &Path) -> Store {
    Store::new(root)
}

/// Now, as wall-clock time in the configured timezone.
pub fn local_now(store: &Store) -> Result<NaiveDateTime> {
    store.wall_clock(Utc::now())
}

/// Creates a project file. Used by tests and available to callers that want to
/// seed a tree without the HTTP API.
pub fn create_project(store: &Store, slug: &str, name: &str, today: NaiveDate) -> Result<()> {
    store.create_project(&Project::new(ProjectSlug::new(slug)?, name, today))
}

/// Reads a flag that may also be given empty to mean "clear it".
///
/// The outer `Option` is "was the flag passed", the inner one is the value, so
/// leaving a tag alone and removing it stay distinguishable — the same
/// distinction the HTTP API draws with a doubly-optional field.
///
/// One helper rather than one per flag because that distinction is the whole
/// rule, and `crates/server/src/parse.rs` records what happened last time it
/// was written out per caller: the copies had already disagreed about whether
/// an empty string means "absent" or "clear".
pub(crate) fn clearable<T, E: Into<Error>>(
    raw: Option<String>,
    parse: impl FnOnce(String) -> std::result::Result<T, E>,
) -> Result<Option<Option<T>>> {
    match raw {
        None => Ok(None),
        Some(value) if value.is_empty() => Ok(Some(None)),
        Some(value) => parse(value)
            .map(|parsed| Some(Some(parsed)))
            .map_err(Into::into),
    }
}

pub(crate) fn optional_slug(raw: Option<String>) -> Result<Option<Option<ProjectSlug>>> {
    clearable(raw, ProjectSlug::new)
}

pub(crate) fn name_or_dash(project: Option<&ProjectSlug>) -> String {
    project.map_or_else(|| "-".to_owned(), ToString::to_string)
}

pub(crate) fn suffix(note: &str) -> String {
    if note.is_empty() {
        String::new()
    } else {
        format!("  {note}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{at, log, moment, store};
    use clap::CommandFactory;

    #[test]
    fn the_command_model_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn serve_defaults_to_every_interface_on_8080() {
        let cli = Cli::try_parse_from(["timemd", "serve"]).expect("parses");
        let Command::Serve { addr } = cli.command else {
            panic!("expected serve");
        };
        assert_eq!(addr.to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn the_data_root_defaults_and_can_be_overridden() {
        let cli = Cli::try_parse_from(["timemd", "status"]).expect("parses");
        assert_eq!(cli.data, Path::new("./data"));

        let cli =
            Cli::try_parse_from(["timemd", "--data", "/srv/timemd", "status"]).expect("parses");
        assert_eq!(cli.data, Path::new("/srv/timemd"));
    }

    #[test]
    fn rejects_unparseable_arguments() {
        assert!(Cli::try_parse_from(["timemd"]).is_err());
        assert!(Cli::try_parse_from(["timemd", "serve", "--addr", "nope"]).is_err());
        assert!(Cli::try_parse_from(["timemd", "log", "--from", "nope", "--to", "10:00"]).is_err());
    }

    #[test]
    fn accepts_the_shorthand_flags() {
        let cli = Cli::try_parse_from(["timemd", "start", "timemd", "-n", "work", "-d", "50m"])
            .expect("parses");
        let Command::Start {
            project,
            note,
            duration,
        } = cli.command
        else {
            panic!("expected start");
        };
        assert_eq!(project.as_deref(), Some("timemd"));
        assert_eq!(note, "work");
        assert_eq!(duration.as_deref(), Some("50m"));
    }

    #[test]
    fn report_defaults_to_the_last_seven_days() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(11, 0), None),
            moment(12, 0),
        )
        .expect("logs");

        let output = run(
            &store,
            Command::Report {
                from: None,
                to: None,
                group_by: "project".to_owned(),
            },
            moment(12, 0),
        )
        .expect("reads");

        assert!(output.contains("2h total"), "{output}");
        assert!(output.contains("0m planned"), "{output}");
        assert!(output.contains("timemd"), "{output}");
    }

    #[test]
    fn report_prints_what_was_planned_beside_what_was_tracked() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(10, 0), None),
            moment(12, 0),
        )
        .expect("logs");
        // The CLI has no schedule command, so the plan is seeded through the store.
        let block =
            timemd_core::DayBlock::parse("09:00-11:00 [[timemd]] Deep work").expect("parses");
        store
            .update_day(moment(9, 0).date(), |day| day.add_block(block))
            .expect("writes");

        let output = run(
            &store,
            Command::Report {
                from: None,
                to: None,
                group_by: "project".to_owned(),
            },
            moment(12, 0),
        )
        .expect("reads");

        assert!(output.contains("1h total · 2h planned"), "{output}");
        assert!(
            output
                .lines()
                .any(|line| line.contains("timemd") && line.contains("1h") && line.contains("2h")),
            "{output}"
        );
    }

    #[test]
    fn report_can_group_by_day_and_labels_untracked_time() {
        let (_directory, store) = store();
        run(&store, log(None, at(9, 0), at(10, 0), None), moment(12, 0)).expect("logs");

        let by_day = run(
            &store,
            Command::Report {
                from: None,
                to: None,
                group_by: "day".to_owned(),
            },
            moment(12, 0),
        )
        .expect("reads");
        assert!(by_day.contains("2026-08-01"), "{by_day}");

        let by_project = run(
            &store,
            Command::Report {
                from: None,
                to: None,
                group_by: "project".to_owned(),
            },
            moment(12, 0),
        )
        .expect("reads");
        assert!(by_project.contains("  -  "), "{by_project}");
    }

    #[test]
    fn an_unknown_grouping_is_an_error() {
        let (_directory, store) = store();
        assert!(
            run(
                &store,
                Command::Report {
                    from: None,
                    to: None,
                    group_by: "colour".to_owned(),
                },
                moment(12, 0),
            )
            .is_err()
        );
    }

    #[test]
    fn the_binary_owned_commands_produce_no_output_here() {
        let (_directory, store) = store();
        let addr = "127.0.0.1:8080".parse().expect("valid address");
        assert_eq!(
            run(&store, Command::Serve { addr }, moment(9, 0)).expect("runs"),
            ""
        );
        assert_eq!(run(&store, Command::Mcp, moment(9, 0)).expect("runs"), "");
    }

    #[test]
    fn local_now_reads_the_configured_timezone() {
        let (_directory, store) = store();
        store
            .update_settings(|settings| settings.timezone = chrono_tz::UTC)
            .expect("writes");

        assert!(local_now(&store).is_ok());
    }

    #[test]
    fn open_builds_a_store_at_the_given_root() {
        let directory = tempfile::tempdir().expect("temp dir");
        assert_eq!(open(directory.path()).root(), directory.path());
    }
}
