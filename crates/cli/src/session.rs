//! `today`, `log` and the `session` group — time that has already been spent.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use clap::Subcommand;
use timemd_core::day::Session;
use timemd_core::error::Error;
use timemd_core::grammar::format_time;
use timemd_core::{ProjectSlug, Result, Store};

use crate::{name_or_dash, optional_slug, suffix};

#[derive(Subcommand, Debug)]
pub enum SessionCommand {
    /// Amend a logged session. Only the fields given change.
    Edit {
        /// Position in the day, as `timemd today` prints it.
        index: usize,
        #[arg(long)]
        date: Option<NaiveDate>,
        /// New start time, `HH:MM`.
        #[arg(long)]
        from: Option<NaiveTime>,
        /// New end time, `HH:MM`.
        #[arg(long)]
        to: Option<NaiveTime>,
        /// Project slug. Pass an empty string to clear it.
        #[arg(long)]
        project: Option<String>,
        #[arg(short, long)]
        note: Option<String>,
    },

    /// Delete a logged session.
    Rm {
        /// Position in the day, as `timemd today` prints it.
        index: usize,
        #[arg(long)]
        date: Option<NaiveDate>,
    },
}

pub fn run(store: &Store, command: SessionCommand, now: NaiveDateTime) -> Result<String> {
    match command {
        SessionCommand::Edit {
            index,
            date,
            from,
            to,
            project,
            note,
        } => {
            let date = date.unwrap_or_else(|| now.date());
            let project = optional_slug(project)?;

            store.try_update_day(date, |day| {
                let existing = day
                    .sessions()
                    .get(index)
                    .cloned()
                    .ok_or_else(|| missing(index, date))?;

                day.replace_session(
                    index,
                    Session::new(
                        from.unwrap_or(existing.start),
                        to.unwrap_or(existing.end),
                        project.unwrap_or(existing.project),
                        note.unwrap_or(existing.note),
                    ),
                );
                Ok::<_, Error>(())
            })??;

            // The day is re-sorted by the edit, so the listing is reprinted:
            // the index the next command needs may not be the one just used.
            today(store, Some(date), now)
        }

        SessionCommand::Rm { index, date } => {
            let date = date.unwrap_or_else(|| now.date());
            store.try_update_day(date, |day| {
                day.remove_session(index)
                    .map(|_| ())
                    .ok_or_else(|| missing(index, date))
            })??;
            today(store, Some(date), now)
        }
    }
}

fn missing(index: usize, date: NaiveDate) -> Error {
    Error::Invalid(format!("no session at index {index} on {date}"))
}

pub fn today(store: &Store, date: Option<NaiveDate>, now: NaiveDateTime) -> Result<String> {
    let date = date.unwrap_or_else(|| now.date());
    let day = store.read_day(date)?;

    let mut lines = vec![format!("{date} — {} tracked", day.total())];
    // The index is printed because it is the handle `session edit` and
    // `session rm` take, and there is nowhere else to read it from.
    for (index, session) in day.sessions().iter().enumerate() {
        lines.push(format!(
            "  {index}  {}-{} {:>7}  {}{}",
            format_time(session.start),
            format_time(session.end),
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

pub fn log(
    store: &Store,
    project: Option<String>,
    from: NaiveTime,
    to: NaiveTime,
    note: String,
    date: Option<NaiveDate>,
    now: NaiveDateTime,
) -> Result<String> {
    let date = date.unwrap_or_else(|| now.date());
    let session = Session::new(from, to, project.map(ProjectSlug::new).transpose()?, note);
    let logged = session.duration();
    store.update_day(date, |day| day.add_session(session))?;
    Ok(format!("logged {logged} on {date}"))
}

#[cfg(test)]
mod tests {
    use super::SessionCommand;
    use crate::testing::{at, log, moment, store};
    use crate::{Command, run};
    use chrono::NaiveDate;
    use clap::Parser;
    use timemd_core::Minutes;

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
    fn log_rejects_an_invalid_slug() {
        let (_directory, store) = store();
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

    fn session(operation: SessionCommand) -> Command {
        Command::Session { operation }
    }

    /// The index printed by `today` is the handle these commands take, so it
    /// has to be there and it has to address what it says it does.
    #[test]
    fn today_numbers_its_sessions() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(9, 25), None),
            moment(10, 0),
        )
        .expect("logs");
        run(
            &store,
            log(None, at(10, 30), at(10, 45), None),
            moment(11, 0),
        )
        .expect("logs");

        let output = run(&store, Command::Today { date: None }, moment(12, 0)).expect("reads");
        assert!(output.contains("  0  09:00-09:25"), "{output}");
        assert!(output.contains("  1  10:30-10:45"), "{output}");
    }

    #[test]
    fn a_logged_session_is_amended_field_by_field() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(9, 25), None),
            moment(10, 0),
        )
        .expect("logs");

        let output = run(
            &store,
            session(SessionCommand::Edit {
                index: 0,
                date: None,
                from: None,
                to: Some(at(10, 0)),
                project: None,
                note: Some("rewritten".to_owned()),
            }),
            moment(12, 0),
        )
        .expect("edits");

        assert!(output.contains("1h tracked"), "{output}");
        assert!(output.contains("09:00-10:00"), "{output}");
        assert!(output.contains("rewritten"), "{output}");
        assert!(
            output.contains("timemd"),
            "omitted fields stay put: {output}"
        );
    }

    /// There is no other way to say "no project" that an omitted flag does not
    /// already mean.
    #[test]
    fn an_empty_project_clears_a_session_tag() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(9, 25), None),
            moment(10, 0),
        )
        .expect("logs");

        let output = run(
            &store,
            session(SessionCommand::Edit {
                index: 0,
                date: None,
                from: None,
                to: None,
                project: Some(String::new()),
                note: None,
            }),
            moment(12, 0),
        )
        .expect("edits");
        assert!(!output.contains("timemd"), "{output}");
        assert!(output.contains("  0  09:00-09:25     25m  -"), "{output}");
    }

    /// Moving a start time re-sorts the day, so the listing is reprinted: the
    /// index the next command needs is not the one just used.
    #[test]
    fn editing_a_session_reprints_the_re_sorted_day() {
        let (_directory, store) = store();
        run(
            &store,
            log(Some("timemd"), at(9, 0), at(9, 25), None),
            moment(10, 0),
        )
        .expect("logs");
        run(
            &store,
            log(None, at(10, 30), at(10, 45), None),
            moment(11, 0),
        )
        .expect("logs");

        let output = run(
            &store,
            session(SessionCommand::Edit {
                index: 0,
                date: None,
                from: Some(at(14, 0)),
                to: Some(at(14, 30)),
                project: None,
                note: None,
            }),
            moment(15, 0),
        )
        .expect("edits");

        assert!(output.contains("  0  10:30-10:45"), "{output}");
        assert!(output.contains("  1  14:00-14:30"), "{output}");
    }

    #[test]
    fn a_logged_session_is_removed() {
        let (_directory, store) = store();
        run(&store, log(None, at(9, 0), at(9, 25), None), moment(10, 0)).expect("logs");

        let output = run(
            &store,
            session(SessionCommand::Rm {
                index: 0,
                date: None,
            }),
            moment(12, 0),
        )
        .expect("removes");
        assert!(output.contains("0m tracked"), "{output}");
    }

    #[test]
    fn addressing_a_session_that_is_not_there_is_an_error() {
        let (_directory, store) = store();
        run(&store, log(None, at(9, 0), at(9, 25), None), moment(10, 0)).expect("logs");

        for operation in [
            SessionCommand::Rm {
                index: 7,
                date: None,
            },
            SessionCommand::Edit {
                index: 7,
                date: None,
                from: None,
                to: None,
                project: None,
                note: Some("nowhere".to_owned()),
            },
        ] {
            assert!(run(&store, session(operation), moment(12, 0)).is_err());
        }
    }

    #[test]
    fn the_group_parses_from_the_command_line() {
        let cli = crate::Cli::try_parse_from([
            "timemd", "session", "edit", "0", "--to", "10:00", "-n", "fixed",
        ])
        .expect("parses");
        let Command::Session {
            operation: SessionCommand::Edit { index, note, .. },
        } = cli.command
        else {
            panic!("expected session edit");
        };
        assert_eq!(index, 0);
        assert_eq!(note.as_deref(), Some("fixed"));
    }
}
