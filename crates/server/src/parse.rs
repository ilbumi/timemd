//! Turning request strings into domain types.
//!
//! Shared because the same four conversions were being written out per handler,
//! and had already diverged over whether an empty string means "absent".

use serde::{Deserialize, Deserializer};
use timemd_core::{BlockId, Color, Mark, Minutes, ProjectSlug};

use crate::error::{ApiError, ApiResult};

/// Reads a field that may be absent or explicitly `null` into `Option<Option<T>>`.
///
/// Paired with `#[serde(default)]` on a doubly-optional field: an absent key
/// arrives as `None` ("leave it"), an explicit `null` as `Some(None)` ("clear
/// it"). Without it the two collapse, and a request meaning "turn this off"
/// becomes a silent no-op.
pub fn nullable<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// An empty string means "not given" — an HTML `<select>` with no choice made
/// submits `""`, and that is not a validation failure.
pub fn optional_slug(raw: Option<String>) -> ApiResult<Option<ProjectSlug>> {
    present(raw).map(slug).transpose()
}

pub fn slug(raw: String) -> ApiResult<ProjectSlug> {
    ProjectSlug::new(raw).map_err(bad_request)
}

pub fn optional_minutes(raw: Option<String>) -> ApiResult<Option<Minutes>> {
    present(raw)
        .map(|value| value.parse::<Minutes>().map_err(bad_request))
        .transpose()
}

pub fn optional_color(raw: Option<String>) -> ApiResult<Option<Color>> {
    present(raw)
        .map(|value| Color::new(value).map_err(bad_request))
        .transpose()
}

/// Core rejects an unknown mark with `Error::Invalid`, which `ApiError` already
/// maps to a 400 — so the message and the status stay in one place.
pub fn optional_mark(raw: Option<String>) -> ApiResult<Option<Mark>> {
    present(raw)
        .map(|value| value.parse::<Mark>().map_err(ApiError::from))
        .transpose()
}

pub fn block_id(raw: &str) -> ApiResult<BlockId> {
    BlockId::new(raw).map_err(bad_request)
}

fn present(raw: Option<String>) -> Option<String> {
    raw.filter(|value| !value.is_empty())
}

fn bad_request(error: timemd_core::ParseErrorKind) -> ApiError {
    ApiError::bad_request(error.to_string())
}
