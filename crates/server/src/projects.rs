//! `/api/projects` — the CRUD half of the app.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use timemd_core::{Color, Project, ProjectSlug, ProjectStatus};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list).post(create))
        .route("/projects/{slug}", get(read).patch(update).delete(remove))
}

#[derive(Serialize)]
pub struct ProjectView {
    slug: String,
    name: String,
    color: Option<String>,
    status: ProjectStatus,
    created: Option<NaiveDate>,
}

impl From<&Project> for ProjectView {
    fn from(project: &Project) -> Self {
        Self {
            slug: project.slug().to_string(),
            name: project.name.clone(),
            color: project.color.as_ref().map(ToString::to_string),
            status: project.status,
            created: project.created,
        }
    }
}

#[derive(Deserialize)]
pub struct NewProject {
    name: String,
    color: Option<String>,
}

#[derive(Deserialize)]
pub struct ProjectPatch {
    name: Option<String>,
    /// Doubly optional so an absent key ("leave it") is distinguishable from an
    /// explicit `null` ("clear it").
    #[serde(default, deserialize_with = "present")]
    color: Option<Option<String>>,
    status: Option<ProjectStatus>,
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
    let color = parse_color(request.color)?;

    let mut project = Project::new(slug, &request.name, state.today()?);
    project.color = color;
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
    let color = match patch.color {
        Some(raw) => Some(parse_color(raw)?),
        None => None,
    };

    let view = state.store().update_project(&slug, |project| {
        if let Some(name) = patch.name {
            project.name = name;
        }
        if let Some(color) = color {
            project.color = color;
        }
        if let Some(status) = patch.status {
            project.status = status;
        }
        ProjectView::from(&*project)
    })?;

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

fn parse_color(raw: Option<String>) -> ApiResult<Option<Color>> {
    raw.map(|value| Color::new(value).map_err(|error| ApiError::bad_request(error.to_string())))
        .transpose()
}
