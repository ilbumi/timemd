//! `schedule` and the `block` and `repeat` groups — time that is planned.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use clap::Subcommand;
use timemd_core::error::Error;
use timemd_core::schedule::planned_range;
use timemd_core::{
    BlockId, DateRange, DayBlock, Minutes, Occurrence, RecurringBlock, Result, Store,
};

use crate::{clearable, name_or_dash, optional_slug};

#[derive(Subcommand, Debug)]
pub enum BlockCommand {
    /// Plan a one-off block on a day.
    Add {
        #[arg(long)]
        date: Option<NaiveDate>,
        #[arg(long)]
        from: NaiveTime,
        #[arg(long)]
        to: NaiveTime,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value = "")]
        title: String,
        /// How long before the start to remind, e.g. `5m`.
        #[arg(long)]
        remind: Option<String>,
    },

    /// Amend a one-off block. Only the fields given change.
    Edit {
        /// Position among the day's one-offs, as `timemd schedule` prints it.
        index: usize,
        #[arg(long)]
        date: Option<NaiveDate>,
        #[arg(long)]
        from: Option<NaiveTime>,
        #[arg(long)]
        to: Option<NaiveTime>,
        /// Project slug. Pass an empty string to clear it.
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        title: Option<String>,
        /// Pass an empty string to clear the reminder.
        #[arg(long)]
        remind: Option<String>,
    },

    /// Delete a one-off block.
    Rm {
        /// Position among the day's one-offs, as `timemd schedule` prints it.
        index: usize,
        #[arg(long)]
        date: Option<NaiveDate>,
    },
}

#[derive(Subcommand, Debug)]
pub enum RepeatCommand {
    /// Show the weekly pattern.
    List,

    /// Create or replace one repeating block, keyed on its id.
    Set {
        id: String,
        /// Weekdays: `mon`, `mon-fri`, `mon,wed,fri` or `daily`.
        #[arg(long)]
        days: String,
        #[arg(long)]
        from: NaiveTime,
        #[arg(long)]
        to: NaiveTime,
        #[arg(long)]
        project: Option<String>,
        #[arg(long, default_value = "")]
        title: String,
        /// How long before the start to remind, e.g. `5m`.
        #[arg(long)]
        remind: Option<String>,
    },

    /// Delete a repeating block.
    Rm { id: String },

    /// Suppress a repeating block on one day, leaving the pattern alone.
    Skip {
        id: String,
        #[arg(long)]
        date: Option<NaiveDate>,
    },

    /// Restore a repeating block that was skipped on a day.
    Restore {
        id: String,
        #[arg(long)]
        date: Option<NaiveDate>,
    },
}

/// What is planned over a range, defaulting to today.
pub fn range(
    store: &Store,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    now: NaiveDateTime,
) -> Result<String> {
    let from = from.unwrap_or_else(|| now.date());
    span(store, from, to.unwrap_or(from))
}

/// What is planned on one day, which every block and repeat command reprints so
/// the handle its next argument takes is on screen.
fn on(store: &Store, date: NaiveDate) -> Result<String> {
    span(store, date, date)
}

fn span(store: &Store, from: NaiveDate, to: NaiveDate) -> Result<String> {
    let occurrences = planned_range(store, DateRange::new(from, to)?)?;

    if occurrences.is_empty() {
        return Ok(format!("{from} → {to} — nothing planned"));
    }

    let mut lines = Vec::new();
    let mut date = None;

    for occurrence in &occurrences {
        if date != Some(occurrence.date) {
            date = Some(occurrence.date);
            lines.push(occurrence.date.to_string());
        }
        lines.push(match &occurrence.block {
            Some(id) => format!("     {}  `{id}`", body(occurrence)),
            // `planned` numbers exactly the entries with no repeating source,
            // and the number it gives is the one `block edit` and `block rm`
            // take: a position among that day's one-offs.
            None => format!(
                "  {}  {}",
                occurrence.one_off_index.unwrap_or_default(),
                body(occurrence)
            ),
        });
    }
    Ok(lines.join("\n"))
}

pub fn block(store: &Store, command: BlockCommand, now: NaiveDateTime) -> Result<String> {
    match command {
        BlockCommand::Add {
            date,
            from,
            to,
            project,
            title,
            remind,
        } => {
            let date = date.unwrap_or_else(|| now.date());
            let block = DayBlock {
                start: from,
                end: to,
                project: optional_slug(project)?.flatten(),
                title: title.trim().to_owned(),
                remind_before: remind.map(|raw| raw.parse::<Minutes>()).transpose()?,
            };
            store.update_day(date, |day| day.add_block(block))?;
            on(store, date)
        }

        BlockCommand::Edit {
            index,
            date,
            from,
            to,
            project,
            title,
            remind,
        } => {
            let date = date.unwrap_or_else(|| now.date());
            let project = optional_slug(project)?;
            let remind = clearable(remind, |raw: String| raw.parse::<Minutes>())?;

            store.try_update_day(date, |day| {
                let existing = day
                    .schedule()
                    .get(index)
                    .cloned()
                    .ok_or_else(|| missing_block(index, date))?;

                day.replace_block(
                    index,
                    DayBlock {
                        start: from.unwrap_or(existing.start),
                        end: to.unwrap_or(existing.end),
                        project: project.unwrap_or(existing.project),
                        title: title.map_or(existing.title, |title| title.trim().to_owned()),
                        remind_before: remind.unwrap_or(existing.remind_before),
                    },
                );
                Ok::<_, Error>(())
            })??;
            on(store, date)
        }

        BlockCommand::Rm { index, date } => {
            let date = date.unwrap_or_else(|| now.date());
            store.try_update_day(date, |day| {
                day.remove_block(index)
                    .map(|_| ())
                    .ok_or_else(|| missing_block(index, date))
            })??;
            on(store, date)
        }
    }
}

/// The repeating pattern, which every `repeat` write reprints so the id its
/// next argument takes is on screen.
fn list(store: &Store) -> Result<String> {
    let recurring = store.read_recurring()?;
    if recurring.blocks().is_empty() {
        return Ok("nothing repeats yet".to_owned());
    }
    let mut lines: Vec<String> = recurring
        .blocks()
        .iter()
        .map(|block| {
            format!(
                "  `{}`  {:<16} {}-{}  {}  {}{}",
                block.id,
                block.days,
                block.start.format("%H:%M"),
                block.end.format("%H:%M"),
                name_or_dash(block.project.as_ref()),
                block.title,
                block
                    .remind_before
                    .map_or_else(String::new, |lead| format!("  !{lead}")),
            )
        })
        .collect();
    for problem in recurring.problems() {
        lines.push(format!("  ! {problem}"));
    }
    Ok(lines.join("\n"))
}

pub fn repeat(store: &Store, command: RepeatCommand, now: NaiveDateTime) -> Result<String> {
    match command {
        RepeatCommand::List => list(store),

        RepeatCommand::Set {
            id,
            days,
            from,
            to,
            project,
            title,
            remind,
        } => {
            let block = RecurringBlock {
                id: BlockId::new(&id)?,
                // Handed straight to core's parser, so there is one definition
                // of what a day spec means across all four surfaces.
                days: days.parse()?,
                start: from,
                end: to,
                project: optional_slug(project)?.flatten(),
                title: title.trim().to_owned(),
                remind_before: remind.map(|raw| raw.parse::<Minutes>()).transpose()?,
            };
            // Keyed on the id, so changing one block leaves every other alone.
            store.update_recurring(|recurring| recurring.upsert(block))?;
            list(store)
        }

        RepeatCommand::Rm { id } => {
            let id = BlockId::new(&id)?;
            if !store.update_recurring(|recurring| recurring.remove(&id))? {
                return Err(Error::Invalid(format!("no repeating block named {id:?}")));
            }
            list(store)
        }

        RepeatCommand::Skip { id, date } => {
            let date = date.unwrap_or_else(|| now.date());
            let id = BlockId::new(&id)?;
            store.update_day(date, |day| day.skip(id))?;
            on(store, date)
        }

        RepeatCommand::Restore { id, date } => {
            let date = date.unwrap_or_else(|| now.date());
            let id = BlockId::new(&id)?;
            if !store.update_day(date, |day| day.unskip(&id))? {
                return Err(Error::Invalid(format!("{id} was not skipped on {date}")));
            }
            on(store, date)
        }
    }
}

fn body(occurrence: &Occurrence) -> String {
    format!(
        "{}-{} {:>7}  {}  {}{}",
        occurrence.start.format("%H:%M"),
        occurrence.end.format("%H:%M"),
        occurrence.duration().to_string(),
        name_or_dash(occurrence.project.as_ref()),
        occurrence.title,
        occurrence
            .remind_before
            .map_or_else(String::new, |lead| format!("  !{lead}")),
    )
}

fn missing_block(index: usize, date: NaiveDate) -> Error {
    Error::Invalid(format!("no block at index {index} on {date}"))
}

#[cfg(test)]
mod tests {
    use super::{BlockCommand, RepeatCommand};
    use crate::testing::{at, moment, store};
    use crate::{Command, run};
    use clap::Parser;

    fn deep_work(store: &timemd_core::Store) {
        run(
            store,
            Command::Repeat {
                operation: RepeatCommand::Set {
                    id: "deep-work".to_owned(),
                    days: "mon-fri".to_owned(),
                    from: at(9, 0),
                    to: at(11, 0),
                    project: None,
                    title: "Deep work".to_owned(),
                    remind: Some("5m".to_owned()),
                },
            },
            moment(9, 0),
        )
        .expect("sets");
    }

    /// 2026-08-01 is a Saturday, so `mon-fri` does not fall on it.
    const WEDNESDAY: &str = "2026-08-05";

    fn wednesday() -> chrono::NaiveDate {
        WEDNESDAY.parse().expect("a date")
    }

    #[test]
    fn schedule_defaults_to_today_and_says_when_nothing_is_planned() {
        let (_directory, store) = store();
        let output = run(
            &store,
            Command::Schedule {
                from: None,
                to: None,
            },
            moment(9, 0),
        )
        .expect("reads");
        assert!(output.contains("nothing planned"), "{output}");
    }

    /// The one-off index and the repeat's id are the handles `block edit`,
    /// `block rm` and `repeat skip` take, so the listing must print both.
    #[test]
    fn schedule_numbers_one_offs_and_names_repeats() {
        let (_directory, store) = store();
        deep_work(&store);
        for (from, to, title) in [(12, 0, "Lunch"), (15, 0, "Walk")] {
            run(
                &store,
                Command::Block {
                    operation: BlockCommand::Add {
                        date: Some(wednesday()),
                        from: at(from, to),
                        to: at(from + 1, 0),
                        project: None,
                        title: title.to_owned(),
                        remind: None,
                    },
                },
                moment(9, 0),
            )
            .expect("adds");
        }

        let output = run(
            &store,
            Command::Schedule {
                from: Some(wednesday()),
                to: Some(wednesday()),
            },
            moment(9, 0),
        )
        .expect("reads");

        assert!(output.contains("`deep-work`"), "{output}");
        assert!(output.contains("  0  12:00"), "{output}");
        assert!(output.contains("  1  15:00"), "{output}");
    }

    #[test]
    fn a_one_off_block_is_added_amended_and_removed() {
        let (_directory, store) = store();
        let block = |operation| Command::Block { operation };

        run(
            &store,
            block(BlockCommand::Add {
                date: Some(wednesday()),
                from: at(12, 0),
                to: at(12, 30),
                project: None,
                title: "Lunch".to_owned(),
                remind: None,
            }),
            moment(9, 0),
        )
        .expect("adds");

        let output = run(
            &store,
            block(BlockCommand::Edit {
                index: 0,
                date: Some(wednesday()),
                from: None,
                to: Some(at(13, 0)),
                project: Some("timemd".to_owned()),
                title: Some("Long lunch".to_owned()),
                remind: None,
            }),
            moment(9, 0),
        )
        .expect("edits");
        assert!(output.contains("Long lunch"), "{output}");
        assert!(output.contains("12:00-13:00"), "{output}");
        assert!(output.contains("timemd"), "{output}");

        let output = run(
            &store,
            block(BlockCommand::Rm {
                index: 0,
                date: Some(wednesday()),
            }),
            moment(9, 0),
        )
        .expect("removes");
        assert!(output.contains("nothing planned"), "{output}");
    }

    #[test]
    fn addressing_a_block_that_is_not_there_is_an_error() {
        let (_directory, store) = store();
        for operation in [
            BlockCommand::Rm {
                index: 3,
                date: Some(wednesday()),
            },
            BlockCommand::Edit {
                index: 3,
                date: Some(wednesday()),
                from: None,
                to: None,
                project: None,
                title: Some("nowhere".to_owned()),
                remind: None,
            },
        ] {
            assert!(run(&store, Command::Block { operation }, moment(9, 0)).is_err());
        }
    }

    /// Keyed on the id: setting an existing one replaces that block and leaves
    /// every other alone.
    #[test]
    fn a_repeating_block_is_upserted_by_id_and_removed_by_id() {
        let (_directory, store) = store();
        deep_work(&store);

        let output = run(
            &store,
            Command::Repeat {
                operation: RepeatCommand::Set {
                    id: "deep-work".to_owned(),
                    days: "mon,wed".to_owned(),
                    from: at(10, 0),
                    to: at(11, 0),
                    project: None,
                    title: "Focus".to_owned(),
                    remind: None,
                },
            },
            moment(9, 0),
        )
        .expect("replaces");
        assert_eq!(output.lines().count(), 1, "{output}");
        assert!(output.contains("Focus"), "{output}");
        assert!(output.contains("mon,wed"), "{output}");

        let output = run(
            &store,
            Command::Repeat {
                operation: RepeatCommand::Rm {
                    id: "deep-work".to_owned(),
                },
            },
            moment(9, 0),
        )
        .expect("removes");
        assert_eq!(output, "nothing repeats yet");

        assert!(
            run(
                &store,
                Command::Repeat {
                    operation: RepeatCommand::Rm {
                        id: "deep-work".to_owned()
                    },
                },
                moment(9, 0),
            )
            .is_err()
        );
    }

    #[test]
    fn a_repeat_is_skipped_on_one_day_and_restored() {
        let (_directory, store) = store();
        deep_work(&store);
        let skip = |operation| Command::Repeat { operation };

        let output = run(
            &store,
            skip(RepeatCommand::Skip {
                id: "deep-work".to_owned(),
                date: Some(wednesday()),
            }),
            moment(9, 0),
        )
        .expect("skips");
        assert!(output.contains("nothing planned"), "{output}");

        // Thursday is untouched: the pattern itself did not change.
        let thursday = "2026-08-06".parse().expect("a date");
        let output = run(
            &store,
            Command::Schedule {
                from: Some(thursday),
                to: Some(thursday),
            },
            moment(9, 0),
        )
        .expect("reads");
        assert!(output.contains("Deep work"), "{output}");

        let output = run(
            &store,
            skip(RepeatCommand::Restore {
                id: "deep-work".to_owned(),
                date: Some(wednesday()),
            }),
            moment(9, 0),
        )
        .expect("restores");
        assert!(output.contains("Deep work"), "{output}");

        assert!(
            run(
                &store,
                skip(RepeatCommand::Restore {
                    id: "deep-work".to_owned(),
                    date: Some(wednesday()),
                }),
                moment(9, 0),
            )
            .is_err()
        );
    }

    #[test]
    fn a_malformed_day_spec_or_id_is_rejected() {
        let (_directory, store) = store();
        for (id, days) in [("deep-work", "someday"), ("Not An Id", "mon")] {
            assert!(
                run(
                    &store,
                    Command::Repeat {
                        operation: RepeatCommand::Set {
                            id: id.to_owned(),
                            days: days.to_owned(),
                            from: at(9, 0),
                            to: at(10, 0),
                            project: None,
                            title: "x".to_owned(),
                            remind: None,
                        },
                    },
                    moment(9, 0),
                )
                .is_err(),
                "{id} on {days} should be rejected"
            );
        }
    }

    #[test]
    fn the_group_parses_from_the_command_line() {
        let cli = crate::Cli::try_parse_from([
            "timemd",
            "repeat",
            "set",
            "deep-work",
            "--days",
            "mon-fri",
            "--from",
            "09:00",
            "--to",
            "11:00",
        ])
        .expect("parses");
        let Command::Repeat {
            operation: RepeatCommand::Set { id, days, .. },
        } = cli.command
        else {
            panic!("expected repeat set");
        };
        assert_eq!(id, "deep-work");
        assert_eq!(days, "mon-fri");
    }
}
