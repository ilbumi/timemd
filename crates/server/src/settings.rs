//! `/api/settings` — the pomodoro lengths and the reminder default.
//!
//! Read and write only the four knobs the settings screen offers. `timezone` is
//! deliberately not among them: it reinterprets every time already stored in the
//! tree, which is not a thing to change by mistyping into a phone. Editing
//! `settings.md` by hand still works, and any key the app does not know is
//! carried through untouched either way.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use timemd_core::SettingsPatch;

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

/// The wire body. Durations arrive as the file's own spelling (`25m`, `1h30m`)
/// and become a [`SettingsPatch`] before the store is touched.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsRequest {
    focus: Option<String>,
    short_break: Option<String>,
    long_break: Option<String>,
    remind_before: Option<String>,
}

async fn read(State(state): State<AppState>) -> ApiResult<Json<SettingsView>> {
    let settings = state.store().read_settings()?;
    Ok(Json(SettingsView::from(&settings)))
}

async fn write(
    State(state): State<AppState>,
    Json(request): Json<SettingsRequest>,
) -> ApiResult<Json<SettingsView>> {
    // Parsed before the store is touched, so a rejected duration cannot leave
    // the file half-updated. `Settings::apply` owns the rest of the rule,
    // including refusing a zero session length.
    let patch = SettingsPatch {
        focus: optional_minutes(request.focus)?,
        short_break: optional_minutes(request.short_break)?,
        long_break: optional_minutes(request.long_break)?,
        remind_before: optional_minutes(request.remind_before)?,
    };

    let view = state.store().update_settings(|settings| {
        settings.apply(patch)?;
        Ok::<_, timemd_core::Error>(SettingsView::from(&*settings))
    })??;

    Ok(Json(view))
}
