//! `/api/reports` — where the time went.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use timemd_core::report::{self, GroupBy, Report};
use timemd_core::{Bucket, DateRange, Minutes};

use crate::error::ApiResult;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/reports", get(read))
}

/// The wire shape. Core's `Report` stays free of HTTP naming conventions, in
/// keeping with every other endpoint here.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportView {
    from: NaiveDate,
    to: NaiveDate,
    group_by: GroupBy,
    total: Minutes,
    /// Core's `Bucket` serialises correctly as-is: its field names are single
    /// words, so there is no camelCase to apply and nothing to restate here.
    buckets: Vec<Bucket>,
}

impl From<Report> for ReportView {
    fn from(report: Report) -> Self {
        Self {
            from: report.from,
            to: report.to,
            group_by: report.group_by,
            total: report.total,
            buckets: report.buckets,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportQuery {
    from: NaiveDate,
    to: NaiveDate,
    #[serde(default = "by_project")]
    group_by: GroupBy,
}

fn by_project() -> GroupBy {
    GroupBy::Project
}

async fn read(
    State(state): State<AppState>,
    Query(query): Query<ReportQuery>,
) -> ApiResult<Json<ReportView>> {
    let range = DateRange::new(query.from, query.to)?;
    let report = report::build(state.store(), range, query.group_by)?;
    Ok(Json(ReportView::from(report)))
}
