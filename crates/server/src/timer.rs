//! `/api/timer` — starting, stopping and reading the pomodoro.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use timemd_core::active::SessionKind;
use timemd_core::{Minutes, StartRequest, Timer, TimerState};

use crate::parse::{optional_minutes, optional_slug};

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/timer", get(read))
        .route("/timer/start", post(start))
        .route("/timer/stop", post(stop))
        .route("/timer/cancel", post(cancel))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningView {
    kind: SessionKind,
    project: Option<String>,
    note: String,
    started_at: NaiveDateTime,
    ends_at: NaiveDateTime,
    duration: Minutes,
    /// The block's full length in seconds.
    ///
    /// Sent as a number because the client does arithmetic with it (the dial's
    /// sweep); `duration` is the same value as a label. Without this the browser
    /// had to re-implement `Minutes`'s parser to get back to a quantity.
    duration_seconds: i64,
    /// Seconds left, at `server_now`.
    ///
    /// Sent alongside the server's clock so the client can run a smooth
    /// countdown from its own monotonic time instead of trusting the device
    /// clock to agree with the server's.
    remaining_seconds: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerView {
    active: Option<RunningView>,
    completed_today: u32,
    tracked_today: Minutes,
    next_break: Minutes,
    next_break_kind: SessionKind,
    server_now: NaiveDateTime,
}

impl TimerView {
    fn build(state: TimerState, now: NaiveDateTime) -> Self {
        Self {
            active: state.active.map(|active| RunningView {
                kind: active.kind,
                project: active.project.as_ref().map(ToString::to_string),
                note: active.note.clone(),
                started_at: active.started,
                ends_at: active.ends_at(),
                duration: active.duration,
                duration_seconds: i64::from(active.duration.get()) * 60,
                remaining_seconds: (active.ends_at() - now).num_seconds().max(0),
            }),
            completed_today: state.completed_today,
            tracked_today: state.tracked_today,
            next_break: state.next_break,
            next_break_kind: state.next_break_kind,
            server_now: now,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartBody {
    #[serde(default = "focus")]
    kind: SessionKind,
    project: Option<String>,
    #[serde(default)]
    note: String,
    duration: Option<String>,
}

fn focus() -> SessionKind {
    SessionKind::Focus
}

async fn read(State(state): State<AppState>) -> ApiResult<Json<TimerView>> {
    let now = state.local_now()?;
    let current = Timer::new(state.store()).state(now)?;
    Ok(Json(TimerView::build(current, now)))
}

async fn start(
    State(state): State<AppState>,
    Json(body): Json<StartBody>,
) -> ApiResult<Json<TimerView>> {
    let project = optional_slug(body.project)?;
    let duration = optional_minutes(body.duration)?;

    let now = state.local_now()?;
    let timer = Timer::new(state.store());
    timer.start(
        now,
        StartRequest {
            kind: body.kind,
            duration,
            project,
            note: body.note.trim().to_owned(),
        },
    )?;

    Ok(Json(TimerView::build(timer.state(now)?, now)))
}

async fn stop(State(state): State<AppState>) -> ApiResult<Json<TimerView>> {
    let now = state.local_now()?;
    let timer = Timer::new(state.store());
    timer.stop(now)?;
    Ok(Json(TimerView::build(timer.state(now)?, now)))
}

async fn cancel(State(state): State<AppState>) -> ApiResult<Json<TimerView>> {
    let now = state.local_now()?;
    let timer = Timer::new(state.store());
    timer.cancel()?;
    Ok(Json(TimerView::build(timer.state(now)?, now)))
}
