//! `/api/projects` — the CRUD half of the app.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use timemd_core::{Mark, Milestone, Project, ProjectSlug, ProjectStatus};

use crate::parse::{optional_color, optional_mark, optional_minutes};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(create))
        .route("/projects/{slug}", get(read).patch(update).delete(remove))
}

/// One milestone as the wire sees it. The core type is not `Serialize` because
/// its constructor is the write-side gate, and a deserialised struct would walk
/// straight past it.
#[derive(Serialize, Deserialize)]
pub struct MilestoneView {
    done: bool,
    title: String,
}

impl From<&Milestone> for MilestoneView {
    fn from(milestone: &Milestone) -> Self {
        Self {
            done: milestone.done,
            title: milestone.title().to_owned(),
        }
    }
}

#[derive(Serialize)]
pub struct ProjectView {
    slug: String,
    name: String,
    color: Option<String>,
    mark: Mark,
    target: Option<String>,
    status: ProjectStatus,
    created: Option<NaiveDate>,
    milestones: Vec<MilestoneView>,
    /// Milestone lines the app could not parse, so a broken file is visible in
    /// the UI rather than silently half-loaded.
    problems: Vec<String>,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            slug: project.slug().to_string(),
            name: project.name.clone(),
            color: project.color.as_ref().map(ToString::to_string),
            mark: project.mark,
            target: project.target.map(|target| target.to_string()),
            status: project.status,
            created: project.created,
            milestones: project.milestones.iter().map(MilestoneView::from).collect(),
            problems: project.problems().iter().map(ToString::to_string).collect(),
        }
    }
}

#[derive(Deserialize)]
pub struct NewProject {
    name: String,
    color: Option<String>,
    mark: Option<String>,
    target: Option<String>,
    /// Accepted here so creating a project with a milestone list is one atomic
    /// write: a rejected title must not leave a half-made project behind.
    milestones: Option<Vec<MilestoneView>>,
}

#[derive(Deserialize)]
pub struct ProjectPatch {
    name: Option<String>,
    /// Doubly optional so an absent key ("leave it") is distinguishable from an
    /// explicit `null` ("clear it").
    #[serde(default, deserialize_with = "present")]
    color: Option<Option<String>>,
    mark: Option<String>,
    #[serde(default, deserialize_with = "present")]
    target: Option<Option<String>>,
    status: Option<ProjectStatus>,
    /// Replaces the whole list, like the recurring schedule does: it is short,
    /// editing it is rare, and one field beats three endpoints.
    milestones: Option<Vec<MilestoneView>>,
}

fn milestones_from(views: Vec<MilestoneView>) -> ApiResult<Vec<Milestone>> {
    views
        .into_iter()
        .map(|view| {
            Milestone::new(view.done, view.title)
                .map_err(|error| ApiError::bad_request(error.to_string()))
        })
        .collect()
}

fn present<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<ProjectView>>> {
    let projects = state.store().list_projects()?;
    Ok(Json(projects.iter().map(ProjectView::from).collect()))
}

async fn create(
    State(state): State<AppState>,
    Json(request): Json<NewProject>,
) -> ApiResult<(StatusCode, Json<ProjectView>)> {
    let slug = ProjectSlug::from_name(&request.name).ok_or_else(|| {
        ApiError::bad_request(format!(
            "{:?} contains nothing usable as a project name",
            request.name
        ))
    })?;
    let color = optional_color(request.color)?;
    let mark = optional_mark(request.mark)?;
    let target = optional_minutes(request.target)?;
    let milestones = request.milestones.map(milestones_from).transpose()?;

    let mut project = Project::new(slug, &request.name, state.today()?);
    project.color = color;
    project.mark = mark.unwrap_or_default();
    project.target = target;
    project.set_milestones(milestones.unwrap_or_default())?;
    state.store().create_project(&project)?;

    Ok((StatusCode::CREATED, Json(ProjectView::from(&project))))
}

async fn read(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Json<ProjectView>> {
    let slug = slug_from_path(&slug)?;
    let project = state
        .store()
        .read_project(&slug)?
        .ok_or_else(|| ApiError::not_found(format!("no project named {slug:?}")))?;
    Ok(Json(ProjectView::from(&project)))
}

async fn update(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(patch): Json<ProjectPatch>,
) -> ApiResult<Json<ProjectView>> {
    let slug = slug_from_path(&slug)?;

    // Every conversion happens before the store is touched, so a rejected value
    // cannot leave the file half-updated.
    let color = match patch.color {
        Some(raw) => Some(optional_color(raw)?),
        None => None,
    };
    let mark = optional_mark(patch.mark)?;
    let target = match patch.target {
        Some(raw) => Some(optional_minutes(raw)?),
        None => None,
    };
    let milestones = patch.milestones.map(milestones_from).transpose()?;

    let view = state.store().try_update_project(&slug, |project| {
        if let Some(name) = patch.name {
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
        if let Some(status) = patch.status {
            project.status = status;
        }
        if let Some(milestones) = milestones {
            project.set_milestones(milestones)?;
        }
        Ok::<_, timemd_core::Error>(ProjectView::from(&*project))
    })??;

    Ok(Json(view))
}

async fn remove(State(state): State<AppState>, Path(slug): Path<String>) -> ApiResult<StatusCode> {
    let slug = slug_from_path(&slug)?;
    if state.store().delete_project(&slug)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!("no project named {slug:?}")))
    }
}

/// A path segment that is not a valid slug names no project, so it is a 404
/// rather than a 400 — there is no resource there either way.
fn slug_from_path(raw: &str) -> ApiResult<ProjectSlug> {
    ProjectSlug::new(raw).map_err(|_| ApiError::not_found(format!("no project named {raw:?}")))
}
