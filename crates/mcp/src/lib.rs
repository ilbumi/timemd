//! Model Context Protocol server exposing timemd to agents.
//!
//! The tools call `timemd-core` directly, so they work whether or not the web
//! app is running. They are also deliberately thin: an agent that wants
//! something not offered here can read and write the markdown tree by hand, and
//! the app picks the change up on its next read.

use std::sync::Arc;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rmcp::ServerHandler;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{Implementation, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use timemd_core::active::SessionKind;
use timemd_core::day::Session;
use timemd_core::report::{self, GroupBy};
use timemd_core::schedule::{planned, planned_range};
use timemd_core::{
    BlockId, Color, DateRange, DayBlock, Mark, Milestone, MilestoneEdit, Minutes, Project,
    ProjectSlug, ProjectStatus, Recurring, RecurringBlock, Settings, SettingsPatch, StartRequest,
    Stopped, Store, Timer,
};

/// The MCP server. One store, no other state.
#[derive(Clone)]
pub struct TimeMd {
    store: Arc<Store>,
    tool_router: ToolRouter<Self>,
}

// ---- tool parameters -------------------------------------------------------

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct StartParams {
    /// Project slug to track against. Omit to track untagged time.
    pub project: Option<String>,
    /// What you are working on.
    pub note: Option<String>,
    /// Length such as `25m` or `1h30m`. Defaults to the configured focus length.
    pub duration: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct LogParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// Start time, `HH:MM`.
    pub start: String,
    /// End time, `HH:MM`. Earlier than `start` means it crossed midnight.
    pub end: String,
    pub project: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct DateParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct RangeParams {
    /// `YYYY-MM-DD`, inclusive.
    pub from: String,
    /// `YYYY-MM-DD`, inclusive.
    pub to: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ReportParams {
    pub from: String,
    pub to: String,
    /// `project` (default) or `day`.
    pub group_by: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct UpsertProjectParams {
    /// Lowercase letters, digits and dashes. Becomes the filename.
    pub slug: String,
    pub name: Option<String>,
    /// `#rrggbb`.
    pub color: Option<String>,
    /// `square`, `circle`, `triangle`, `diamond` or `bar` — the shape the
    /// project is drawn as.
    pub mark: Option<String>,
    /// Hours to spend on this each week, as `10h` or `1h30m`.
    pub target: Option<String>,
    /// `active` or `archived`.
    pub status: Option<String>,
    /// Replaces the whole list when given. Omit to leave it alone.
    pub milestones: Option<Vec<MilestoneIo>>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct SlugParams {
    /// Project slug.
    pub slug: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct AddMilestoneParams {
    /// Project slug.
    pub project: String,
    /// The line's text. Must be non-empty, on one line, and not already used by
    /// another milestone on this project.
    pub title: String,
    /// Whether it starts ticked. Defaults to false.
    pub done: Option<bool>,
    /// 0-based position to insert at. Omit to append.
    pub position: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct UpdateMilestoneParams {
    /// Project slug.
    pub project: String,
    /// The milestone to change, named by its current title. Must match exactly
    /// one: two milestones with the same title is an error, not a coin flip.
    pub title: String,
    /// Tick or untick it. Omit to leave it alone.
    pub done: Option<bool>,
    /// Retitle it. Omit to leave it alone.
    pub new_title: Option<String>,
    /// Move it to this 0-based position. Omit to leave it where it is.
    pub position: Option<usize>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct MilestoneParams {
    /// Project slug.
    pub project: String,
    /// The milestone to remove, named by its title.
    pub title: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct EditSessionParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// Position in the day's sessions, as `day` reports it. Sessions are stored
    /// in start order, so changing a start time renumbers them — the day this
    /// answers with carries the fresh indexes.
    pub index: usize,
    /// Start time, `HH:MM`. Omit to leave it alone.
    pub start: Option<String>,
    /// End time, `HH:MM`. Omit to leave it alone.
    pub end: Option<String>,
    /// Project slug. An empty string clears it; omit to leave it alone.
    pub project: Option<String>,
    /// Omit to leave it alone.
    pub note: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct IndexParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// Position in the day, as `day` reports it.
    pub index: usize,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct SettingsParams {
    /// Focus length, as `25m` or `1h30m`. Omit to leave it alone.
    pub focus: Option<String>,
    /// Omit to leave it alone.
    pub short_break: Option<String>,
    /// Omit to leave it alone.
    pub long_break: Option<String>,
    /// Default reminder lead for blocks that do not set their own. Omit to
    /// leave it alone.
    pub remind_before: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct BlockParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// Start time, `HH:MM`.
    pub start: String,
    /// End time, `HH:MM`.
    pub end: String,
    pub project: Option<String>,
    pub title: String,
    /// How long before the start to remind, as `5m`. Omit for no reminder.
    pub remind_before: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct EditBlockParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// The block's `one_off_index`, as `day` reports it. Blocks are stored in
    /// start order, so changing a start time renumbers them — the day this
    /// answers with carries the fresh indexes.
    pub index: usize,
    /// Omit to leave it alone.
    pub start: Option<String>,
    /// Omit to leave it alone.
    pub end: Option<String>,
    /// An empty string clears the project. Omit to leave it alone.
    pub project: Option<String>,
    /// Omit to leave it alone.
    pub title: Option<String>,
    /// An empty string clears the reminder. Omit to leave it alone.
    pub remind_before: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct SkipParams {
    /// `YYYY-MM-DD`. Defaults to today.
    pub date: Option<String>,
    /// The repeating block's id, as `day` reports it in `block`.
    pub id: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct RecurringBlockParams {
    /// Identifies the block. Creating one that already exists replaces it.
    pub id: String,
    /// Weekday names, `mon` to `sun`. A range like `mon-fri` or `daily` also
    /// works, as a single entry.
    pub days: Vec<String>,
    /// Start time, `HH:MM`.
    pub start: String,
    /// End time, `HH:MM`.
    pub end: String,
    pub project: Option<String>,
    pub title: String,
    /// How long before the start to remind, as `5m`. Omit for no reminder.
    pub remind_before: Option<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct BlockIdParams {
    /// The repeating block's id.
    pub id: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct NoParams {}

// ---- tool results ----------------------------------------------------------

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Outcome {
    pub message: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct CurrentSession {
    pub running: bool,
    pub kind: Option<String>,
    pub project: Option<String>,
    pub note: Option<String>,
    pub started_at: Option<String>,
    pub remaining: Option<String>,
    pub tracked_today: String,
    pub completed_today: u32,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct LoggedSession {
    /// Position in the day, which is what editing and deleting address. A
    /// session has no name, so this is the only handle there is. Sessions are
    /// stored in start order, so changing a start time renumbers the day —
    /// every tool that writes one answers with the day it produced.
    pub index: usize,
    pub start: String,
    pub end: String,
    pub duration: String,
    pub project: Option<String>,
    pub note: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct PlannedBlock {
    pub date: String,
    pub start: String,
    pub end: String,
    pub duration: String,
    pub project: Option<String>,
    pub title: String,
    /// How long before the start a reminder fires, or null for none.
    pub remind_before: Option<String>,
    /// The repeating block this came from, or null for a one-off. Repeats are
    /// addressed by this id; one-offs by `one_off_index`.
    pub block: Option<String>,
    /// Position among that day's one-off blocks, which is what editing and
    /// removing address. Null for a repeat.
    pub one_off_index: Option<usize>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct DaySummary {
    pub date: String,
    pub tracked: String,
    pub sessions: Vec<LoggedSession>,
    pub planned: Vec<PlannedBlock>,
    /// Ids of repeating blocks suppressed on this day.
    pub skipped: Vec<String>,
    /// Lines in the file the parser could not read, kept verbatim.
    pub problems: Vec<String>,
}

/// A list on its own is not a legal tool result: MCP requires structured
/// content, and its schema, to be an object. Lists travel wrapped in one.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct Schedule {
    pub blocks: Vec<PlannedBlock>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectList {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct SettingsSummary {
    /// Read-only here: it is what turns every bare wall-clock time in the tree
    /// into an instant, so it is changed by editing `settings.md`.
    pub timezone: String,
    pub focus: String,
    pub short_break: String,
    pub long_break: String,
    /// Focus sessions between long breaks. Read-only here.
    pub long_break_every: u32,
    pub remind_before: String,
}

/// A recurring block in both directions, like `MilestoneIo`.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct RecurringBlockIo {
    pub id: String,
    /// Weekday names, Monday first. The file spells runs as ranges and `daily`;
    /// core owns that, so the wire carries the plain set.
    pub days: Vec<String>,
    pub start: String,
    pub end: String,
    pub project: Option<String>,
    pub title: String,
    pub remind_before: Option<String>,
}

/// Wrapped for the same reason as `Schedule`: a bare array is not a legal
/// tool result.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct RecurringList {
    pub blocks: Vec<RecurringBlockIo>,
    /// Lines in `schedule/recurring.md` the parser could not read.
    pub problems: Vec<String>,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ProjectSummary {
    pub slug: String,
    pub name: String,
    pub color: Option<String>,
    pub mark: String,
    pub target: Option<String>,
    pub status: String,
    pub milestones: Vec<MilestoneIo>,
}

/// A milestone in both directions. The two halves were identical structs that
/// differed only in which serde trait they derived.
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MilestoneIo {
    pub done: bool,
    pub title: String,
}

impl From<&Milestone> for MilestoneIo {
    fn from(milestone: &Milestone) -> Self {
        Self {
            done: milestone.done,
            title: milestone.title().to_owned(),
        }
    }
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReportBucket {
    /// Project slug or date, depending on the grouping. Null means no project.
    pub key: Option<String>,
    pub tracked: String,
    /// What the schedule set aside for this key. A bucket can have this and no
    /// tracked time at all.
    pub planned: String,
    pub sessions: u32,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ReportSummary {
    pub from: String,
    pub to: String,
    pub group_by: String,
    pub total: String,
    /// Everything scheduled over the range; `total` is what was tracked.
    pub planned: String,
    pub buckets: Vec<ReportBucket>,
}

#[tool_router]
impl TimeMd {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "start_session",
        description = "Start a focus session, logging whatever was already running."
    )]
    fn start_session(
        &self,
        Parameters(params): Parameters<StartParams>,
    ) -> Result<Json<Outcome>, ErrorData> {
        let request = StartRequest {
            kind: SessionKind::Focus,
            duration: params.duration.as_deref().map(minutes).transpose()?,
            project: params.project.as_deref().map(slug).transpose()?,
            note: params.note.unwrap_or_default(),
        };

        let active = Timer::new(&self.store)
            .start(self.now()?, request)
            .map_err(failed)?;

        Ok(Json(Outcome {
            message: format!(
                "started {} for {} on {}",
                active.started.format("%H:%M"),
                active.duration,
                active
                    .project
                    .as_ref()
                    .map_or_else(|| "no project".to_owned(), ToString::to_string),
            ),
        }))
    }

    #[tool(
        name = "stop_session",
        description = "Stop the running session and log the time worked."
    )]
    fn stop_session(&self, _params: Parameters<NoParams>) -> Result<Json<Outcome>, ErrorData> {
        let message = match Timer::new(&self.store).stop(self.now()?).map_err(failed)? {
            Stopped::Logged(session) => format!(
                "logged {} to {}",
                session.duration(),
                session
                    .project
                    .as_ref()
                    .map_or_else(|| "no project".to_owned(), ToString::to_string),
            ),
            Stopped::TooShort => "stopped; under a minute, so nothing was logged".to_owned(),
            Stopped::Idle => "nothing was running".to_owned(),
        };
        Ok(Json(Outcome { message }))
    }

    #[tool(
        name = "cancel_session",
        description = "Discard the running session without logging it."
    )]
    fn cancel_session(&self, _params: Parameters<NoParams>) -> Result<Json<Outcome>, ErrorData> {
        let discarded = Timer::new(&self.store).cancel().map_err(failed)?;
        Ok(Json(Outcome {
            message: if discarded {
                "discarded".to_owned()
            } else {
                "nothing was running".to_owned()
            },
        }))
    }

    #[tool(
        name = "current_session",
        description = "What is running right now, and how much has been tracked today."
    )]
    fn current_session(
        &self,
        _params: Parameters<NoParams>,
    ) -> Result<Json<CurrentSession>, ErrorData> {
        let now = self.now()?;
        let state = Timer::new(&self.store).state(now).map_err(failed)?;
        let active = state.active.as_ref();

        Ok(Json(CurrentSession {
            running: active.is_some(),
            kind: active.map(|active| active.kind.to_string()),
            project: active.and_then(|active| active.project.as_ref().map(ToString::to_string)),
            note: active.map(|active| active.note.clone()),
            started_at: active.map(|active| active.started.format("%Y-%m-%dT%H:%M").to_string()),
            remaining: active.map(|active| active.remaining(now).to_string()),
            tracked_today: state.tracked_today.to_string(),
            completed_today: state.completed_today,
        }))
    }

    #[tool(
        name = "log_time",
        description = "Record time that was not tracked live."
    )]
    fn log_time(
        &self,
        Parameters(params): Parameters<LogParams>,
    ) -> Result<Json<Outcome>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let session = Session::new(
            time(&params.start)?,
            time(&params.end)?,
            params.project.as_deref().map(slug).transpose()?,
            params.note.unwrap_or_default(),
        );
        let logged = session.duration();

        self.store
            .update_day(on, |day| day.add_session(session))
            .map_err(failed)?;

        Ok(Json(Outcome {
            message: format!("logged {logged} on {on}"),
        }))
    }

    #[tool(
        name = "day",
        description = "A day's tracked sessions and planned blocks."
    )]
    fn day(
        &self,
        Parameters(params): Parameters<DateParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let day = self.store.read_day(on).map_err(failed)?;
        let recurring = self.store.read_recurring().map_err(failed)?;

        Ok(Json(summarise_day(&day, &recurring)))
    }

    #[tool(
        name = "edit_session",
        description = "Amend a logged session. Only the fields given change. Answers with the whole day, renumbered."
    )]
    fn edit_session(
        &self,
        Parameters(params): Parameters<EditSessionParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        // Every conversion before the store is touched, so a rejected value
        // cannot leave the day half-updated.
        let start = params.start.as_deref().map(time).transpose()?;
        let end = params.end.as_deref().map(time).transpose()?;
        let project = clearable(params.project.as_deref(), slug)?;

        self.edit_day(on, |day| {
            let existing =
                day.sessions().get(params.index).cloned().ok_or_else(|| {
                    invalid(format!("no session at index {} on {on}", params.index))
                })?;

            day.replace_session(
                params.index,
                Session::new(
                    start.unwrap_or(existing.start),
                    end.unwrap_or(existing.end),
                    project.unwrap_or(existing.project),
                    params.note.unwrap_or(existing.note),
                ),
            );
            Ok(())
        })
    }

    #[tool(
        name = "delete_session",
        description = "Remove a logged session. Answers with the whole day, renumbered."
    )]
    fn delete_session(
        &self,
        Parameters(params): Parameters<IndexParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;

        self.edit_day(on, |day| {
            day.remove_session(params.index)
                .map(|_| ())
                .ok_or_else(|| invalid(format!("no session at index {} on {on}", params.index)))
        })
    }

    #[tool(name = "schedule", description = "Planned blocks over a date range.")]
    fn schedule(
        &self,
        Parameters(params): Parameters<RangeParams>,
    ) -> Result<Json<Schedule>, ErrorData> {
        let occurrences =
            planned_range(&self.store, range(&params.from, &params.to)?).map_err(failed)?;
        Ok(Json(Schedule {
            blocks: blocks(&occurrences),
        }))
    }

    #[tool(
        name = "report",
        description = "Tracked and scheduled time over a range."
    )]
    fn report(
        &self,
        Parameters(params): Parameters<ReportParams>,
    ) -> Result<Json<ReportSummary>, ErrorData> {
        let range = range(&params.from, &params.to)?;
        let grouping: GroupBy = params
            .group_by
            .as_deref()
            .unwrap_or("project")
            .parse()
            .map_err(failed)?;

        let summary = report::build(&self.store, range, grouping).map_err(failed)?;

        Ok(Json(ReportSummary {
            from: summary.from.to_string(),
            to: summary.to.to_string(),
            group_by: summary.group_by.to_string(),
            total: summary.total.to_string(),
            planned: summary.planned.to_string(),
            buckets: summary
                .buckets
                .iter()
                .map(|bucket| ReportBucket {
                    key: bucket.key.clone(),
                    tracked: bucket.tracked.to_string(),
                    planned: bucket.planned.to_string(),
                    sessions: bucket.sessions,
                })
                .collect(),
        }))
    }

    #[tool(
        name = "list_projects",
        description = "Every project, ordered by slug."
    )]
    fn list_projects(&self, _params: Parameters<NoParams>) -> Result<Json<ProjectList>, ErrorData> {
        let projects = self.store.list_projects().map_err(failed)?;
        Ok(Json(ProjectList {
            projects: projects.iter().map(summarise).collect(),
        }))
    }

    #[tool(name = "project", description = "One project, with its milestones.")]
    fn project(
        &self,
        Parameters(params): Parameters<SlugParams>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        let slug = slug(&params.slug)?;
        let project = self
            .store
            .read_project(&slug)
            .map_err(failed)?
            .ok_or_else(|| invalid(format!("no project named {slug:?}")))?;
        Ok(Json(summarise(&project)))
    }

    #[tool(
        name = "delete_project",
        description = "Delete a project file. Sessions already logged against it keep their link."
    )]
    fn delete_project(
        &self,
        Parameters(params): Parameters<SlugParams>,
    ) -> Result<Json<Outcome>, ErrorData> {
        let slug = slug(&params.slug)?;
        if self.store.delete_project(&slug).map_err(failed)? {
            Ok(Json(Outcome {
                message: format!("deleted {slug}"),
            }))
        } else {
            Err(invalid(format!("no project named {slug:?}")))
        }
    }

    #[tool(
        name = "upsert_project",
        description = "Create a project, or update only the fields given on an existing one."
    )]
    fn upsert_project(
        &self,
        Parameters(params): Parameters<UpsertProjectParams>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        let slug = slug(&params.slug)?;
        let color = params.color.as_deref().map(colour).transpose()?;
        let mark = params
            .mark
            .as_deref()
            .map(|raw| raw.parse::<Mark>().map_err(failed))
            .transpose()?;
        let target = params.target.as_deref().map(minutes).transpose()?;
        let status = params
            .status
            .as_deref()
            .map(|raw| raw.parse::<ProjectStatus>().map_err(failed))
            .transpose()?;
        let milestones = params
            .milestones
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| {
                        Milestone::new(entry.done, entry.title)
                            .map_err(|error| invalid(error.to_string()))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let today = self.now()?.date();

        // One transaction, so the file is read once and cannot be created
        // between the existence check and the write.
        let summary = self
            .store
            .transaction(|tx| {
                let existing = tx.read_project(&slug)?;
                let mut project = existing.unwrap_or_else(|| {
                    Project::new(
                        slug.clone(),
                        params.name.clone().unwrap_or_else(|| slug.to_string()),
                        today,
                    )
                });

                if let Some(name) = params.name {
                    project.name = name;
                }
                if color.is_some() {
                    project.color = color;
                }
                if let Some(mark) = mark {
                    project.mark = mark;
                }
                if target.is_some() {
                    project.target = target;
                }
                if let Some(status) = status {
                    project.status = status;
                }
                if let Some(milestones) = milestones {
                    project.set_milestones(milestones)?;
                }

                tx.write_project(&project)?;
                Ok(summarise(&project))
            })
            .map_err(failed)?;

        Ok(Json(summary))
    }

    #[tool(
        name = "add_milestone",
        description = "Add a milestone to a project. Titles address milestones, so they must be unique within one."
    )]
    fn add_milestone(
        &self,
        Parameters(params): Parameters<AddMilestoneParams>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        let slug = slug(&params.project)?;
        let milestone = Milestone::new(params.done.unwrap_or_default(), &params.title)
            .map_err(|error| invalid(error.to_string()))?;

        self.edit_project(&slug, |project| {
            project
                .insert_milestone(params.position.unwrap_or(usize::MAX), milestone)
                .map(|_| ())
                .map_err(|error| invalid(error.to_string()))
        })
    }

    #[tool(
        name = "update_milestone",
        description = "Tick, retitle or reorder one milestone, named by its current title."
    )]
    fn update_milestone(
        &self,
        Parameters(params): Parameters<UpdateMilestoneParams>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        let slug = slug(&params.project)?;

        self.edit_project(&slug, |project| {
            project
                .update_milestone(
                    &params.title,
                    MilestoneEdit {
                        done: params.done,
                        title: params.new_title,
                        position: params.position,
                    },
                )
                .map(|_| ())
                .map_err(|error| invalid(error.to_string()))
        })
    }

    #[tool(
        name = "remove_milestone",
        description = "Delete one milestone, named by its title."
    )]
    fn remove_milestone(
        &self,
        Parameters(params): Parameters<MilestoneParams>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        let slug = slug(&params.project)?;

        self.edit_project(&slug, |project| {
            project
                .remove_milestone(&params.title)
                .map(|_| ())
                .map_err(|error| invalid(error.to_string()))
        })
    }

    /// Reads, edits and writes a project inside one transaction, answering with
    /// the whole project.
    ///
    /// One transaction because the alternative — read with one tool, write with
    /// another — is the two-call race that made ticking a milestone unsafe. The
    /// whole project comes back because an edit may reorder the list, and the
    /// agent's next call has to be made against what is now on disk.
    fn edit_project(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> Result<(), ErrorData>,
    ) -> Result<Json<ProjectSummary>, ErrorData> {
        // `try_update_project` gives back the closure's value, so the inner
        // error travels out rather than being swallowed by the store's own
        // type — and leaves the file alone when that value is one.
        self.store
            .try_update_project(slug, |project| {
                edit(project)?;
                Ok(Json(summarise(project)))
            })
            .map_err(failed)?
    }

    #[tool(
        name = "settings",
        description = "Read the pomodoro lengths and reminder default. Give a field to change it; give none to just read."
    )]
    fn settings(
        &self,
        Parameters(params): Parameters<SettingsParams>,
    ) -> Result<Json<SettingsSummary>, ErrorData> {
        let patch = SettingsPatch {
            focus: params.focus.as_deref().map(minutes).transpose()?,
            short_break: params.short_break.as_deref().map(minutes).transpose()?,
            long_break: params.long_break.as_deref().map(minutes).transpose()?,
            remind_before: params.remind_before.as_deref().map(minutes).transpose()?,
        };

        // A pure read must not write: an agent asking what the lengths are
        // should not dirty a git-tracked tree.
        if patch.is_empty() {
            let settings = self.store.read_settings().map_err(failed)?;
            return Ok(Json(summarise_settings(&settings)));
        }

        let summary = self
            .store
            .try_update_settings(|settings| {
                settings.apply(patch)?;
                Ok(summarise_settings(settings))
            })
            .map_err(failed)?
            .map_err(|error: timemd_core::Error| invalid(error.to_string()))?;

        Ok(Json(summary))
    }

    #[tool(
        name = "recurring",
        description = "The weekly repeating schedule, as stored."
    )]
    fn recurring(&self, _params: Parameters<NoParams>) -> Result<Json<RecurringList>, ErrorData> {
        let recurring = self.store.read_recurring().map_err(failed)?;
        Ok(Json(summarise_recurring(&recurring)))
    }

    #[tool(
        name = "set_recurring_block",
        description = "Create or replace one repeating block, keyed on its id. Leaves every other block alone."
    )]
    fn set_recurring_block(
        &self,
        Parameters(params): Parameters<RecurringBlockParams>,
    ) -> Result<Json<RecurringList>, ErrorData> {
        let block = RecurringBlock {
            id: block_id(&params.id)?,
            // Joined and handed to core's parser, so there is one definition of
            // what a day spec means. An empty list is refused there, which is
            // right: a block on no days would never fire.
            days: params
                .days
                .join(",")
                .parse()
                .map_err(|error: timemd_core::ParseErrorKind| invalid(error.to_string()))?,
            start: time(&params.start)?,
            end: time(&params.end)?,
            project: params.project.as_deref().map(slug).transpose()?,
            title: params.title.trim().to_owned(),
            remind_before: params.remind_before.as_deref().map(minutes).transpose()?,
        };

        // Keyed on the id rather than replacing the whole list, for the same
        // reason milestones are addressed by title: an agent that had to
        // read-modify-write every block to change one would race the UI.
        let summary = self
            .store
            .update_recurring(|recurring| {
                recurring.upsert(block);
                summarise_recurring(recurring)
            })
            .map_err(failed)?;

        Ok(Json(summary))
    }

    #[tool(
        name = "remove_recurring_block",
        description = "Delete one repeating block by id."
    )]
    fn remove_recurring_block(
        &self,
        Parameters(params): Parameters<BlockIdParams>,
    ) -> Result<Json<RecurringList>, ErrorData> {
        let id = block_id(&params.id)?;
        let removed = self
            .store
            .update_recurring(|recurring| {
                recurring
                    .remove(&id)
                    .then(|| summarise_recurring(recurring))
            })
            .map_err(failed)?;

        removed
            .map(Json)
            .ok_or_else(|| invalid(format!("no repeating block named {id:?}")))
    }

    #[tool(
        name = "add_block",
        description = "Plan a one-off block on a day. Answers with the whole day, renumbered."
    )]
    fn add_block(
        &self,
        Parameters(params): Parameters<BlockParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let block = DayBlock {
            start: time(&params.start)?,
            end: time(&params.end)?,
            project: params.project.as_deref().map(slug).transpose()?,
            title: params.title.trim().to_owned(),
            remind_before: params.remind_before.as_deref().map(minutes).transpose()?,
        };

        self.edit_day(on, |day| {
            day.add_block(block);
            Ok(())
        })
    }

    #[tool(
        name = "edit_block",
        description = "Amend a one-off block. Only the fields given change. Answers with the whole day, renumbered."
    )]
    fn edit_block(
        &self,
        Parameters(params): Parameters<EditBlockParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let start = params.start.as_deref().map(time).transpose()?;
        let end = params.end.as_deref().map(time).transpose()?;
        let project = clearable(params.project.as_deref(), slug)?;
        let remind_before = clearable(params.remind_before.as_deref(), minutes)?;

        self.edit_day(on, |day| {
            let existing =
                day.schedule().get(params.index).cloned().ok_or_else(|| {
                    invalid(format!("no block at index {} on {on}", params.index))
                })?;

            day.replace_block(
                params.index,
                DayBlock {
                    start: start.unwrap_or(existing.start),
                    end: end.unwrap_or(existing.end),
                    project: project.unwrap_or(existing.project),
                    title: params
                        .title
                        .map_or(existing.title, |title| title.trim().to_owned()),
                    remind_before: remind_before.unwrap_or(existing.remind_before),
                },
            );
            Ok(())
        })
    }

    #[tool(
        name = "remove_block",
        description = "Delete a one-off block. Answers with the whole day, renumbered."
    )]
    fn remove_block(
        &self,
        Parameters(params): Parameters<IndexParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;

        self.edit_day(on, |day| {
            day.remove_block(params.index)
                .map(|_| ())
                .ok_or_else(|| invalid(format!("no block at index {} on {on}", params.index)))
        })
    }

    #[tool(
        name = "skip_block",
        description = "Suppress one repeating block on one day, leaving the pattern alone."
    )]
    fn skip_block(
        &self,
        Parameters(params): Parameters<SkipParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let id = block_id(&params.id)?;

        self.edit_day(on, |day| {
            day.skip(id);
            Ok(())
        })
    }

    #[tool(
        name = "unskip_block",
        description = "Restore a repeating block that was skipped on a day."
    )]
    fn unskip_block(
        &self,
        Parameters(params): Parameters<SkipParams>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let on = self.on(params.date.as_deref())?;
        let id = block_id(&params.id)?;

        self.edit_day(on, |day| {
            day.unskip(&id)
                .then_some(())
                .ok_or_else(|| invalid(format!("{id} was not skipped on {on}")))
        })
    }

    /// The date a tool was pointed at, defaulting to today.
    fn on(&self, requested: Option<&str>) -> Result<NaiveDate, ErrorData> {
        match requested {
            Some(raw) => date(raw),
            None => Ok(self.now()?.date()),
        }
    }

    /// Edits a day and answers with all of it, freshly numbered.
    ///
    /// Sessions and one-off blocks are addressed by index and both lists
    /// re-sort on write, so an index the agent is holding may already name
    /// something else. Returning the day means its next handle arrives with the
    /// answer to its last call, and there is no window in between.
    fn edit_day(
        &self,
        on: NaiveDate,
        edit: impl FnOnce(&mut timemd_core::day::Day) -> Result<(), ErrorData>,
    ) -> Result<Json<DaySummary>, ErrorData> {
        let recurring = self.store.read_recurring().map_err(failed)?;
        self.store
            .try_update_day(on, |day| {
                edit(day)?;
                Ok(Json(summarise_day(day, &recurring)))
            })
            .map_err(failed)?
    }

    /// Now, as wall-clock time in the configured timezone.
    fn now(&self) -> Result<NaiveDateTime, ErrorData> {
        self.store.wall_clock(Utc::now()).map_err(failed)
    }
}

/// Instructions the client shows to the agent alongside the tool list.
///
/// Worth spending words on: the tools are only half the interface here, and an
/// agent that knows the files exist can do things no tool exposes.
const INSTRUCTIONS: &str = "\
timemd tracks time in markdown files. Every tool here reads and writes that same
tree, and so can you: projects live in `projects/<slug>.md` (frontmatter plus a
`## Milestones` list of `- [x] Title` lines), tracked time in
`days/YYYY/YYYY-MM-DD.md` under a `## Sessions` list, and repeating schedule
blocks in `schedule/recurring.md`. Editing those files by hand is supported — the
app re-reads them on the next request, and anything it does not understand
(prose, your own `##` sections, extra frontmatter keys) is preserved untouched.

Times are local wall-clock with no offsets; the timezone lives in `settings.md`.
A session whose end is earlier than its start crossed midnight.";

// Points at the stored router so it is built once, at construction, rather
// than rebuilt on every tools/list and every call.
#[tool_handler(router = self.tool_router)]
impl ServerHandler for TimeMd {
    fn get_info(&self) -> ServerInfo {
        // Both types are `#[non_exhaustive]`, so they are built from their
        // defaults rather than a struct literal.
        let mut implementation = Implementation::default();
        implementation.name = "timemd".to_owned();
        implementation.version = env!("CARGO_PKG_VERSION").to_owned();

        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = implementation;
        info.instructions = Some(INSTRUCTIONS.to_owned());
        info
    }
}

/// Serves the protocol on stdio until the client disconnects.
pub async fn serve(store: Arc<Store>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use rmcp::ServiceExt;

    let service = TimeMd::new(store)
        .serve(rmcp::transport::io::stdio())
        .await?;
    service.waiting().await?;
    Ok(())
}

fn summarise(project: &Project) -> ProjectSummary {
    ProjectSummary {
        slug: project.slug().to_string(),
        name: project.name.clone(),
        color: project.color.as_ref().map(ToString::to_string),
        mark: project.mark.to_string(),
        target: project.target.map(|target| target.to_string()),
        status: project.status.to_string(),
        milestones: project.milestones.iter().map(MilestoneIo::from).collect(),
    }
}

fn summarise_settings(settings: &Settings) -> SettingsSummary {
    SettingsSummary {
        timezone: settings.timezone.to_string(),
        focus: settings.focus.to_string(),
        short_break: settings.short_break.to_string(),
        long_break: settings.long_break.to_string(),
        long_break_every: settings.long_break_every,
        remind_before: settings.remind_before.to_string(),
    }
}

fn summarise_recurring(recurring: &Recurring) -> RecurringList {
    RecurringList {
        blocks: recurring
            .blocks()
            .iter()
            .map(|block| RecurringBlockIo {
                id: block.id.to_string(),
                days: block
                    .days
                    .names()
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect(),
                start: block.start.format("%H:%M").to_string(),
                end: block.end.format("%H:%M").to_string(),
                project: block.project.as_ref().map(ToString::to_string),
                title: block.title.clone(),
                remind_before: block.remind_before.map(|lead| lead.to_string()),
            })
            .collect(),
        problems: recurring
            .problems()
            .iter()
            .map(ToString::to_string)
            .collect(),
    }
}

/// A whole day, freshly numbered.
///
/// Every tool that writes a session or a block answers with this rather than a
/// message, because both are addressed by index and both lists re-sort on
/// write. Returning the day means the agent's next handle arrives with the
/// answer to its last call, so there is no window in which it holds a stale one.
fn summarise_day(day: &timemd_core::day::Day, recurring: &timemd_core::Recurring) -> DaySummary {
    DaySummary {
        date: day.date().to_string(),
        tracked: day.total().to_string(),
        sessions: day.sessions().iter().enumerate().map(logged).collect(),
        planned: blocks(&planned(day, recurring)),
        skipped: day.skipped().iter().map(ToString::to_string).collect(),
        problems: day.problems().iter().map(ToString::to_string).collect(),
    }
}

fn logged((index, session): (usize, &Session)) -> LoggedSession {
    LoggedSession {
        index,
        start: session.start.format("%H:%M").to_string(),
        end: session.end.format("%H:%M").to_string(),
        duration: session.duration().to_string(),
        project: session.project.as_ref().map(ToString::to_string),
        note: session.note.clone(),
    }
}

fn blocks(occurrences: &[timemd_core::Occurrence]) -> Vec<PlannedBlock> {
    occurrences
        .iter()
        .map(|occurrence| PlannedBlock {
            date: occurrence.date.to_string(),
            start: occurrence.start.format("%H:%M").to_string(),
            end: occurrence.end.format("%H:%M").to_string(),
            duration: occurrence.duration().to_string(),
            project: occurrence.project.as_ref().map(ToString::to_string),
            title: occurrence.title.clone(),
            remind_before: occurrence.remind_before.map(|lead| lead.to_string()),
            block: occurrence.block.as_ref().map(ToString::to_string),
            one_off_index: occurrence.one_off_index,
        })
        .collect()
}

fn failed(error: timemd_core::Error) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

fn invalid(message: String) -> ErrorData {
    ErrorData::invalid_params(message, None)
}

/// Reads a field that may also be given empty to mean "clear it".
///
/// An empty string is how a caller says "no project" or "no reminder"; there is
/// no other way to spell it in JSON that omitting the field does not already
/// mean. The outer `Option` is "was the field given", the inner one the value.
///
/// One helper rather than one per field: `crates/server/src/parse.rs` records
/// what happened last time this was written out per caller.
fn clearable<T>(
    raw: Option<&str>,
    parse: impl FnOnce(&str) -> Result<T, ErrorData>,
) -> Result<Option<Option<T>>, ErrorData> {
    match raw {
        None => Ok(None),
        Some("") => Ok(Some(None)),
        Some(value) => parse(value).map(|parsed| Some(Some(parsed))),
    }
}

fn slug(raw: &str) -> Result<ProjectSlug, ErrorData> {
    ProjectSlug::new(raw).map_err(|error| invalid(error.to_string()))
}

fn block_id(raw: &str) -> Result<BlockId, ErrorData> {
    BlockId::new(raw).map_err(|error| invalid(error.to_string()))
}

fn colour(raw: &str) -> Result<Color, ErrorData> {
    Color::new(raw).map_err(|error| invalid(error.to_string()))
}

fn minutes(raw: &str) -> Result<Minutes, ErrorData> {
    raw.parse()
        .map_err(|error: timemd_core::ParseErrorKind| invalid(error.to_string()))
}

fn date(raw: &str) -> Result<NaiveDate, ErrorData> {
    raw.parse()
        .map_err(|_| invalid(format!("invalid date {raw:?}; expected YYYY-MM-DD")))
}

fn time(raw: &str) -> Result<NaiveTime, ErrorData> {
    NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(raw, "%H:%M:%S"))
        .map_err(|_| invalid(format!("invalid time {raw:?}; expected HH:MM")))
}

fn range(from: &str, to: &str) -> Result<DateRange, ErrorData> {
    DateRange::new(date(from)?, date(to)?).map_err(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server() -> (tempfile::TempDir, TimeMd) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Arc::new(Store::new(directory.path()));
        store
            .update_settings(|settings| settings.timezone = chrono_tz::UTC)
            .expect("writes settings");
        (directory, TimeMd::new(store))
    }

    fn log(server: &TimeMd, on: &str, start: &str, end: &str, project: Option<&str>) {
        server
            .log_time(Parameters(LogParams {
                date: Some(on.to_owned()),
                start: start.to_owned(),
                end: end.to_owned(),
                project: project.map(ToOwned::to_owned),
                note: Some("work".to_owned()),
            }))
            .expect("logs");
    }

    #[test]
    fn the_server_identifies_itself_and_points_at_the_files() {
        let (_directory, server) = server();
        let info = server.get_info();

        assert_eq!(info.server_info.name, "timemd");
        assert_eq!(info.server_info.version, env!("CARGO_PKG_VERSION"));

        let instructions = info.instructions.expect("instructions are the point");
        assert!(instructions.contains("days/YYYY"), "{instructions}");
        assert!(instructions.contains("settings.md"), "{instructions}");
    }

    #[test]
    fn every_tool_is_advertised() {
        let (_directory, server) = server();
        let names: Vec<String> = server
            .tool_router
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();

        for expected in [
            "start_session",
            "stop_session",
            "cancel_session",
            "current_session",
            "log_time",
            "edit_session",
            "delete_session",
            "day",
            "schedule",
            "recurring",
            "set_recurring_block",
            "remove_recurring_block",
            "add_block",
            "edit_block",
            "remove_block",
            "skip_block",
            "unskip_block",
            "settings",
            "report",
            "list_projects",
            "project",
            "upsert_project",
            "delete_project",
            "add_milestone",
            "update_milestone",
            "remove_milestone",
        ] {
            assert!(
                names.contains(&expected.to_owned()),
                "{expected} is missing from {names:?}"
            );
        }
    }

    /// A tool that answers with a bare array is rejected by the client at
    /// connect time, taking every other tool down with it.
    #[test]
    fn every_result_schema_describes_an_object() {
        let (_directory, server) = server();

        for tool in server.tool_router.list_all() {
            let schema = tool.output_schema.as_ref().expect("a result schema");
            assert_eq!(
                schema.get("type").and_then(serde_json::Value::as_str),
                Some("object"),
                "{} answers with a {schema:?}",
                tool.name
            );
        }
    }

    /// Asking what the lengths are must not dirty a git-tracked tree, so a call
    /// with no field given reads and does not write.
    #[test]
    fn reading_settings_does_not_write_the_file() {
        let (_directory, server) = server();
        let path = server.store.settings_path();
        let before = std::fs::read(&path).expect("settings exist");

        let summary = server
            .settings(Parameters(SettingsParams::default()))
            .expect("reads")
            .0;
        assert_eq!(summary.focus, "25m");
        assert_eq!(summary.timezone, "UTC");
        assert_eq!(summary.long_break_every, 4);

        assert_eq!(
            std::fs::read(&path).expect("settings exist"),
            before,
            "a pure read must not rewrite the file"
        );

        let summary = server
            .settings(Parameters(SettingsParams {
                focus: Some("50m".to_owned()),
                ..SettingsParams::default()
            }))
            .expect("writes")
            .0;
        assert_eq!(summary.focus, "50m");
        assert_eq!(summary.short_break, "5m", "omitted fields stay put");
    }

    /// `Settings::parse` falls back when it reads a zero, so writing one would
    /// leave the file saying one thing and the timer doing another.
    #[test]
    fn a_zero_session_length_is_rejected_and_nothing_is_written() {
        let (_directory, server) = server();
        let path = server.store.settings_path();
        let before = std::fs::read(&path).expect("settings exist");

        let error = server
            .settings(Parameters(SettingsParams {
                focus: Some("0m".to_owned()),
                ..SettingsParams::default()
            }))
            .err()
            .expect("a zero focus length is refused");
        assert!(error.message.contains("focus"), "{}", error.message);

        assert_eq!(std::fs::read(&path).expect("settings exist"), before);
    }

    /// Keyed on the id, not on position: an agent that had to read-modify-write
    /// the whole pattern to change one block would race the web editor.
    #[test]
    fn a_recurring_block_is_upserted_by_id_and_removed_by_id() {
        let (_directory, server) = server();

        let block = |id: &str, title: &str, start: &str| RecurringBlockParams {
            id: id.to_owned(),
            days: vec!["mon-fri".to_owned()],
            start: start.to_owned(),
            end: "11:00".to_owned(),
            title: title.to_owned(),
            project: None,
            remind_before: Some("5m".to_owned()),
        };

        server
            .set_recurring_block(Parameters(block("deep-work", "Deep work", "09:00")))
            .expect("creates");
        let list = server
            .set_recurring_block(Parameters(block("standup", "Standup", "08:00")))
            .expect("creates")
            .0;

        assert_eq!(list.blocks.len(), 2);
        assert_eq!(list.blocks[0].days, ["mon", "tue", "wed", "thu", "fri"]);

        // Setting an existing id replaces that block and leaves the other one.
        let list = server
            .set_recurring_block(Parameters(block("deep-work", "Focus", "10:00")))
            .expect("replaces")
            .0;
        assert_eq!(list.blocks.len(), 2);
        let deep = list
            .blocks
            .iter()
            .find(|block| block.id == "deep-work")
            .expect("still there");
        assert_eq!(deep.title, "Focus");
        assert_eq!(deep.start, "10:00");

        let list = server
            .remove_recurring_block(Parameters(BlockIdParams {
                id: "deep-work".to_owned(),
            }))
            .expect("removes")
            .0;
        assert_eq!(list.blocks.len(), 1);
        assert_eq!(list.blocks[0].id, "standup");

        assert!(
            server
                .remove_recurring_block(Parameters(BlockIdParams {
                    id: "deep-work".to_owned(),
                }))
                .is_err()
        );
    }

    #[test]
    fn a_one_off_block_can_be_planned_amended_and_removed() {
        let (_directory, server) = server();
        let on = || Some("2026-08-05".to_owned());

        let day = server
            .add_block(Parameters(BlockParams {
                date: on(),
                start: "12:00".to_owned(),
                end: "12:30".to_owned(),
                title: "Lunch".to_owned(),
                project: None,
                remind_before: None,
            }))
            .expect("plans")
            .0;
        assert_eq!(day.planned[0].title, "Lunch");
        assert_eq!(day.planned[0].one_off_index, Some(0));

        let day = server
            .edit_block(Parameters(EditBlockParams {
                date: on(),
                index: 0,
                end: Some("13:00".to_owned()),
                title: Some("Long lunch".to_owned()),
                ..EditBlockParams::default()
            }))
            .expect("edits")
            .0;
        assert_eq!(day.planned[0].title, "Long lunch");
        assert_eq!(day.planned[0].start, "12:00", "omitted fields stay put");
        assert_eq!(day.planned[0].end, "13:00");

        let day = server
            .remove_block(Parameters(IndexParams {
                date: on(),
                index: 0,
            }))
            .expect("removes")
            .0;
        assert!(day.planned.is_empty());
    }

    /// Skipping suppresses one occurrence without touching the pattern, so the
    /// same block still appears on every other day it falls on.
    #[test]
    fn a_repeat_is_skipped_on_one_day_and_restored() {
        let (_directory, server) = server();
        server
            .set_recurring_block(Parameters(RecurringBlockParams {
                id: "deep-work".to_owned(),
                days: vec!["mon-fri".to_owned()],
                start: "09:00".to_owned(),
                end: "11:00".to_owned(),
                title: "Deep work".to_owned(),
                project: None,
                remind_before: None,
            }))
            .expect("creates");

        let skip = || SkipParams {
            date: Some("2026-08-05".to_owned()),
            id: "deep-work".to_owned(),
        };

        let day = server.skip_block(Parameters(skip())).expect("skips").0;
        assert!(day.planned.is_empty());
        assert_eq!(day.skipped, vec!["deep-work".to_owned()]);

        // Thursday is untouched: the pattern itself did not change.
        let thursday = server
            .day(Parameters(DateParams {
                date: Some("2026-08-06".to_owned()),
            }))
            .expect("reads")
            .0;
        assert_eq!(thursday.planned.len(), 1);

        let day = server.unskip_block(Parameters(skip())).expect("restores").0;
        assert_eq!(day.planned.len(), 1);
        assert!(day.skipped.is_empty());

        assert!(server.unskip_block(Parameters(skip())).is_err());
    }

    /// An agent that mis-logged time had to hand-edit the markdown, which is
    /// what the server instructions told it to do because nothing else could.
    #[test]
    fn a_logged_session_can_be_amended_field_by_field() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", Some("timemd"));

        let day = server
            .edit_session(Parameters(EditSessionParams {
                date: Some("2026-08-05".to_owned()),
                index: 0,
                end: Some("10:00".to_owned()),
                ..EditSessionParams::default()
            }))
            .expect("edits")
            .0;

        // Only `end` was given, so everything else is as it was.
        assert_eq!(day.sessions[0].start, "09:00");
        assert_eq!(day.sessions[0].end, "10:00");
        assert_eq!(day.sessions[0].duration, "1h");
        assert_eq!(day.sessions[0].project.as_deref(), Some("timemd"));
        assert_eq!(day.sessions[0].note, "work");
        assert_eq!(day.tracked, "1h");
    }

    /// There is no other way to spell "no project" in JSON that an omitted key
    /// does not already mean.
    #[test]
    fn an_empty_project_clears_a_session_tag() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", Some("timemd"));

        let day = server
            .edit_session(Parameters(EditSessionParams {
                date: Some("2026-08-05".to_owned()),
                index: 0,
                project: Some(String::new()),
                ..EditSessionParams::default()
            }))
            .expect("edits")
            .0;
        assert_eq!(day.sessions[0].project, None);
    }

    /// The reason every session tool answers with the whole day: moving a start
    /// time re-sorts it, so the index the agent just used now names something
    /// else. The fresh numbering arrives with the answer to the call that
    /// invalidated it.
    #[test]
    fn editing_a_session_answers_with_the_re_sorted_day() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", Some("timemd"));
        log(&server, "2026-08-05", "10:30", "10:45", None);

        let day = server
            .edit_session(Parameters(EditSessionParams {
                date: Some("2026-08-05".to_owned()),
                index: 0,
                start: Some("14:00".to_owned()),
                end: Some("14:30".to_owned()),
                ..EditSessionParams::default()
            }))
            .expect("edits")
            .0;

        assert_eq!(day.sessions[0].index, 0);
        assert_eq!(day.sessions[0].start, "10:30");
        assert_eq!(day.sessions[1].index, 1);
        assert_eq!(day.sessions[1].start, "14:00");
    }

    #[test]
    fn a_logged_session_can_be_deleted() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", Some("timemd"));
        log(&server, "2026-08-05", "10:30", "10:45", None);

        let day = server
            .delete_session(Parameters(IndexParams {
                date: Some("2026-08-05".to_owned()),
                index: 0,
            }))
            .expect("deletes")
            .0;

        assert_eq!(day.sessions.len(), 1);
        assert_eq!(day.sessions[0].start, "10:30");
        assert_eq!(day.sessions[0].index, 0);
        assert_eq!(day.tracked, "15m");
    }

    #[test]
    fn addressing_a_session_that_is_not_there_is_refused() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", None);

        for attempt in [
            server
                .edit_session(Parameters(EditSessionParams {
                    date: Some("2026-08-05".to_owned()),
                    index: 7,
                    note: Some("nowhere".to_owned()),
                    ..EditSessionParams::default()
                }))
                .err(),
            server
                .delete_session(Parameters(IndexParams {
                    date: Some("2026-08-05".to_owned()),
                    index: 7,
                }))
                .err(),
        ] {
            let error = attempt.expect("no session there");
            assert!(error.message.contains('7'), "{}", error.message);
        }
    }

    fn thesis(server: &TimeMd, titles: &[&str]) {
        server
            .upsert_project(Parameters(UpsertProjectParams {
                slug: "thesis".to_owned(),
                name: Some("Thesis".to_owned()),
                milestones: Some(
                    titles
                        .iter()
                        .map(|title| MilestoneIo {
                            done: false,
                            title: (*title).to_owned(),
                        })
                        .collect(),
                ),
                ..UpsertProjectParams::default()
            }))
            .expect("creates");
    }

    fn milestone_titles(summary: &ProjectSummary) -> Vec<&str> {
        summary
            .milestones
            .iter()
            .map(|milestone| milestone.title.as_str())
            .collect()
    }

    /// Ticking one milestone used to mean reading every project and writing the
    /// whole list back, so a concurrent hand edit was silently clobbered. One
    /// call, one transaction, addressed by the title the agent already has.
    #[test]
    fn a_milestone_is_ticked_by_title_in_one_call() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1", "Ch. 2"]);

        let summary = server
            .update_milestone(Parameters(UpdateMilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 1".to_owned(),
                done: Some(true),
                ..UpdateMilestoneParams::default()
            }))
            .expect("ticks")
            .0;

        assert!(summary.milestones[0].done);
        assert!(!summary.milestones[1].done);
    }

    #[test]
    fn a_milestone_is_added_renamed_reordered_and_removed() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1", "Ch. 2"]);

        let summary = server
            .add_milestone(Parameters(AddMilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 0".to_owned(),
                position: Some(0),
                done: None,
            }))
            .expect("adds")
            .0;
        assert_eq!(milestone_titles(&summary), ["Ch. 0", "Ch. 1", "Ch. 2"]);

        // Retitling and reordering in one call, so the two cannot race.
        let summary = server
            .update_milestone(Parameters(UpdateMilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 0".to_owned(),
                new_title: Some("Preface".to_owned()),
                position: Some(2),
                done: None,
            }))
            .expect("updates")
            .0;
        assert_eq!(milestone_titles(&summary), ["Ch. 1", "Ch. 2", "Preface"]);

        let summary = server
            .remove_milestone(Parameters(MilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 2".to_owned(),
            }))
            .expect("removes")
            .0;
        assert_eq!(milestone_titles(&summary), ["Ch. 1", "Preface"]);
    }

    /// The summary is only worth trusting if it is what landed on disk.
    #[test]
    fn reordering_a_milestone_reorders_the_file() {
        let (_directory, server) = server();
        thesis(&server, &["a", "b", "c"]);

        server
            .update_milestone(Parameters(UpdateMilestoneParams {
                project: "thesis".to_owned(),
                title: "a".to_owned(),
                position: Some(2),
                ..UpdateMilestoneParams::default()
            }))
            .expect("moves");

        let project = server
            .store
            .read_project(&ProjectSlug::new("thesis").expect("a slug"))
            .expect("reads")
            .expect("exists");
        let titles: Vec<&str> = project.milestones.iter().map(Milestone::title).collect();
        assert_eq!(titles, ["b", "c", "a"]);
    }

    /// Reads are lenient, so a hand-written duplicate lists fine. Writes are
    /// strict, so addressing one is refused rather than resolved by guessing —
    /// and the file is left exactly as it was.
    #[test]
    fn a_hand_written_duplicate_title_lists_but_is_not_addressable() {
        let (_directory, server) = server();
        let slug = ProjectSlug::new("thesis").expect("a slug");
        thesis(&server, &["Ch. 4"]);

        // Appended to the file, not pushed onto the list: every door in refuses
        // a repeated title, so the only way to get one is the way the test says
        // it happened — a hand edit.
        let path = server.store.project_path(&slug);
        let text = std::fs::read_to_string(&path).expect("reads");
        std::fs::write(&path, format!("{text}- [x] Ch. 4\n")).expect("writes");

        let summary = server
            .project(Parameters(SlugParams {
                slug: "thesis".to_owned(),
            }))
            .expect("reads")
            .0;
        assert_eq!(milestone_titles(&summary), ["Ch. 4", "Ch. 4"]);

        let error = server
            .update_milestone(Parameters(UpdateMilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 4".to_owned(),
                done: Some(true),
                ..UpdateMilestoneParams::default()
            }))
            .err()
            .expect("ambiguous");
        assert!(error.message.contains('2'), "{}", error.message);

        let after = server
            .store
            .read_project(&slug)
            .expect("reads")
            .expect("exists");
        assert!(!after.milestones[0].done, "the file must be untouched");
    }

    /// The refusal an agent is most likely to hit: tick and retitle in one
    /// call, onto a title another milestone already carries. The tick is
    /// applied by assignment, so before this was fixed the tool answered with
    /// an error and the file came back ticked anyway.
    #[test]
    fn a_refused_rename_leaves_the_tick_out_of_the_file() {
        let (_directory, server) = server();
        let slug = ProjectSlug::new("thesis").expect("a slug");
        thesis(&server, &["Ch. 1", "Ch. 2"]);

        server
            .update_milestone(Parameters(UpdateMilestoneParams {
                project: "thesis".to_owned(),
                title: "Ch. 2".to_owned(),
                done: Some(true),
                new_title: Some("Ch. 1".to_owned()),
                position: None,
            }))
            .err()
            .expect("the title is taken");

        // Read the text, not the parse: this must fail if either the store's
        // suppression or `update_milestone`'s ordering is removed.
        let text = std::fs::read_to_string(server.store.project_path(&slug)).expect("reads");
        assert!(text.contains("- [ ] Ch. 2"), "{text}");
        assert!(!text.contains("- [x]"), "{text}");
    }

    /// A title nobody can address is a title nobody can edit, so writing a
    /// second one is refused — by `add_milestone` and by a rename alike.
    #[test]
    fn a_duplicate_title_is_refused_on_the_way_in() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1", "Ch. 2"]);

        assert!(
            server
                .add_milestone(Parameters(AddMilestoneParams {
                    project: "thesis".to_owned(),
                    title: "Ch. 1".to_owned(),
                    done: None,
                    position: None,
                }))
                .is_err()
        );
        assert!(
            server
                .update_milestone(Parameters(UpdateMilestoneParams {
                    project: "thesis".to_owned(),
                    title: "Ch. 2".to_owned(),
                    new_title: Some("Ch. 1".to_owned()),
                    ..UpdateMilestoneParams::default()
                }))
                .is_err()
        );

        // Renaming a milestone to what it is already called is not a duplicate.
        assert!(
            server
                .update_milestone(Parameters(UpdateMilestoneParams {
                    project: "thesis".to_owned(),
                    title: "Ch. 2".to_owned(),
                    new_title: Some("Ch. 2".to_owned()),
                    ..UpdateMilestoneParams::default()
                }))
                .is_ok(),
            "renaming a milestone to its own title is a no-op, not a clash"
        );
    }

    #[test]
    fn an_unknown_milestone_title_is_refused() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1"]);

        for attempt in [
            server
                .update_milestone(Parameters(UpdateMilestoneParams {
                    project: "thesis".to_owned(),
                    title: "Ch. 9".to_owned(),
                    done: Some(true),
                    ..UpdateMilestoneParams::default()
                }))
                .err(),
            server
                .remove_milestone(Parameters(MilestoneParams {
                    project: "thesis".to_owned(),
                    title: "Ch. 9".to_owned(),
                }))
                .err(),
        ] {
            let error = attempt.expect("no such milestone");
            assert!(error.message.contains("Ch. 9"), "{}", error.message);
        }
    }

    /// `Milestone::new` is the write-side gate, and a rename is the one path
    /// that could have walked past it.
    #[test]
    fn renaming_rejects_a_title_the_reader_could_not_read() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1"]);

        for candidate in ["", "   ", "two\nlines"] {
            assert!(
                server
                    .update_milestone(Parameters(UpdateMilestoneParams {
                        project: "thesis".to_owned(),
                        title: "Ch. 1".to_owned(),
                        new_title: Some(candidate.to_owned()),
                        ..UpdateMilestoneParams::default()
                    }))
                    .is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    /// The targeted tools are additions, not a replacement: bulk-setting the
    /// whole list is still the right call when an agent is building a project.
    #[test]
    fn upsert_project_still_replaces_the_whole_milestone_list() {
        let (_directory, server) = server();
        thesis(&server, &["Ch. 1", "Ch. 2"]);

        let summary = server
            .upsert_project(Parameters(UpsertProjectParams {
                slug: "thesis".to_owned(),
                milestones: Some(vec![MilestoneIo {
                    done: true,
                    title: "Only this".to_owned(),
                }]),
                ..UpsertProjectParams::default()
            }))
            .expect("updates")
            .0;

        assert_eq!(milestone_titles(&summary), ["Only this"]);
    }

    /// Reading one project meant listing every project and their whole
    /// milestone lists, which is the read half of the same problem that made
    /// editing a milestone a two-call race.
    #[test]
    fn one_project_can_be_read_and_deleted_by_slug() {
        let (_directory, server) = server();
        server
            .upsert_project(Parameters(UpsertProjectParams {
                slug: "thesis".to_owned(),
                name: Some("Thesis".to_owned()),
                ..UpsertProjectParams::default()
            }))
            .expect("creates");

        let project = server
            .project(Parameters(SlugParams {
                slug: "thesis".to_owned(),
            }))
            .expect("reads")
            .0;
        assert_eq!(project.name, "Thesis");

        let outcome = server
            .delete_project(Parameters(SlugParams {
                slug: "thesis".to_owned(),
            }))
            .expect("deletes")
            .0;
        assert!(outcome.message.contains("thesis"), "{}", outcome.message);

        assert!(
            server
                .project(Parameters(SlugParams {
                    slug: "thesis".to_owned(),
                }))
                .is_err()
        );
        assert!(
            server
                .delete_project(Parameters(SlugParams {
                    slug: "thesis".to_owned(),
                }))
                .is_err()
        );
    }

    /// The CLI and the web app could both discard a session; an agent that
    /// started one by mistake had to stop it and then delete what it logged.
    #[test]
    fn a_running_session_can_be_discarded_without_logging_it() {
        let (_directory, server) = server();
        server
            .start_session(Parameters(StartParams::default()))
            .expect("starts");

        let outcome = server
            .cancel_session(Parameters(NoParams {}))
            .expect("cancels")
            .0;
        assert_eq!(outcome.message, "discarded");

        let state = server
            .current_session(Parameters(NoParams {}))
            .expect("reads")
            .0;
        assert!(!state.running);
        assert_eq!(state.tracked_today, "0m");

        let outcome = server
            .cancel_session(Parameters(NoParams {}))
            .expect("cancels")
            .0;
        assert_eq!(outcome.message, "nothing was running");
    }

    /// Sessions have no name, so an index is the only handle there is — and a
    /// day that did not carry one left an agent unable to address the session
    /// it had just read.
    #[test]
    fn a_day_numbers_its_sessions_blocks_and_skips() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "09:25", Some("timemd"));
        log(&server, "2026-08-05", "10:30", "10:45", None);

        server
            .store
            .update_day("2026-08-05".parse().expect("a date"), |day| {
                day.add_block(timemd_core::DayBlock::parse("12:00-12:30 Lunch").expect("a block"));
                day.skip(timemd_core::BlockId::new("deep-work").expect("an id"));
            })
            .expect("writes the day");

        let day = server
            .day(Parameters(DateParams {
                date: Some("2026-08-05".to_owned()),
            }))
            .expect("reads the day")
            .0;

        assert_eq!(day.sessions[0].index, 0);
        assert_eq!(day.sessions[1].index, 1);
        assert_eq!(day.planned[0].one_off_index, Some(0));
        assert_eq!(day.skipped, vec!["deep-work".to_owned()]);
    }

    /// A repeat is addressed by its id and a one-off by its position among the
    /// one-offs, so the two must not be numbered from the same counter.
    #[test]
    fn a_repeat_carries_its_id_and_a_one_off_its_position() {
        let (_directory, server) = server();
        server
            .store
            .update_recurring(|recurring| {
                recurring.upsert(timemd_core::RecurringBlock {
                    id: timemd_core::BlockId::new("deep-work").expect("an id"),
                    days: "mon-fri".parse().expect("a day set"),
                    start: time("09:00").expect("a time"),
                    end: time("11:00").expect("a time"),
                    project: None,
                    title: "Deep work".to_owned(),
                    remind_before: Some(Minutes::new(5)),
                });
            })
            .expect("writes the pattern");
        server
            .store
            .update_day("2026-08-05".parse().expect("a date"), |day| {
                day.add_block(timemd_core::DayBlock::parse("12:00-12:30 Lunch").expect("a block"));
            })
            .expect("writes the day");

        let day = server
            .day(Parameters(DateParams {
                date: Some("2026-08-05".to_owned()),
            }))
            .expect("reads the day")
            .0;

        assert_eq!(day.planned[0].title, "Deep work");
        assert_eq!(day.planned[0].block.as_deref(), Some("deep-work"));
        assert_eq!(day.planned[0].one_off_index, None);
        assert_eq!(day.planned[0].remind_before.as_deref(), Some("5m"));

        assert_eq!(day.planned[1].title, "Lunch");
        assert_eq!(day.planned[1].block, None);
        assert_eq!(day.planned[1].one_off_index, Some(0));
    }

    #[test]
    fn starting_then_reading_reports_the_running_session() {
        let (_directory, server) = server();

        let started = server
            .start_session(Parameters(StartParams {
                project: Some("timemd".to_owned()),
                note: Some("file store".to_owned()),
                duration: Some("50m".to_owned()),
            }))
            .expect("starts");
        assert!(started.0.message.contains("50m"), "{}", started.0.message);
        assert!(
            started.0.message.contains("timemd"),
            "{}",
            started.0.message
        );

        let current = server
            .current_session(Parameters(NoParams {}))
            .expect("reads");
        assert!(current.0.running);
        assert_eq!(current.0.project.as_deref(), Some("timemd"));
        assert_eq!(current.0.kind.as_deref(), Some("focus"));
        assert_eq!(current.0.note.as_deref(), Some("file store"));
    }

    #[test]
    fn stopping_nothing_says_so() {
        let (_directory, server) = server();
        let stopped = server.stop_session(Parameters(NoParams {})).expect("stops");
        assert_eq!(stopped.0.message, "nothing was running");
    }

    #[test]
    fn stopping_straight_away_reports_that_it_was_too_short() {
        let (_directory, server) = server();
        server
            .start_session(Parameters(StartParams::default()))
            .expect("starts");

        let stopped = server.stop_session(Parameters(NoParams {})).expect("stops");
        assert!(
            stopped.0.message.contains("under a minute"),
            "{}",
            stopped.0.message
        );
    }

    #[test]
    fn an_idle_server_reports_an_empty_day() {
        let (_directory, server) = server();
        let current = server
            .current_session(Parameters(NoParams {}))
            .expect("reads");

        assert!(!current.0.running);
        assert_eq!(current.0.tracked_today, "0m");
        assert_eq!(current.0.completed_today, 0);
    }

    #[test]
    fn logging_time_shows_up_in_the_day() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "10:30", Some("timemd"));

        let day = server
            .day(Parameters(DateParams {
                date: Some("2026-08-05".to_owned()),
            }))
            .expect("reads");

        assert_eq!(day.0.tracked, "1h30m");
        assert_eq!(day.0.sessions.len(), 1);
        assert_eq!(day.0.sessions[0].project.as_deref(), Some("timemd"));
        assert_eq!(day.0.sessions[0].start, "09:00");
    }

    #[test]
    fn a_day_reports_lines_it_could_not_read() {
        let (directory, server) = server();
        let path = directory.path().join("days/2026/2026-08-05.md");
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates dir");
        std::fs::write(
            &path,
            "---\ndate: 2026-08-05\n---\n\n## Sessions\n\n- nonsense\n",
        )
        .expect("writes");

        let day = server
            .day(Parameters(DateParams {
                date: Some("2026-08-05".to_owned()),
            }))
            .expect("reads");
        assert_eq!(day.0.problems.len(), 1);
    }

    #[test]
    fn projects_are_created_then_updated_field_by_field() {
        let (_directory, server) = server();

        let created = server
            .upsert_project(Parameters(UpsertProjectParams {
                slug: "timemd".to_owned(),
                name: Some("timemd".to_owned()),
                color: Some("#4f46e5".to_owned()),
                mark: Some("triangle".to_owned()),
                target: Some("10h".to_owned()),
                milestones: Some(vec![MilestoneIo {
                    done: false,
                    title: "Ship it".to_owned(),
                }]),
                status: None,
            }))
            .expect("creates");
        assert_eq!(created.0.status, "active");
        assert_eq!(created.0.color.as_deref(), Some("#4f46e5"));
        assert_eq!(created.0.mark, "triangle");
        assert_eq!(created.0.target.as_deref(), Some("10h"));
        assert_eq!(created.0.milestones[0].title, "Ship it");

        let updated = server
            .upsert_project(Parameters(UpsertProjectParams {
                slug: "timemd".to_owned(),
                status: Some("archived".to_owned()),
                ..UpsertProjectParams::default()
            }))
            .expect("updates");
        assert_eq!(updated.0.status, "archived");
        assert_eq!(
            updated.0.name, "timemd",
            "an omitted field must not be cleared"
        );
        assert_eq!(updated.0.color.as_deref(), Some("#4f46e5"));
        assert_eq!(updated.0.mark, "triangle");
        assert_eq!(updated.0.target.as_deref(), Some("10h"));
        assert_eq!(updated.0.milestones.len(), 1);

        assert_eq!(
            server
                .list_projects(Parameters(NoParams {}))
                .expect("lists")
                .0
                .projects
                .len(),
            1
        );
    }

    #[test]
    fn reports_group_by_project_and_by_day() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "11:00", Some("timemd"));
        log(&server, "2026-08-06", "09:00", "09:30", Some("admin"));

        let by_project = server
            .report(Parameters(ReportParams {
                from: "2026-08-01".to_owned(),
                to: "2026-08-31".to_owned(),
                group_by: None,
            }))
            .expect("reads");
        assert_eq!(by_project.0.total, "2h30m");
        assert_eq!(by_project.0.planned, "0m");
        assert_eq!(by_project.0.group_by, "project");
        assert_eq!(by_project.0.buckets[0].key.as_deref(), Some("timemd"));
        assert_eq!(by_project.0.buckets[0].planned, "0m");

        let by_day = server
            .report(Parameters(ReportParams {
                from: "2026-08-01".to_owned(),
                to: "2026-08-31".to_owned(),
                group_by: Some("day".to_owned()),
            }))
            .expect("reads");
        assert_eq!(by_day.0.buckets[0].key.as_deref(), Some("2026-08-05"));
    }

    #[test]
    fn a_report_carries_the_plan_beside_the_work() {
        let (_directory, server) = server();
        log(&server, "2026-08-05", "09:00", "10:00", Some("timemd"));
        let block =
            timemd_core::DayBlock::parse("09:00-11:00 [[timemd]] Deep work").expect("parses");
        server
            .store
            .update_day(
                NaiveDate::from_ymd_opt(2026, 8, 5).expect("valid date"),
                |day| day.add_block(block),
            )
            .expect("writes");

        let summary = server
            .report(Parameters(ReportParams {
                from: "2026-08-01".to_owned(),
                to: "2026-08-31".to_owned(),
                group_by: None,
            }))
            .expect("reads");

        assert_eq!(summary.0.total, "1h");
        assert_eq!(summary.0.planned, "2h");
        assert_eq!(summary.0.buckets[0].tracked, "1h");
        assert_eq!(summary.0.buckets[0].planned, "2h");
    }

    #[test]
    fn the_schedule_expands_over_a_range() {
        let (_directory, server) = server();
        server
            .store
            .update_recurring(|recurring| {
                recurring.upsert(timemd_core::RecurringBlock {
                    id: timemd_core::BlockId::new("deep-work").expect("valid id"),
                    days: timemd_core::DaySet::ALL,
                    start: NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
                    end: NaiveTime::from_hms_opt(11, 0, 0).expect("valid time"),
                    project: None,
                    title: "Deep work".to_owned(),
                    remind_before: None,
                });
            })
            .expect("writes");

        let blocks = server
            .schedule(Parameters(RangeParams {
                from: "2026-08-05".to_owned(),
                to: "2026-08-07".to_owned(),
            }))
            .expect("reads");

        assert_eq!(blocks.0.blocks.len(), 3);
        assert_eq!(blocks.0.blocks[0].duration, "2h");
        assert_eq!(blocks.0.blocks[0].block.as_deref(), Some("deep-work"));
    }

    #[test]
    fn malformed_input_is_rejected_rather_than_written() {
        let (_directory, server) = server();

        assert!(
            server
                .start_session(Parameters(StartParams {
                    project: Some("Not A Slug".to_owned()),
                    ..StartParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .start_session(Parameters(StartParams {
                    duration: Some("ages".to_owned()),
                    ..StartParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .day(Parameters(DateParams {
                    date: Some("yesterday".to_owned())
                }))
                .is_err()
        );
        assert!(
            server
                .log_time(Parameters(LogParams {
                    start: "9am".to_owned(),
                    end: "10:00".to_owned(),
                    ..LogParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .upsert_project(Parameters(UpsertProjectParams {
                    slug: "timemd".to_owned(),
                    color: Some("blurple".to_owned()),
                    ..UpsertProjectParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .upsert_project(Parameters(UpsertProjectParams {
                    slug: "timemd".to_owned(),
                    status: Some("hibernating".to_owned()),
                    ..UpsertProjectParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .report(Parameters(ReportParams {
                    from: "2026-08-01".to_owned(),
                    to: "2026-08-31".to_owned(),
                    group_by: Some("colour".to_owned()),
                }))
                .is_err()
        );
        assert!(
            server
                .set_recurring_block(Parameters(RecurringBlockParams {
                    id: "Not An Id".to_owned(),
                    days: vec!["mon".to_owned()],
                    start: "09:00".to_owned(),
                    end: "10:00".to_owned(),
                    title: "x".to_owned(),
                    ..RecurringBlockParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .set_recurring_block(Parameters(RecurringBlockParams {
                    id: "deep-work".to_owned(),
                    days: vec!["someday".to_owned()],
                    start: "09:00".to_owned(),
                    end: "10:00".to_owned(),
                    title: "x".to_owned(),
                    ..RecurringBlockParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .add_block(Parameters(BlockParams {
                    start: "elevenish".to_owned(),
                    end: "12:00".to_owned(),
                    title: "x".to_owned(),
                    ..BlockParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .settings(Parameters(SettingsParams {
                    focus: Some("a while".to_owned()),
                    ..SettingsParams::default()
                }))
                .is_err()
        );
        assert!(
            server
                .add_milestone(Parameters(AddMilestoneParams {
                    project: "timemd".to_owned(),
                    title: "   ".to_owned(),
                    ..AddMilestoneParams::default()
                }))
                .is_err()
        );

        // Nothing above should have reached a file.
        assert!(
            server
                .recurring(Parameters(NoParams {}))
                .expect("reads")
                .0
                .blocks
                .is_empty()
        );
        assert_eq!(
            server
                .settings(Parameters(SettingsParams::default()))
                .expect("reads")
                .0
                .focus,
            "25m"
        );
    }

    #[test]
    fn a_backwards_or_oversized_range_is_rejected() {
        assert!(range("2026-08-31", "2026-08-01").is_err());
        assert!(range("2020-01-01", "2026-08-01").is_err());
        assert!(range("2026-08-01", "2026-08-31").is_ok());
        assert!(range("yesterday", "2026-08-31").is_err());
    }

    #[test]
    fn a_time_may_carry_seconds_or_not() {
        let nine = NaiveTime::from_hms_opt(9, 0, 0).expect("valid time");
        assert_eq!(time("09:00").expect("parses"), nine);
        assert_eq!(time("09:00:00").expect("parses"), nine);
        assert!(time("9am").is_err());
    }

    #[test]
    fn omitted_dates_fall_back_to_today() {
        let (_directory, server) = server();
        let today = server.now().expect("reads").date().to_string();

        let day = server
            .day(Parameters(DateParams { date: None }))
            .expect("reads");
        assert_eq!(day.0.date, today);

        server
            .log_time(Parameters(LogParams {
                start: "09:00".to_owned(),
                end: "09:30".to_owned(),
                ..LogParams::default()
            }))
            .expect("logs");
        assert_eq!(
            server
                .day(Parameters(DateParams { date: None }))
                .expect("reads")
                .0
                .tracked,
            "30m"
        );
    }
}
