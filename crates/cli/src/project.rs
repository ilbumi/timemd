//! `projects` and the `project` and `milestone` groups — what is being tracked.

use chrono::NaiveDate;
use clap::Subcommand;
use timemd_core::error::Error;
use timemd_core::{
    Color, Mark, Milestone, MilestoneEdit, Minutes, Project, ProjectSlug, ProjectStatus, Result,
    Store,
};

use crate::clearable;

#[derive(Subcommand, Debug)]
pub enum ProjectCommand {
    /// Show one project and its milestones.
    Show { slug: String },

    /// Create a project. The slug is derived from the name.
    New {
        name: String,
        /// `#rrggbb`.
        #[arg(long)]
        color: Option<String>,
        /// `square`, `circle`, `triangle`, `diamond` or `bar`.
        #[arg(long)]
        mark: Option<String>,
        /// Hours a week, e.g. `10h`.
        #[arg(long)]
        target: Option<String>,
    },

    /// Change a project. Only the fields given change.
    Set {
        slug: String,
        #[arg(long)]
        name: Option<String>,
        /// Pass an empty string to clear it.
        #[arg(long)]
        color: Option<String>,
        #[arg(long)]
        mark: Option<String>,
        /// Pass an empty string to clear it.
        #[arg(long)]
        target: Option<String>,
        /// `active` or `archived`.
        #[arg(long)]
        status: Option<String>,
    },

    /// Delete a project file. Sessions logged against it keep their link.
    Rm {
        slug: String,
        /// Required: there is no undo and no prompt.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum MilestoneCommand {
    /// Add a milestone. Titles address milestones, so they must be unique.
    Add {
        title: String,
        #[arg(long)]
        project: String,
        /// 0-based position. Omit to append.
        #[arg(long)]
        position: Option<usize>,
    },

    /// Tick, retitle or reorder a milestone, named by its current title.
    Set {
        title: String,
        #[arg(long)]
        project: String,
        #[arg(long)]
        done: bool,
        #[arg(long, conflicts_with = "done")]
        undone: bool,
        #[arg(long)]
        rename: Option<String>,
        #[arg(long)]
        position: Option<usize>,
    },

    /// Delete a milestone, named by its title.
    Rm {
        title: String,
        #[arg(long)]
        project: String,
    },
}

pub fn run(store: &Store, command: ProjectCommand, today: NaiveDate) -> Result<String> {
    match command {
        ProjectCommand::Show { slug } => show(store, &ProjectSlug::new(slug)?),

        ProjectCommand::New {
            name,
            color,
            mark,
            target,
        } => {
            let slug = ProjectSlug::from_name(&name).ok_or_else(|| {
                Error::Invalid(format!(
                    "{name:?} contains nothing usable as a project name"
                ))
            })?;
            let mut project = Project::new(slug.clone(), &name, today);
            project.color = color.map(Color::new).transpose()?;
            project.mark = mark
                .map(|raw| raw.parse::<Mark>())
                .transpose()?
                .unwrap_or_default();
            project.target = target.map(|raw| raw.parse::<Minutes>()).transpose()?;
            store.create_project(&project)?;
            show(store, &slug)
        }

        ProjectCommand::Set {
            slug,
            name,
            color,
            mark,
            target,
            status,
        } => {
            let slug = ProjectSlug::new(slug)?;
            // Every conversion before the store is touched, so a rejected value
            // cannot leave the file half-updated.
            let color = clearable(color, Color::new)?;
            let mark = mark.map(|raw| raw.parse::<Mark>()).transpose()?;
            let target = clearable(target, |raw| raw.parse::<Minutes>())?;
            let status = status.map(|raw| raw.parse::<ProjectStatus>()).transpose()?;

            store.update_project(&slug, |project| {
                if let Some(name) = name {
                    project.name = name;
                }
                if let Some(color) = color {
                    project.color = color;
                }
                if let Some(mark) = mark {
                    project.mark = mark;
                }
                if let Some(target) = target {
                    project.target = target;
                }
                if let Some(status) = status {
                    project.status = status;
                }
            })?;
            show(store, &slug)
        }

        ProjectCommand::Rm { slug, force } => {
            let slug = ProjectSlug::new(slug)?;
            if !force {
                return Err(Error::Invalid(format!(
                    "deleting {slug} cannot be undone; pass --force to confirm"
                )));
            }
            if store.delete_project(&slug)? {
                Ok(format!("deleted {slug}"))
            } else {
                Err(Error::UnknownProject(slug.to_string()))
            }
        }
    }
}

pub fn milestone(store: &Store, command: MilestoneCommand) -> Result<String> {
    match command {
        MilestoneCommand::Add {
            title,
            project,
            position,
        } => {
            let slug = ProjectSlug::new(project)?;
            let milestone = Milestone::new(false, &title)?;
            store.try_update_project(&slug, |project| {
                project
                    .insert_milestone(position.unwrap_or(usize::MAX), milestone)
                    .map(|_| ())
            })??;
            show(store, &slug)
        }

        MilestoneCommand::Set {
            title,
            project,
            done,
            undone,
            rename,
            position,
        } => {
            let slug = ProjectSlug::new(project)?;
            store.try_update_project(&slug, |project| {
                project
                    .update_milestone(
                        &title,
                        MilestoneEdit {
                            // `conflicts_with` makes the pair exclusive, so
                            // either flag decides it and neither means leave it.
                            done: (done || undone).then_some(done),
                            title: rename.clone(),
                            position,
                        },
                    )
                    .map(|_| ())
            })??;
            show(store, &slug)
        }

        MilestoneCommand::Rm { title, project } => {
            let slug = ProjectSlug::new(project)?;
            store.try_update_project(&slug, |project| {
                let index = project.milestone_titled(&title)?;
                project.milestones.remove(index);
                Ok::<_, Error>(())
            })??;
            show(store, &slug)
        }
    }
}

/// One project and its milestones.
///
/// Printed back after every mutation so the argument the next command takes —
/// a milestone's title — is on screen.
fn show(store: &Store, slug: &ProjectSlug) -> Result<String> {
    let project = store
        .read_project(slug)?
        .ok_or_else(|| Error::UnknownProject(slug.to_string()))?;

    let done = project
        .milestones
        .iter()
        .filter(|milestone| milestone.done)
        .count();
    let mut lines = vec![format!(
        "{} — {done}/{} done{}{}",
        project.name,
        project.milestones.len(),
        project
            .target
            .map_or_else(String::new, |target| format!("  {target}/wk")),
        if project.status.is_archived() {
            "  (archived)"
        } else {
            ""
        },
    )];
    for milestone in &project.milestones {
        lines.push(format!(
            "  [{}] {}",
            if milestone.done { "x" } else { " " },
            milestone.title(),
        ));
    }
    for problem in project.problems() {
        lines.push(format!("  ! {problem}"));
    }
    Ok(lines.join("\n"))
}

pub fn list(store: &Store) -> Result<String> {
    let projects = store.list_projects()?;
    if projects.is_empty() {
        return Ok("no projects yet".to_owned());
    }
    Ok(projects
        .iter()
        .map(|project| {
            let target = project
                .target
                .map_or_else(String::new, |target| format!("  {target}/wk"));
            let milestones = if project.milestones.is_empty() {
                String::new()
            } else {
                let done = project
                    .milestones
                    .iter()
                    .filter(|milestone| milestone.done)
                    .count();
                format!("  {done}/{} done", project.milestones.len())
            };
            let slug = project.slug().as_str();
            let name = &project.name;
            let archived = if project.status.is_archived() {
                "  (archived)"
            } else {
                ""
            };
            format!("{slug:<24} {name}{target}{milestones}{archived}")
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{MilestoneCommand, ProjectCommand};
    use crate::testing::{moment, store};
    use crate::{Command, create_project, run};
    use clap::Parser;
    use timemd_core::{Milestone, Minutes, ProjectSlug, ProjectStatus};

    #[test]
    fn projects_lists_names_and_archived_state() {
        let (_directory, store) = store();
        let today = moment(9, 0).date();
        create_project(&store, "timemd", "timemd", today).expect("creates");
        create_project(&store, "admin", "Admin", today).expect("creates");
        store
            .update_project(&ProjectSlug::new("admin").expect("valid"), |project| {
                project.status = ProjectStatus::Archived;
            })
            .expect("updates");

        let output = run(&store, Command::Projects, moment(9, 0)).expect("reads");
        assert!(output.contains("timemd"), "{output}");
        assert!(output.contains("(archived)"), "{output}");
    }

    #[test]
    fn projects_shows_the_weekly_target_and_milestone_progress() {
        let (_directory, store) = store();
        let today = moment(9, 0).date();
        create_project(&store, "thesis", "Thesis", today).expect("creates");
        store
            .update_project(&ProjectSlug::new("thesis").expect("valid"), |project| {
                project.target = Some(Minutes::new(600));
                project.milestones = vec![
                    Milestone::new(true, "Ch. 1").expect("valid"),
                    Milestone::new(false, "Ch. 2").expect("valid"),
                ];
            })
            .expect("updates");

        let output = run(&store, Command::Projects, moment(9, 0)).expect("reads");
        assert!(output.contains("10h/wk"), "{output}");
        assert!(output.contains("1/2 done"), "{output}");
    }

    #[test]
    fn projects_says_so_when_there_are_none() {
        let (_directory, store) = store();
        assert_eq!(
            run(&store, Command::Projects, moment(9, 0)).expect("reads"),
            "no projects yet"
        );
    }

    fn thesis(store: &timemd_core::Store) {
        run(
            store,
            Command::Project {
                operation: ProjectCommand::New {
                    name: "Thesis".to_owned(),
                    color: None,
                    mark: None,
                    target: Some("10h".to_owned()),
                },
            },
            moment(9, 0),
        )
        .expect("creates");
    }

    fn milestone(operation: MilestoneCommand) -> Command {
        Command::Milestone { operation }
    }

    fn add(title: &str) -> MilestoneCommand {
        MilestoneCommand::Add {
            title: title.to_owned(),
            project: "thesis".to_owned(),
            position: None,
        }
    }

    #[test]
    fn a_project_is_created_shown_changed_and_deleted() {
        let (_directory, store) = store();
        thesis(&store);

        let output = run(
            &store,
            Command::Project {
                operation: ProjectCommand::Show {
                    slug: "thesis".to_owned(),
                },
            },
            moment(9, 0),
        )
        .expect("shows");
        assert!(output.contains("Thesis"), "{output}");
        assert!(output.contains("10h/wk"), "{output}");

        let output = run(
            &store,
            Command::Project {
                operation: ProjectCommand::Set {
                    slug: "thesis".to_owned(),
                    name: Some("The Thesis".to_owned()),
                    color: Some("#4f46e5".to_owned()),
                    mark: None,
                    // An empty string clears it, which is the only way to say so.
                    target: Some(String::new()),
                    status: Some("archived".to_owned()),
                },
            },
            moment(9, 0),
        )
        .expect("changes");
        assert!(output.contains("The Thesis"), "{output}");
        assert!(output.contains("(archived)"), "{output}");
        assert!(!output.contains("/wk"), "{output}");

        let output = run(
            &store,
            Command::Project {
                operation: ProjectCommand::Rm {
                    slug: "thesis".to_owned(),
                    force: true,
                },
            },
            moment(9, 0),
        )
        .expect("deletes");
        assert_eq!(output, "deleted thesis");
    }

    /// There is no undo and no prompt, so the flag is the confirmation.
    #[test]
    fn deleting_a_project_needs_the_force_flag() {
        let (_directory, store) = store();
        thesis(&store);

        assert!(
            run(
                &store,
                Command::Project {
                    operation: ProjectCommand::Rm {
                        slug: "thesis".to_owned(),
                        force: false,
                    },
                },
                moment(9, 0),
            )
            .is_err()
        );
        assert!(
            store
                .read_project(&timemd_core::ProjectSlug::new("thesis").expect("valid"))
                .expect("reads")
                .is_some()
        );
    }

    #[test]
    fn a_rejected_value_leaves_the_project_alone() {
        let (_directory, store) = store();
        thesis(&store);

        for operation in [
            ProjectCommand::Set {
                slug: "thesis".to_owned(),
                name: Some("Renamed".to_owned()),
                color: Some("blurple".to_owned()),
                mark: None,
                target: None,
                status: None,
            },
            ProjectCommand::Set {
                slug: "thesis".to_owned(),
                name: Some("Renamed".to_owned()),
                color: None,
                mark: None,
                target: None,
                status: Some("hibernating".to_owned()),
            },
        ] {
            assert!(run(&store, Command::Project { operation }, moment(9, 0)).is_err());
        }

        let output = run(
            &store,
            Command::Project {
                operation: ProjectCommand::Show {
                    slug: "thesis".to_owned(),
                },
            },
            moment(9, 0),
        )
        .expect("shows");
        assert!(output.contains("Thesis"), "{output}");
        assert!(!output.contains("Renamed"), "{output}");
    }

    #[test]
    fn a_milestone_is_added_ticked_renamed_reordered_and_removed() {
        let (_directory, store) = store();
        thesis(&store);
        run(&store, milestone(add("Ch. 1")), moment(9, 0)).expect("adds");
        let output = run(&store, milestone(add("Ch. 2")), moment(9, 0)).expect("adds");
        assert!(output.contains("0/2 done"), "{output}");

        let output = run(
            &store,
            milestone(MilestoneCommand::Set {
                title: "Ch. 1".to_owned(),
                project: "thesis".to_owned(),
                done: true,
                undone: false,
                rename: Some("Ch. 1 — lit review".to_owned()),
                position: Some(1),
            }),
            moment(9, 0),
        )
        .expect("sets");
        assert!(output.contains("1/2 done"), "{output}");
        let lines: Vec<&str> = output.lines().skip(1).collect();
        assert_eq!(lines, ["  [ ] Ch. 2", "  [x] Ch. 1 — lit review"]);

        let output = run(
            &store,
            milestone(MilestoneCommand::Rm {
                title: "Ch. 2".to_owned(),
                project: "thesis".to_owned(),
            }),
            moment(9, 0),
        )
        .expect("removes");
        assert!(output.contains("1/1 done"), "{output}");
    }

    #[test]
    fn a_milestone_can_be_unticked() {
        let (_directory, store) = store();
        thesis(&store);
        run(&store, milestone(add("Ch. 1")), moment(9, 0)).expect("adds");

        let tick = |done: bool, undone: bool| {
            run(
                &store,
                milestone(MilestoneCommand::Set {
                    title: "Ch. 1".to_owned(),
                    project: "thesis".to_owned(),
                    done,
                    undone,
                    rename: None,
                    position: None,
                }),
                moment(9, 0),
            )
            .expect("sets")
        };

        assert!(tick(true, false).contains("1/1 done"));
        assert!(tick(false, true).contains("0/1 done"));
    }

    /// A title nobody can address is a title nobody can edit.
    #[test]
    fn a_duplicate_or_unknown_milestone_title_is_an_error() {
        let (_directory, store) = store();
        thesis(&store);
        run(&store, milestone(add("Ch. 1")), moment(9, 0)).expect("adds");

        assert!(run(&store, milestone(add("Ch. 1")), moment(9, 0)).is_err());
        assert!(
            run(
                &store,
                milestone(MilestoneCommand::Rm {
                    title: "Ch. 9".to_owned(),
                    project: "thesis".to_owned(),
                }),
                moment(9, 0),
            )
            .is_err()
        );
    }

    /// The shell half of the reproduction: `--done` lands by assignment and
    /// `--rename` can refuse, so the command used to report an error and leave
    /// the milestone ticked.
    #[test]
    fn a_refused_rename_does_not_tick_the_milestone() {
        let (_directory, store) = store();
        thesis(&store);
        run(&store, milestone(add("Ch. 1")), moment(9, 0)).expect("adds");
        run(&store, milestone(add("Ch. 2")), moment(9, 0)).expect("adds");

        assert!(
            run(
                &store,
                milestone(MilestoneCommand::Set {
                    title: "Ch. 2".to_owned(),
                    project: "thesis".to_owned(),
                    done: true,
                    undone: false,
                    rename: Some("Ch. 1".to_owned()),
                    position: None,
                }),
                moment(9, 0),
            )
            .is_err(),
            "the title is taken"
        );

        let output = run(
            &store,
            Command::Project {
                operation: ProjectCommand::Show {
                    slug: "thesis".to_owned(),
                },
            },
            moment(9, 0),
        )
        .expect("shows");
        assert!(output.contains("0/2 done"), "{output}");
    }

    #[test]
    fn done_and_undone_cannot_both_be_given() {
        assert!(
            crate::Cli::try_parse_from([
                "timemd",
                "milestone",
                "set",
                "Ch. 1",
                "--project",
                "thesis",
                "--done",
                "--undone",
            ])
            .is_err()
        );

        let cli = crate::Cli::try_parse_from([
            "timemd",
            "milestone",
            "set",
            "Ch. 1",
            "--project",
            "thesis",
            "--done",
        ])
        .expect("parses");
        let Command::Milestone {
            operation: MilestoneCommand::Set { title, done, .. },
        } = cli.command
        else {
            panic!("expected milestone set");
        };
        assert_eq!(title, "Ch. 1");
        assert!(done);
    }

    #[test]
    fn a_name_with_nothing_sluggable_in_it_is_rejected() {
        let (_directory, store) = store();
        assert!(
            run(
                &store,
                Command::Project {
                    operation: ProjectCommand::New {
                        name: "!!!".to_owned(),
                        color: None,
                        mark: None,
                        target: None,
                    },
                },
                moment(9, 0),
            )
            .is_err()
        );
    }
}
