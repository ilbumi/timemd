//! `/api/settings` — the pomodoro lengths and the reminder default.
//!
//! Read and write only the four knobs the settings screen offers. `timezone` is
//! deliberately not among them: it reinterprets every time already stored in the
//! tree, which is not a thing to change by mistyping into a phone. Editing
//! `settings.md` by hand still works, and any key the app does not know is
//! carried through untouched either way.

use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use timemd_core::Minutes;

use crate::error::ApiResult;
use crate::parse::optional_minutes;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/settings", get(read).put(write))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    timezone: String,
    focus: String,
    short_break: String,
    long_break: String,
    long_break_every: u32,
    remind_before: String,
}

impl From<&timemd_core::Settings> for SettingsView {
    fn from(settings: &timemd_core::Settings) -> Self {
        Self {
            timezone: settings.timezone.name().to_owned(),
            focus: settings.focus.to_string(),
            short_break: settings.short_break.to_string(),
            long_break: settings.long_break.to_string(),
            long_break_every: settings.long_break_every,
            remind_before: settings.remind_before.to_string(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    focus: Option<String>,
    short_break: Option<String>,
    long_break: Option<String>,
    remind_before: Option<String>,
}

async fn read(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> ApiResult<Json<SettingsView>> {
    let settings = state.store().read_settings()?;
    Ok(Json(SettingsView::from(&settings)))
}

async fn write(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(patch): Json<SettingsPatch>,
) -> ApiResult<Json<SettingsView>> {
    // Parsed before the store is touched, so a rejected duration cannot leave
    // the file half-updated.
    let focus = duration(patch.focus)?;
    let short_break = duration(patch.short_break)?;
    let long_break = duration(patch.long_break)?;
    let remind_before = optional_minutes(patch.remind_before)?;

    let view = state.store().update_settings(|settings| {
        if let Some(focus) = focus {
            settings.focus = focus;
        }
        if let Some(short_break) = short_break {
            settings.short_break = short_break;
        }
        if let Some(long_break) = long_break {
            settings.long_break = long_break;
        }
        if let Some(remind_before) = remind_before {
            settings.remind_before = remind_before;
        }
        SettingsView::from(&*settings)
    })?;

    Ok(Json(view))
}

/// A session length has to be a real length: a zero-minute pomodoro would start
/// and immediately retire itself.
fn duration(raw: Option<String>) -> ApiResult<Option<Minutes>> {
    let parsed = optional_minutes(raw)?;
    match parsed {
        Some(value) if value.is_zero() => Err(crate::error::ApiError::bad_request(
            "a session length must be more than zero minutes",
        )),
        other => Ok(other),
    }
}
