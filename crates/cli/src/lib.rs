//! The `timemd` command model and the operations behind it.
//!
//! Commands talk to `timemd-core` directly rather than to a running server, so
//! an agent in a shell can log time whether or not the web app is up. The store
//! serialises its own writes, which is what makes that safe.
//!
//! Operations return the text to print rather than printing it, so they stay
//! testable without capturing stdout.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime};
use clap::{Parser, Subcommand};
use timemd_core::active::SessionKind;
use timemd_core::day::Session;
use timemd_core::report::{self, GroupBy};
use timemd_core::{
    Error, Minutes, Project, ProjectSlug, Result, StartRequest, Stopped, Store, Timer,
};

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
}

/// Runs everything except `serve`, which needs the async runtime the binary owns.
pub fn run(store: &Store, command: Command, now: NaiveDateTime) -> Result<String> {
    match command {
        // Handled by the binary; reaching here would be a wiring mistake.
        Command::Serve { .. } => Ok(String::new()),

        Command::Start {
            project,
            note,
            duration,
        } => {
            let request = StartRequest {
                kind: SessionKind::Focus,
                duration: duration.map(|raw| parse_duration(&raw)).transpose()?,
                project: project.map(|raw| parse_slug(&raw)).transpose()?,
                note,
            };
            let active = Timer::new(store).start(now, request)?;
            Ok(format!(
                "started {} → {}{}",
                active.started.format("%H:%M"),
                name_or_dash(active.project.as_ref()),
                suffix(&active.note),
            ))
        }

        Command::Stop => Ok(match Timer::new(store).stop(now)? {
            Stopped::Logged(session) => format!(
                "logged {} to {}",
                session.duration(),
                name_or_dash(session.project.as_ref()),
            ),
            Stopped::TooShort => "stopped — under a minute, so nothing was logged".to_owned(),
            Stopped::Idle => "nothing was running".to_owned(),
        }),

        Command::Cancel => Ok(if Timer::new(store).cancel()? {
            "discarded".to_owned()
        } else {
            "nothing was running".to_owned()
        }),

        Command::Status => {
            let state = Timer::new(store).state(now)?;
            Ok(match state.active {
                Some(active) => format!(
                    "{} on {} — {} left{}",
                    kind_name(active.kind),
                    name_or_dash(active.project.as_ref()),
                    active.remaining(now),
                    suffix(&active.note),
                ),
                None => format!(
                    "idle — {} tracked today across {} session(s)",
                    state.tracked_today, state.completed_today
                ),
            })
        }

        Command::Today { date } => {
            let date = date.unwrap_or_else(|| now.date());
            let day = store.read_day(date)?;

            let mut lines = vec![format!("{date} — {} tracked", day.total())];
            for session in day.sessions() {
                lines.push(format!(
                    "  {}-{} {:>7}  {}{}",
                    session.start.format("%H:%M"),
                    session.end.format("%H:%M"),
                    session.duration().to_string(),
                    name_or_dash(session.project.as_ref()),
                    suffix(&session.note),
                ));
            }
            for problem in day.problems() {
                lines.push(format!("  ! {problem}"));
            }
            Ok(lines.join("\n"))
        }

        Command::Log {
            project,
            from,
            to,
            note,
            date,
        } => {
            let date = date.unwrap_or_else(|| now.date());
            let session = Session::new(
                from,
                to,
                project.map(|raw| parse_slug(&raw)).transpose()?,
                note,
            );
            let logged = session.duration();
            store.update_day(date, |day| day.add_session(session))?;
            Ok(format!("logged {logged} on {date}"))
        }

        Command::Projects => {
            let projects = store.list_projects()?;
            if projects.is_empty() {
                return Ok("no projects yet".to_owned());
            }
            Ok(projects
                .iter()
                .map(|project| {
                    format!(
                        "{:<24} {}{}",
                        project.slug().as_str(),
                        project.name,
                        if project.status.is_archived() {
                            "  (archived)"
                        } else {
                            ""
                        },
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }

        Command::Report { from, to, group_by } => {
            let to = to.unwrap_or_else(|| now.date());
            let from = from.unwrap_or_else(|| to - chrono::TimeDelta::days(6));
            let grouping = parse_grouping(&group_by)?;

            let report = report::build(store, from, to, grouping)?;
            let mut lines = vec![format!("{from} → {to} — {} total", report.total)];
            for bucket in &report.buckets {
                lines.push(format!(
                    "  {:<24} {:>8}  {} session(s)",
                    bucket.key.as_deref().unwrap_or("-"),
                    bucket.tracked.to_string(),
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
    let timezone = store.read_settings()?.timezone;
    Ok(Local::now().with_timezone(&timezone).naive_local())
}

/// Creates a project file. Used by tests and available to callers that want to
/// seed a tree without the HTTP API.
pub fn create_project(store: &Store, slug: &str, name: &str, today: NaiveDate) -> Result<()> {
    store.create_project(&Project::new(parse_slug(slug)?, name, today))
}

fn kind_name(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Focus => "focus",
        SessionKind::ShortBreak => "short break",
        SessionKind::LongBreak => "long break",
    }
}

fn name_or_dash(project: Option<&ProjectSlug>) -> String {
    project.map_or_else(|| "-".to_owned(), ToString::to_string)
}

fn suffix(note: &str) -> String {
    if note.is_empty() {
        String::new()
    } else {
        format!("  {note}")
    }
}

fn parse_grouping(raw: &str) -> Result<GroupBy> {
    match raw {
        "project" => Ok(GroupBy::Project),
        "day" => Ok(GroupBy::Day),
        other => Err(Error::UnknownProject(format!(
            "unknown grouping {other:?}; expected `project` or `day`"
        ))),
    }
}

fn parse_slug(raw: &str) -> Result<ProjectSlug> {
    ProjectSlug::new(raw).map_err(|error| Error::UnknownProject(error.to_string()))
}

fn parse_duration(raw: &str) -> Result<Minutes> {
    raw.parse()
        .map_err(|error: timemd_core::ParseErrorKind| Error::UnknownProject(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        (directory, store)
    }

    fn moment(hour: u32, minute: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 1)
            .expect("valid date")
            .and_hms_opt(hour, minute, 0)
            .expect("valid time")
    }

    fn at(hour: u32, minute: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time")
    }

    fn start(project: Option<&str>, duration: Option<&str>) -> Command {
        Command::Start {
            project: project.map(ToOwned::to_owned),
            note: String::new(),
            duration: duration.map(ToOwned::to_owned),
        }
    }

    fn log(
        project: Option<&str>,
        from: NaiveTime,
        to: NaiveTime,
        date: Option<NaiveDate>,
    ) -> Command {
        Command::Log {
            project: project.map(ToOwned::to_owned),
            from,
            to,
            note: String::new(),
            date,
        }
    }

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
    fn start_then_stop_logs_the_time_worked() {
        let (_directory, store) = store();

        let started = run(
            &store,
            Command::Start {
                project: Some("timemd".to_owned()),
                note: "file store".to_owned(),
                duration: None,
            },
            moment(9, 0),
        )
        .expect("starts");
        assert!(started.contains("timemd"), "{started}");
        assert!(started.contains("file store"), "{started}");

        let stopped = run(&store, Command::Stop, moment(9, 10)).expect("stops");
        assert!(stopped.contains("10m"), "{stopped}");
        assert_eq!(
            store.read_day(moment(9, 0).date()).expect("reads").total(),
            Minutes::new(10)
        );
    }

    #[test]
    fn stopping_immediately_says_it_was_too_short_rather_than_idle() {
        let (_directory, store) = store();
        run(&store, start(Some("timemd"), None), moment(9, 0)).expect("starts");

        let stopped = run(&store, Command::Stop, moment(9, 0)).expect("stops");
        assert!(stopped.contains("under a minute"), "{stopped}");
        assert!(!stopped.contains("nothing was running"), "{stopped}");
    }

    #[test]
    fn stopping_or_cancelling_nothing_says_so() {
        let (_directory, store) = store();
        assert_eq!(
            run(&store, Command::Stop, moment(9, 0)).expect("stops"),
            "nothing was running"
        );
        assert_eq!(
            run(&store, Command::Cancel, moment(9, 0)).expect("cancels"),
            "nothing was running"
        );
    }

    #[test]
    fn cancel_discards_without_logging() {
        let (_directory, store) = store();
        run(&store, start(None, None), moment(9, 0)).expect("starts");

        assert_eq!(
            run(&store, Command::Cancel, moment(9, 10)).expect("cancels"),
            "discarded"
        );
        assert!(
            store
                .read_day(moment(9, 0).date())
                .expect("reads")
                .sessions()
                .is_empty()
        );
    }

    #[test]
    fn status_reports_running_and_idle() {
        let (_directory, store) = store();

        let idle = run(&store, Command::Status, moment(9, 0)).expect("reads");
        assert!(idle.starts_with("idle"), "{idle}");

        run(&store, start(Some("timemd"), Some("50m")), moment(9, 0)).expect("starts");

        let running = run(&store, Command::Status, moment(9, 20)).expect("reads");
        assert!(running.contains("focus on timemd"), "{running}");
        assert!(running.contains("30m left"), "{running}");
    }

    #[test]
    fn log_writes_a_session_without_the_timer() {
        let (_directory, store) = store();

        let logged = run(
            &store,
            Command::Log {
                project: Some("timemd".to_owned()),
                from: at(14, 0),
                to: at(15, 30),
                note: "meeting".to_owned(),
                date: None,
            },
            moment(16, 0),
        )
        .expect("logs");

        assert!(logged.contains("1h30m"), "{logged}");
        assert_eq!(
            store.read_day(moment(9, 0).date()).expect("reads").total(),
            Minutes::new(90)
        );
    }

    #[test]
    fn log_accepts_an_explicit_date() {
        let (_directory, store) = store();
        let earlier = NaiveDate::from_ymd_opt(2026, 7, 20).expect("valid date");

        run(
            &store,
            log(None, at(9, 0), at(10, 0), Some(earlier)),
            moment(16, 0),
        )
        .expect("logs");

        assert_eq!(
            store.read_day(earlier).expect("reads").total(),
            Minutes::new(60)
        );
    }

    #[test]
    fn today_lists_sessions_and_the_total() {
        let (_directory, store) = store();
        run(
            &store,
            Command::Log {
                project: Some("timemd".to_owned()),
                from: at(9, 0),
                to: at(9, 25),
                note: "work".to_owned(),
                date: None,
            },
            moment(10, 0),
        )
        .expect("logs");

        let output = run(&store, Command::Today { date: None }, moment(10, 0)).expect("reads");
        assert!(output.contains("25m tracked"), "{output}");
        assert!(output.contains("09:00-09:25"), "{output}");
        assert!(output.contains("timemd"), "{output}");
    }

    #[test]
    fn today_surfaces_lines_it_could_not_read() {
        let (_directory, store) = store();
        let path = store.day_path(moment(9, 0).date());
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates dir");
        std::fs::write(
            &path,
            "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- nonsense\n",
        )
        .expect("writes");

        let output = run(&store, Command::Today { date: None }, moment(10, 0)).expect("reads");
        assert!(output.contains("  ! "), "{output}");
    }

    #[test]
    fn projects_lists_names_and_archived_state() {
        let (_directory, store) = store();
        let today = moment(9, 0).date();
        create_project(&store, "timemd", "timemd", today).expect("creates");
        create_project(&store, "admin", "Admin", today).expect("creates");
        store
            .update_project(&parse_slug("admin").expect("valid"), |project| {
                project.status = timemd_core::ProjectStatus::Archived;
            })
            .expect("updates");

        let output = run(&store, Command::Projects, moment(9, 0)).expect("reads");
        assert!(output.contains("timemd"), "{output}");
        assert!(output.contains("(archived)"), "{output}");
    }

    #[test]
    fn projects_says_so_when_there_are_none() {
        let (_directory, store) = store();
        assert_eq!(
            run(&store, Command::Projects, moment(9, 0)).expect("reads"),
            "no projects yet"
        );
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
        assert!(output.contains("timemd"), "{output}");
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
    fn an_invalid_slug_or_duration_is_rejected() {
        let (_directory, store) = store();

        assert!(run(&store, start(Some("Not A Slug"), None), moment(9, 0)).is_err());
        assert!(run(&store, start(None, Some("ages")), moment(9, 0)).is_err());
        assert!(
            run(
                &store,
                log(Some("Nope!"), at(9, 0), at(10, 0), None),
                moment(9, 0)
            )
            .is_err()
        );
    }

    #[test]
    fn breaks_are_named_in_full() {
        assert_eq!(kind_name(SessionKind::Focus), "focus");
        assert_eq!(kind_name(SessionKind::ShortBreak), "short break");
        assert_eq!(kind_name(SessionKind::LongBreak), "long break");
    }

    #[test]
    fn serve_produces_no_output_here() {
        let (_directory, store) = store();
        let addr = "127.0.0.1:8080".parse().expect("valid address");
        assert_eq!(
            run(&store, Command::Serve { addr }, moment(9, 0)).expect("runs"),
            ""
        );
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
