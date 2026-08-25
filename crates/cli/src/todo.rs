//! `timemd todo` — the todo list.
//!
//! Every mutation prints the list back, so the argument the next command takes
//! — a todo's id — is on screen.

use clap::Subcommand;
use timemd_core::{Priority, Result, Stamp, Store, Todo, TodoEdit, TodoId, TodoQuery, TodoStatus};

use crate::{clearable, optional_slug};

#[derive(Debug, Subcommand)]
pub enum TodoCommand {
    /// Add a todo. Prints the list, including the id it was given.
    Add {
        description: String,
        #[arg(long)]
        project: Option<String>,
        /// `highest`, `high`, `medium`, `normal`, `low` or `lowest`.
        #[arg(long)]
        priority: Option<String>,
        /// `YYYY-MM-DD`, optionally with a ` HH:MM`.
        #[arg(long)]
        scheduled: Option<String>,
        /// `YYYY-MM-DD`, optionally with a ` HH:MM`.
        #[arg(long)]
        due: Option<String>,
        /// `YYYY-MM-DD`, optionally with a ` HH:MM`.
        #[arg(long)]
        start: Option<String>,
        /// An Obsidian Tasks rule such as `every day when done`. Kept verbatim;
        /// ticking a recurring todo here does not spawn the next one.
        #[arg(long)]
        repeat: Option<String>,
    },

    /// Change a todo, named by its id. A flag given empty clears that field.
    Set {
        id: String,
        /// Tick it off, stamping today as the done date.
        #[arg(long)]
        done: bool,
        /// Untick it, clearing the done date.
        #[arg(long, conflicts_with = "done")]
        undone: bool,
        /// Cancel it, stamping today as the cancelled date.
        #[arg(long, conflicts_with_all = ["done", "undone"])]
        cancel: bool,
        #[arg(long)]
        rename: Option<String>,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        priority: Option<String>,
        #[arg(long)]
        scheduled: Option<String>,
        #[arg(long)]
        due: Option<String>,
        #[arg(long)]
        start: Option<String>,
        #[arg(long)]
        repeat: Option<String>,
    },

    /// Delete a todo, named by its id.
    Rm { id: String },
}

/// What `timemd todos` and `timemd todo` both narrow the list by.
#[derive(Debug, Default, clap::Args)]
pub struct Filter {
    /// Only todos on this project.
    #[arg(long)]
    pub project: Option<String>,
    /// Show settled todos too. By default only open ones are listed.
    #[arg(long)]
    pub all: bool,
    /// Only todos due on or before this `YYYY-MM-DD`.
    #[arg(long)]
    pub due_before: Option<String>,
    /// Only todos scheduled for this `YYYY-MM-DD`.
    #[arg(long)]
    pub scheduled_on: Option<String>,
}

pub fn run(store: &Store, command: TodoCommand, today: chrono::NaiveDate) -> Result<String> {
    apply(store, command, today)?;
    list(store, &Filter::default())
}

fn apply(store: &Store, command: TodoCommand, today: chrono::NaiveDate) -> Result<()> {
    match command {
        TodoCommand::Add {
            description,
            project,
            priority,
            scheduled,
            due,
            start,
            repeat,
        } => {
            let mut todo = Todo::new(TodoStatus::Open, &description)?;
            todo.project = optional_slug(project)?.flatten();
            todo.priority = priority
                .map(|raw| raw.parse::<Priority>())
                .transpose()?
                .unwrap_or_default();
            todo.scheduled = stamp(scheduled)?;
            todo.due = stamp(due)?;
            todo.start = stamp(start)?;
            todo.recurrence = repeat.filter(|rule| !rule.trim().is_empty());
            todo.created = Some(Stamp::on(today));

            store.try_update_todos(|todos| todos.add(todo).map(|_| ()))??;
        }

        TodoCommand::Set {
            id,
            done,
            undone,
            cancel,
            rename,
            project,
            priority,
            scheduled,
            due,
            start,
            repeat,
        } => {
            let id = TodoId::new(id)?;
            // The three flags are exclusive, so whichever is set decides the
            // status and none means leave it. Each stamps or clears the date
            // that goes with it, because a done todo with no done date is a
            // half-written line nobody would type by hand.
            let (status, done_stamp, cancelled_stamp) = if done {
                (Some(TodoStatus::Done), Some(Some(Stamp::on(today))), None)
            } else if undone {
                (Some(TodoStatus::Open), Some(None), Some(None))
            } else if cancel {
                (
                    Some(TodoStatus::Cancelled),
                    None,
                    Some(Some(Stamp::on(today))),
                )
            } else {
                (None, None, None)
            };

            let edit = TodoEdit {
                status,
                description: rename,
                project: optional_slug(project)?,
                priority: priority.map(|raw| raw.parse::<Priority>()).transpose()?,
                recurrence: repeat.map(|rule| Some(rule).filter(|rule| !rule.trim().is_empty())),
                depends_on: None,
                created: None,
                start: clearable(start, |raw| raw.parse::<Stamp>())?,
                scheduled: clearable(scheduled, |raw| raw.parse::<Stamp>())?,
                due: clearable(due, |raw| raw.parse::<Stamp>())?,
                cancelled: cancelled_stamp,
                done: done_stamp,
                on_completion: None,
            };

            store.try_update_todos(|todos| todos.update(&id, edit))??;
        }

        TodoCommand::Rm { id } => {
            let id = TodoId::new(id)?;
            store.try_update_todos(|todos| todos.remove(&id).map(|_| ()))??;
        }
    }
    Ok(())
}

pub fn list(store: &Store, filter: &Filter) -> Result<String> {
    let todos = store.read_todos()?;
    let query = TodoQuery {
        project: optional_slug(filter.project.clone())?.flatten(),
        status: None,
        only_open: !filter.all,
        due_before: stamp(filter.due_before.clone())?,
        scheduled_on: stamp(filter.scheduled_on.clone())?,
    };
    let matching = todos.matching(&query);

    if matching.is_empty() && todos.problems().is_empty() {
        return Ok("nothing to do".to_owned());
    }

    let mut lines: Vec<String> = matching.iter().map(|todo| line(todo)).collect();
    for problem in todos.problems() {
        lines.push(format!("  ! {problem}"));
    }
    Ok(lines.join("\n"))
}

/// One row: the id first, because it is the argument every other command takes.
fn line(todo: &Todo) -> String {
    let id = todo.id().map_or("-", TodoId::as_str);
    let marks = [
        todo.priority.symbol().map(|symbol| symbol.to_string()),
        todo.project.as_ref().map(|slug| format!("[{slug}]")),
        todo.due.map(|due| format!("due {due}")),
        todo.scheduled.map(|when| format!("at {when}")),
        todo.recurrence.as_ref().map(|_| "repeats".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("  ");

    format!(
        "{id:<8} [{}] {}{}",
        todo.status.symbol(),
        todo.description(),
        crate::suffix(&marks),
    )
}

/// A date flag, where empty means "not given" — the same rule `clearable`
/// keeps for the flags that can also clear a field.
fn stamp(raw: Option<String>) -> Result<Option<Stamp>> {
    Ok(clearable(raw, |value| value.parse::<Stamp>())?.flatten())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::store;

    fn date() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
    }

    fn add(store: &Store, description: &str) -> String {
        run(
            store,
            TodoCommand::Add {
                description: description.to_owned(),
                project: None,
                priority: None,
                scheduled: None,
                due: None,
                start: None,
                repeat: None,
            },
            date(),
        )
        .expect("adds")
    }

    fn only_id(store: &Store) -> String {
        let todos = store.read_todos().expect("reads");
        todos.todos()[0]
            .id()
            .expect("an id was minted")
            .as_str()
            .to_owned()
    }

    fn set(store: &Store, id: &str, edit: impl FnOnce(&mut TodoCommand)) -> String {
        let mut command = TodoCommand::Set {
            id: id.to_owned(),
            done: false,
            undone: false,
            cancel: false,
            rename: None,
            project: None,
            priority: None,
            scheduled: None,
            due: None,
            start: None,
            repeat: None,
        };
        edit(&mut command);
        run(store, command, date()).expect("sets")
    }

    #[test]
    fn an_empty_list_says_so() {
        let (_directory, store) = store();
        assert_eq!(
            list(&store, &Filter::default()).expect("lists"),
            "nothing to do"
        );
    }

    #[test]
    fn a_todo_is_added_ticked_renamed_and_removed() {
        let (_directory, store) = store();
        let printed = add(&store, "Draft the notes");
        assert!(printed.contains("[ ] Draft the notes"), "{printed}");

        let id = only_id(&store);

        let renamed = set(&store, &id, |command| {
            if let TodoCommand::Set { rename, due, .. } = command {
                *rename = Some("Draft the release notes".to_owned());
                *due = Some("2026-08-31".to_owned());
            }
        });
        assert!(renamed.contains("Draft the release notes"), "{renamed}");
        assert!(renamed.contains("due 2026-08-31"), "{renamed}");

        // Ticking hides it from the default listing, which shows open work.
        let ticked = set(&store, &id, |command| {
            if let TodoCommand::Set { done, .. } = command {
                *done = true;
            }
        });
        assert_eq!(ticked, "nothing to do");
        assert!(
            list(
                &store,
                &Filter {
                    all: true,
                    ..Filter::default()
                }
            )
            .expect("lists")
            .contains("[x] Draft the release notes")
        );

        let stored = store.read_todos().expect("reads");
        assert_eq!(stored.todos()[0].done, Some(Stamp::on(date())));

        assert_eq!(
            run(&store, TodoCommand::Rm { id }, date()).expect("removes"),
            "nothing to do"
        );
    }

    /// Unticking clears the done date too. A ticked-off todo that keeps its
    /// `✅` after being reopened is a line nobody would type by hand.
    #[test]
    fn unticking_clears_the_done_date() {
        let (_directory, store) = store();
        add(&store, "Ship it");
        let id = only_id(&store);

        set(&store, &id, |command| {
            if let TodoCommand::Set { done, .. } = command {
                *done = true;
            }
        });
        set(&store, &id, |command| {
            if let TodoCommand::Set { undone, .. } = command {
                *undone = true;
            }
        });

        let stored = store.read_todos().expect("reads");
        assert!(stored.todos()[0].is_open());
        assert_eq!(stored.todos()[0].done, None);
    }

    #[test]
    fn cancelling_stamps_the_cancelled_date() {
        let (_directory, store) = store();
        add(&store, "Rewrite the CSS");
        let id = only_id(&store);

        set(&store, &id, |command| {
            if let TodoCommand::Set { cancel, .. } = command {
                *cancel = true;
            }
        });

        let stored = store.read_todos().expect("reads");
        assert_eq!(stored.todos()[0].status, TodoStatus::Cancelled);
        assert_eq!(stored.todos()[0].cancelled, Some(Stamp::on(date())));
    }

    #[test]
    fn an_empty_flag_clears_a_date() {
        let (_directory, store) = store();
        add(&store, "Ship it");
        let id = only_id(&store);

        set(&store, &id, |command| {
            if let TodoCommand::Set { due, .. } = command {
                *due = Some("2026-08-31".to_owned());
            }
        });
        set(&store, &id, |command| {
            if let TodoCommand::Set { due, .. } = command {
                *due = Some(String::new());
            }
        });

        assert_eq!(store.read_todos().expect("reads").todos()[0].due, None);
    }

    #[test]
    fn the_line_carries_the_priority_project_and_dates() {
        let (_directory, store) = store();
        run(
            &store,
            TodoCommand::Add {
                description: "Draft the notes".to_owned(),
                project: Some("timemd".to_owned()),
                priority: Some("high".to_owned()),
                scheduled: Some("2026-08-30 14:00".to_owned()),
                due: Some("2026-08-31".to_owned()),
                start: None,
                repeat: Some("every week".to_owned()),
            },
            date(),
        )
        .expect("adds");

        let printed = list(&store, &Filter::default()).expect("lists");
        assert!(printed.contains("⏫"), "{printed}");
        assert!(printed.contains("[timemd]"), "{printed}");
        assert!(printed.contains("due 2026-08-31"), "{printed}");
        assert!(printed.contains("at 2026-08-30 14:00"), "{printed}");
        assert!(printed.contains("repeats"), "{printed}");
    }

    #[test]
    fn narrows_by_project_and_due_date() {
        let (_directory, store) = store();
        run(
            &store,
            TodoCommand::Add {
                description: "Mine".to_owned(),
                project: Some("timemd".to_owned()),
                due: Some("2026-08-05".to_owned()),
                priority: None,
                scheduled: None,
                start: None,
                repeat: None,
            },
            date(),
        )
        .expect("adds");
        add(&store, "Unowned");

        let mine = list(
            &store,
            &Filter {
                project: Some("timemd".to_owned()),
                ..Filter::default()
            },
        )
        .expect("lists");
        assert!(mine.contains("Mine"), "{mine}");
        assert!(!mine.contains("Unowned"), "{mine}");

        let soon = list(
            &store,
            &Filter {
                due_before: Some("2026-09-01".to_owned()),
                ..Filter::default()
            },
        )
        .expect("lists");
        assert!(soon.contains("Mine"), "{soon}");
        assert!(!soon.contains("Unowned"), "{soon}");
    }

    #[test]
    fn refuses_a_description_the_reader_could_not_get_back() {
        let (_directory, store) = store();
        for description in ["", "has a 📅 in it", "[[timemd]] leading link"] {
            assert!(
                run(
                    &store,
                    TodoCommand::Add {
                        description: description.to_owned(),
                        project: None,
                        priority: None,
                        scheduled: None,
                        due: None,
                        start: None,
                        repeat: None,
                    },
                    date(),
                )
                .is_err(),
                "{description:?} should be refused"
            );
        }
    }

    #[test]
    fn refuses_an_id_nothing_carries() {
        let (_directory, store) = store();
        assert!(
            run(
                &store,
                TodoCommand::Rm {
                    id: "nothere".to_owned()
                },
                date()
            )
            .is_err()
        );
    }

    /// A hand-written todo has no id, so it lists with a dash where one would
    /// be — and gets one the moment anything writes the file.
    #[test]
    fn a_hand_written_todo_lists_without_an_id_until_something_writes() {
        let (_directory, store) = store();
        std::fs::write(
            store.todos_path(),
            "---\n---\n\n## Todos\n\n- [ ] Typed by hand\n",
        )
        .expect("writes the file");

        assert!(
            list(&store, &Filter::default())
                .expect("lists")
                .starts_with("-        [ ] Typed by hand")
        );

        add(&store, "Typed by the app");
        assert!(store.read_todos().expect("reads").todos()[0].id().is_some());
    }

    #[test]
    fn reports_a_line_it_could_not_read() {
        let (_directory, store) = store();
        std::fs::write(
            store.todos_path(),
            "---\n---\n\n## Todos\n\n- [ ] Broken 📅 not-a-date\n",
        )
        .expect("writes the file");

        assert!(
            list(&store, &Filter::default())
                .expect("lists")
                .contains("! ")
        );
    }

    /// Starting on a todo takes its project and its words, so the logged
    /// session is findable by what the todo says.
    #[test]
    fn a_session_started_on_a_todo_takes_its_project_and_words() {
        let (_directory, store) = store();
        run(
            &store,
            TodoCommand::Add {
                description: "Fix the ticker drift".to_owned(),
                project: Some("timemd".to_owned()),
                priority: None,
                scheduled: None,
                due: None,
                start: None,
                repeat: None,
            },
            date(),
        )
        .expect("adds");
        let id = only_id(&store);

        let started = crate::timer::start(
            &store,
            None,
            String::new(),
            None,
            Some(id),
            date().and_hms_opt(9, 0, 0).expect("valid time"),
        )
        .expect("starts");

        assert!(started.contains("timemd"), "{started}");
        assert!(started.contains("Fix the ticker drift"), "{started}");
    }

    #[test]
    fn refuses_to_start_on_a_todo_that_is_not_there() {
        let (_directory, store) = store();
        assert!(
            crate::timer::start(
                &store,
                None,
                String::new(),
                None,
                Some("nothere".to_owned()),
                date().and_hms_opt(9, 0, 0).expect("valid time"),
            )
            .is_err()
        );
    }
}
