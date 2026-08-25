//! `/api/todos` — the todo list.
//!
//! A todo is addressed by its id, so unlike milestones there is no whole-list
//! `PATCH`: the web app can name the one row it changed, which is the shape that
//! does not clobber a concurrent edit to a different row.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use timemd_core::{Stamp, Todo, TodoEdit, TodoId, TodoQuery, Todos};

use crate::error::{ApiError, ApiResult};
use crate::parse::{
    optional_on_completion, optional_priority, optional_slug, optional_stamp, optional_status,
    todo_id, todo_ids,
};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/todos", get(list).post(create))
        .route("/todos/{id}", get(read).patch(update).delete(remove))
}

/// One todo as the wire sees it.
///
/// Every field is a string in the same spelling the file uses, so what the UI
/// shows and what `todos.md` holds cannot drift apart. The core type is not
/// `Serialize` because its constructor is the write-side gate.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoView {
    /// Absent only for a hand-written todo the app has not written yet. Until
    /// it has one, no surface can address it.
    id: Option<String>,
    status: String,
    description: String,
    project: Option<String>,
    priority: String,
    tags: Vec<String>,
    recurrence: Option<String>,
    depends_on: Vec<String>,
    created: Option<String>,
    start: Option<String>,
    scheduled: Option<String>,
    due: Option<String>,
    cancelled: Option<String>,
    done: Option<String>,
    on_completion: Option<String>,
}

impl From<&Todo> for TodoView {
    fn from(todo: &Todo) -> Self {
        let stamp = |value: Option<Stamp>| value.map(|stamp| stamp.to_string());
        Self {
            id: todo.id().map(ToString::to_string),
            status: todo.status.to_string(),
            description: todo.description().to_owned(),
            project: todo.project.as_ref().map(ToString::to_string),
            priority: todo.priority.to_string(),
            tags: todo.tags().map(ToOwned::to_owned).collect(),
            recurrence: todo.recurrence.clone(),
            depends_on: todo.depends_on.iter().map(ToString::to_string).collect(),
            created: stamp(todo.created),
            start: stamp(todo.start),
            scheduled: stamp(todo.scheduled),
            due: stamp(todo.due),
            cancelled: stamp(todo.cancelled),
            done: stamp(todo.done),
            on_completion: todo.on_completion.map(|value| value.to_string()),
        }
    }
}

/// The list, plus whatever the app could not read.
///
/// An object rather than a bare array so the problems have somewhere to go: a
/// broken line has to be visible in the UI rather than silently missing, which
/// is the same reason `ProjectView` carries them.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoListView {
    todos: Vec<TodoView>,
    problems: Vec<String>,
}

/// Narrows the list. Every filter absent means every todo.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoFilter {
    project: Option<String>,
    /// `open`, `done` or `cancelled`. Absent means all three.
    status: Option<String>,
    /// Only todos due on or before this date.
    due_before: Option<String>,
    /// Only todos scheduled for this date.
    scheduled_on: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTodo {
    description: String,
    status: Option<String>,
    project: Option<String>,
    priority: Option<String>,
    recurrence: Option<String>,
    depends_on: Option<Vec<String>>,
    created: Option<String>,
    start: Option<String>,
    scheduled: Option<String>,
    due: Option<String>,
    cancelled: Option<String>,
    done: Option<String>,
    on_completion: Option<String>,
}

/// Every clearable field is doubly optional, so an absent key ("leave it") is
/// distinguishable from an explicit `null` ("clear it").
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoPatch {
    description: Option<String>,
    status: Option<String>,
    priority: Option<String>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    project: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    recurrence: Option<Option<String>>,
    depends_on: Option<Vec<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    created: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    start: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    scheduled: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    due: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    cancelled: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    done: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::parse::nullable")]
    on_completion: Option<Option<String>>,
}

/// The conversions a patch and a create share, done before the store is touched
/// so a rejected value cannot leave the file half-updated.
fn edit_from(patch: TodoPatch) -> ApiResult<TodoEdit> {
    let clearable = |raw: Option<Option<String>>| -> ApiResult<Option<Option<Stamp>>> {
        raw.map(optional_stamp).transpose()
    };

    Ok(TodoEdit {
        description: patch.description,
        status: optional_status(patch.status)?,
        priority: optional_priority(patch.priority)?,
        project: patch.project.map(optional_slug).transpose()?,
        recurrence: patch
            .recurrence
            .map(|raw| raw.filter(|rule| !rule.trim().is_empty())),
        depends_on: patch.depends_on.map(todo_ids).transpose()?,
        created: clearable(patch.created)?,
        start: clearable(patch.start)?,
        scheduled: clearable(patch.scheduled)?,
        due: clearable(patch.due)?,
        cancelled: clearable(patch.cancelled)?,
        done: clearable(patch.done)?,
        on_completion: patch
            .on_completion
            .map(optional_on_completion)
            .transpose()?,
    })
}

async fn list(
    State(state): State<AppState>,
    Query(filter): Query<TodoFilter>,
) -> ApiResult<Json<TodoListView>> {
    let todos = state.store().read_todos()?;

    let query = TodoQuery {
        project: optional_slug(filter.project)?,
        status: optional_status(filter.status)?,
        only_open: false,
        due_before: optional_stamp(filter.due_before)?,
        scheduled_on: optional_stamp(filter.scheduled_on)?,
    };

    Ok(Json(TodoListView {
        todos: todos
            .matching(&query)
            .into_iter()
            .map(TodoView::from)
            .collect(),
        problems: todos.problems().iter().map(ToString::to_string).collect(),
    }))
}

async fn create(
    State(state): State<AppState>,
    Json(request): Json<NewTodo>,
) -> ApiResult<(StatusCode, Json<TodoView>)> {
    let status = optional_status(request.status)?.unwrap_or_default();
    let mut todo = Todo::new(status, &request.description)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;

    // A created date the caller did not ask for, because a todo with no history
    // cannot be sorted by age and every other surface would have to invent one.
    let created = match optional_stamp(request.created)? {
        Some(stamp) => stamp,
        None => Stamp::on(state.today()?),
    };

    let edit = edit_from(TodoPatch {
        description: None,
        status: None,
        priority: request.priority,
        project: request.project.map(Some),
        recurrence: request.recurrence.map(Some),
        depends_on: request.depends_on,
        created: None,
        start: request.start.map(Some),
        scheduled: request.scheduled.map(Some),
        due: request.due.map(Some),
        cancelled: request.cancelled.map(Some),
        done: request.done.map(Some),
        on_completion: request.on_completion.map(Some),
    })?;
    // Built before the store is touched, so the todo that lands is the one the
    // request asked for rather than a bare line edited into shape afterwards.
    todo.apply(edit)
        .map_err(|error| ApiError::bad_request(error.to_string()))?;
    todo.created = Some(created);

    let view = state.store().try_update_todos(|todos| {
        let id = todos.add(todo)?;
        Ok::<_, timemd_core::Error>(TodoView::from(todos.get(&id)?))
    })??;

    Ok((StatusCode::CREATED, Json(view)))
}

async fn read(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<TodoView>> {
    let id = todo_id(&id)?;
    let todos = state.store().read_todos()?;
    Ok(Json(TodoView::from(found(&todos, &id)?)))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<TodoPatch>,
) -> ApiResult<Json<TodoView>> {
    let id = todo_id(&id)?;
    let edit = edit_from(patch)?;

    // The closure answers in `ApiError` rather than core's error so that "no
    // such todo" keeps its 404 instead of collapsing into the 400 every other
    // refused write gets.
    let view = state.store().try_update_todos(|todos| {
        found(todos, &id)?;
        todos.update(&id, edit)?;
        Ok::<_, ApiError>(TodoView::from(found(todos, &id)?))
    })??;

    Ok(Json(view))
}

async fn remove(State(state): State<AppState>, Path(id): Path<String>) -> ApiResult<StatusCode> {
    let id = todo_id(&id)?;
    state.store().try_update_todos(|todos| {
        todos.remove(&id).map_err(|_| missing(&id))?;
        Ok::<_, ApiError>(())
    })??;
    Ok(StatusCode::NO_CONTENT)
}

/// A todo that is not there is a 404, where core reports it as a refused write.
fn found<'a>(todos: &'a Todos, id: &TodoId) -> ApiResult<&'a Todo> {
    todos.get(id).map_err(|_| missing(id))
}

fn missing(id: &TodoId) -> ApiError {
    ApiError::not_found(format!("no todo with id {id:?}"))
}
