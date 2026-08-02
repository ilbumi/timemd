//! `projects` — what is being tracked.

use timemd_core::{Result, Store};

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
    use crate::testing::{moment, store};
    use crate::{Command, create_project, run};
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
}
