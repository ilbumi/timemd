//! `/api/schedule` and `/api/days` — what was planned and what was tracked.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use timemd_core::day::Session;
use timemd_core::schedule::{planned, planned_range};
use timemd_core::{DateRange, DayBlock, Minutes, Occurrence, RecurringBlock};

use crate::error::{ApiError, ApiResult};
use crate::parse::{block_id, optional_minutes, optional_slug};
use crate::state::AppState;
use crate::todos::TodoView;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/schedule", get(range))
        .route(
            "/schedule/recurring",
            get(read_recurring).put(write_recurring),
        )
        .route("/days/{date}", get(read_day))
        .route("/days/{date}/sessions", post(add_session))
        .route(
            "/days/{date}/sessions/{index}",
            axum::routing::patch(replace_session).delete(remove_session),
        )
        .route("/days/{date}/blocks", post(add_block))
        .route(
            "/days/{date}/blocks/{index}",
            axum::routing::patch(replace_block).delete(remove_block),
        )
        .route("/days/{date}/skips", post(add_skip))
        .route(
            "/days/{date}/skips/{id}",
            axum::routing::delete(remove_skip),
        )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OccurrenceView {
    date: NaiveDate,
    start: NaiveTime,
    end: NaiveTime,
    duration: Minutes,
    project: Option<String>,
    title: String,
    remind_before: Option<Minutes>,
    /// The repeating block this came from, or `null` for a one-off.
    block: Option<String>,
    /// Position among that day's one-off blocks, which is what the delete
    /// endpoint addresses. `null` for a repeat.
    ///
    /// Sent because the server owns this index. The client used to recover it by
    /// counting entries in the merged list, which silently depended on how
    /// `planned()` happens to sort.
    one_off_index: Option<usize>,
}

fn views_for(occurrences: Vec<Occurrence>) -> Vec<OccurrenceView> {
    occurrences
        .into_iter()
        .map(|occurrence| OccurrenceView {
            duration: occurrence.duration(),
            date: occurrence.date,
            start: occurrence.start,
            end: occurrence.end,
            project: occurrence.project.map(|slug| slug.to_string()),
            title: occurrence.title,
            remind_before: occurrence.remind_before,
            block: occurrence.block.map(|id| id.to_string()),
            one_off_index: occurrence.one_off_index,
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    index: usize,
    start: NaiveTime,
    end: NaiveTime,
    duration: Minutes,
    project: Option<String>,
    note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayView {
    date: NaiveDate,
    sessions: Vec<SessionView>,
    tracked: Minutes,
    planned: Vec<OccurrenceView>,
    skipped: Vec<String>,
    /// Todos scheduled for this day, untimed ones first. Read-only here: the
    /// day view shows them so the plan and the list are one picture, but a todo
    /// is edited where it lives.
    todos: Vec<TodoView>,
    /// Lines the app could not parse, so a broken file is visible in the UI
    /// rather than silently half-loaded.
    problems: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringView {
    id: String,
    /// The weekdays as canonical names. The stored form has ranges and `daily`
    /// in it; spelling those is core's job, so the wire carries the plain set
    /// and the client edits it without knowing the grammar.
    days: Vec<String>,
    start: NaiveTime,
    end: NaiveTime,
    project: Option<String>,
    title: String,
    remind_before: Option<String>,
}

#[derive(Deserialize)]
pub struct RangeQuery {
    from: NaiveDate,
    to: NaiveDate,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewSession {
    start: NaiveTime,
    end: NaiveTime,
    project: Option<String>,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlock {
    start: NaiveTime,
    end: NaiveTime,
    project: Option<String>,
    #[serde(default)]
    title: String,
    remind_before: Option<String>,
}

#[derive(Deserialize)]
pub struct SkipBody {
    id: String,
}

async fn range(
    State(state): State<AppState>,
    Query(query): Query<RangeQuery>,
) -> ApiResult<Json<Vec<OccurrenceView>>> {
    let range = DateRange::new(query.from, query.to)?;
    Ok(Json(views_for(planned_range(state.store(), range)?)))
}

async fn read_recurring(State(state): State<AppState>) -> ApiResult<Json<Vec<RecurringView>>> {
    let recurring = state.store().read_recurring()?;
    Ok(Json(recurring.blocks().iter().map(view_of).collect()))
}

/// Replaces the whole repeating list.
///
/// Wholesale rather than per-block: the list is short, editing it is rare, and
/// one endpoint beats four. Unparseable lines already in the file are preserved
/// by the store regardless.
async fn write_recurring(
    State(state): State<AppState>,
    Json(blocks): Json<Vec<RecurringView>>,
) -> ApiResult<Json<Vec<RecurringView>>> {
    let parsed: Vec<RecurringBlock> = blocks
        .iter()
        .map(block_from)
        .collect::<ApiResult<Vec<_>>>()?;

    // The closure returns the result, so there is no second read of the file
    // that was just written.
    let stored = state.store().update_recurring(|recurring| {
        recurring.replace_all(parsed);
        recurring.blocks().iter().map(view_of).collect()
    })?;

    Ok(Json(stored))
}

async fn read_day(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<DayView>> {
    let day = state.store().read_day(date)?;
    let recurring = state.store().read_recurring()?;
    let todos = state.store().read_todos()?;

    Ok(Json(DayView {
        date,
        sessions: day
            .sessions()
            .iter()
            .enumerate()
            .map(|(index, session)| SessionView {
                index,
                start: session.start,
                end: session.end,
                duration: session.duration(),
                project: session.project.as_ref().map(ToString::to_string),
                note: session.note.clone(),
            })
            .collect(),
        tracked: day.total(),
        planned: views_for(planned(&day, &recurring)),
        skipped: day.skipped().iter().map(ToString::to_string).collect(),
        todos: todos
            .scheduled_on(date)
            .into_iter()
            .map(TodoView::from)
            .collect(),
        problems: day.problems().iter().map(ToString::to_string).collect(),
    }))
}

async fn add_session(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    Json(body): Json<NewSession>,
) -> ApiResult<StatusCode> {
    let project = optional_slug(body.project)?;
    state.store().update_day(date, |day| {
        day.add_session(Session::new(
            body.start,
            body.end,
            project,
            body.note.trim(),
        ));
    })?;
    Ok(StatusCode::CREATED)
}

async fn replace_session(
    State(state): State<AppState>,
    Path((date, index)): Path<(NaiveDate, usize)>,
    Json(body): Json<NewSession>,
) -> ApiResult<StatusCode> {
    let project = optional_slug(body.project)?;
    let replaced = state.store().update_day(date, |day| {
        day.replace_session(
            index,
            Session::new(body.start, body.end, project, body.note.trim()),
        )
    })?;

    replaced
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| missing_session(date, index))
}

async fn remove_session(
    State(state): State<AppState>,
    Path((date, index)): Path<(NaiveDate, usize)>,
) -> ApiResult<StatusCode> {
    let removed = state
        .store()
        .update_day(date, |day| day.remove_session(index))?;

    removed
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| missing_session(date, index))
}

async fn add_block(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    Json(body): Json<NewBlock>,
) -> ApiResult<StatusCode> {
    let block = DayBlock {
        start: body.start,
        end: body.end,
        project: optional_slug(body.project)?,
        title: body.title.trim().to_owned(),
        remind_before: optional_minutes(body.remind_before)?,
    };
    state.store().update_day(date, |day| day.add_block(block))?;
    Ok(StatusCode::CREATED)
}

/// Replaces a one-off block.
///
/// Like `replace_session`, this re-sorts the day, so the `oneOffIndex` the
/// client used may afterwards name a different block. It answers 204 and the
/// client re-reads — which the day screen's `mutate()` wrapper already does
/// after every mutation.
async fn replace_block(
    State(state): State<AppState>,
    Path((date, index)): Path<(NaiveDate, usize)>,
    Json(body): Json<NewBlock>,
) -> ApiResult<StatusCode> {
    // Every conversion happens before the store is touched, so a rejected value
    // cannot leave the file half-updated.
    let block = DayBlock {
        start: body.start,
        end: body.end,
        project: optional_slug(body.project)?,
        title: body.title.trim().to_owned(),
        remind_before: optional_minutes(body.remind_before)?,
    };
    let replaced = state
        .store()
        .update_day(date, |day| day.replace_block(index, block))?;

    replaced
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| missing_block(date, index))
}

async fn remove_block(
    State(state): State<AppState>,
    Path((date, index)): Path<(NaiveDate, usize)>,
) -> ApiResult<StatusCode> {
    let removed = state
        .store()
        .update_day(date, |day| day.remove_block(index))?;

    removed
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| missing_block(date, index))
}

async fn add_skip(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
    Json(body): Json<SkipBody>,
) -> ApiResult<StatusCode> {
    let id = block_id(&body.id)?;
    state.store().update_day(date, |day| day.skip(id))?;
    Ok(StatusCode::CREATED)
}

async fn remove_skip(
    State(state): State<AppState>,
    Path((date, id)): Path<(NaiveDate, String)>,
) -> ApiResult<StatusCode> {
    let id = block_id(&id)?;
    let restored = state.store().update_day(date, |day| day.unskip(&id))?;

    if restored {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "{id} was not skipped on {date}"
        )))
    }
}

fn view_of(block: &RecurringBlock) -> RecurringView {
    RecurringView {
        id: block.id.to_string(),
        days: block
            .days
            .names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        start: block.start,
        end: block.end,
        project: block.project.as_ref().map(ToString::to_string),
        title: block.title.clone(),
        remind_before: block.remind_before.map(|lead| lead.to_string()),
    }
}

fn block_from(view: &RecurringView) -> ApiResult<RecurringBlock> {
    Ok(RecurringBlock {
        id: block_id(&view.id)?,
        // Joined and handed straight back to core's parser, so there is exactly
        // one implementation of what a day spec means. An empty list is refused
        // there, which is right: a block on no days would never fire.
        days: view
            .days
            .join(",")
            .parse()
            .map_err(|error: timemd_core::ParseErrorKind| {
                ApiError::bad_request(error.to_string())
            })?,
        start: view.start,
        end: view.end,
        project: optional_slug(view.project.clone())?,
        title: view.title.trim().to_owned(),
        remind_before: optional_minutes(view.remind_before.clone())?,
    })
}

fn missing_session(date: NaiveDate, index: usize) -> ApiError {
    ApiError::not_found(format!("no session at index {index} on {date}"))
}

fn missing_block(date: NaiveDate, index: usize) -> ApiError {
    ApiError::not_found(format!("no block at index {index} on {date}"))
}
