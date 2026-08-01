//! `/api/schedule` and `/api/days` — what was planned and what was tracked.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use timemd_core::day::Session;
use timemd_core::schedule::planned;
use timemd_core::{BlockId, DayBlock, Minutes, Occurrence, ProjectSlug, RecurringBlock};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Bounds a range request so a stray `to=9999-12-31` cannot walk a decade of
/// dates on a small server.
const MAX_RANGE_DAYS: i64 = 366;

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
            axum::routing::delete(remove_block),
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
}

impl From<&Occurrence> for OccurrenceView {
    fn from(occurrence: &Occurrence) -> Self {
        Self {
            date: occurrence.date,
            start: occurrence.start,
            end: occurrence.end,
            duration: occurrence.duration(),
            project: occurrence.project.as_ref().map(ToString::to_string),
            title: occurrence.title.clone(),
            remind_before: occurrence.remind_before,
            block: occurrence.block.as_ref().map(ToString::to_string),
        }
    }
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
    /// Lines the app could not parse, so a broken file is visible in the UI
    /// rather than silently half-loaded.
    problems: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringView {
    id: String,
    days: String,
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
    if query.to < query.from {
        return Err(ApiError::bad_request("`to` is before `from`"));
    }
    if (query.to - query.from).num_days() > MAX_RANGE_DAYS {
        return Err(ApiError::bad_request(format!(
            "range longer than {MAX_RANGE_DAYS} days"
        )));
    }

    let recurring = state.store().read_recurring()?;
    let mut occurrences = Vec::new();
    for date in query.from.iter_days().take_while(|date| *date <= query.to) {
        let day = state.store().read_day(date)?;
        occurrences.extend(planned(&day, &recurring).iter().map(OccurrenceView::from));
    }

    Ok(Json(occurrences))
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

    state.store().update_recurring(|recurring| {
        for existing in recurring
            .blocks()
            .iter()
            .map(|block| block.id.clone())
            .collect::<Vec<_>>()
        {
            recurring.remove(&existing);
        }
        for block in parsed {
            recurring.upsert(block);
        }
    })?;

    let recurring = state.store().read_recurring()?;
    Ok(Json(recurring.blocks().iter().map(view_of).collect()))
}

async fn read_day(
    State(state): State<AppState>,
    Path(date): Path<NaiveDate>,
) -> ApiResult<Json<DayView>> {
    let day = state.store().read_day(date)?;
    let recurring = state.store().read_recurring()?;

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
        planned: planned(&day, &recurring)
            .iter()
            .map(OccurrenceView::from)
            .collect(),
        skipped: day.skipped().iter().map(ToString::to_string).collect(),
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

async fn remove_block(
    State(state): State<AppState>,
    Path((date, index)): Path<(NaiveDate, usize)>,
) -> ApiResult<StatusCode> {
    let removed = state
        .store()
        .update_day(date, |day| day.remove_block(index))?;

    removed
        .map(|_| StatusCode::NO_CONTENT)
        .ok_or_else(|| ApiError::not_found(format!("no block at index {index} on {date}")))
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
        days: block.days.to_string(),
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
        days: view
            .days
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

fn block_id(raw: &str) -> ApiResult<BlockId> {
    BlockId::new(raw).map_err(|error| ApiError::bad_request(error.to_string()))
}

fn optional_slug(raw: Option<String>) -> ApiResult<Option<ProjectSlug>> {
    raw.filter(|slug| !slug.is_empty())
        .map(|slug| {
            ProjectSlug::new(slug).map_err(|error| ApiError::bad_request(error.to_string()))
        })
        .transpose()
}

fn optional_minutes(raw: Option<String>) -> ApiResult<Option<Minutes>> {
    raw.filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<Minutes>()
                .map_err(|error| ApiError::bad_request(error.to_string()))
        })
        .transpose()
}

fn missing_session(date: NaiveDate, index: usize) -> ApiError {
    ApiError::not_found(format!("no session at index {index} on {date}"))
}
