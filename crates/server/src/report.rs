//! `/api/reports` — where the time went.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use timemd_core::Minutes;
use timemd_core::report::{self, GroupBy, Report};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Matches the schedule's bound: a report is a scan over day files, so the same
/// "do not walk a decade" rule applies.
const MAX_RANGE_DAYS: i64 = 366;

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
    buckets: Vec<BucketView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketView {
    /// The project slug or the date, depending on the grouping. `null` is time
    /// tracked against no project.
    key: Option<String>,
    tracked: Minutes,
    sessions: u32,
}

impl From<Report> for ReportView {
    fn from(report: Report) -> Self {
        Self {
            from: report.from,
            to: report.to,
            group_by: report.group_by,
            total: report.total,
            buckets: report
                .buckets
                .into_iter()
                .map(|bucket| BucketView {
                    key: bucket.key,
                    tracked: bucket.tracked,
                    sessions: bucket.sessions,
                })
                .collect(),
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
    if query.to < query.from {
        return Err(ApiError::bad_request("`to` is before `from`"));
    }
    if (query.to - query.from).num_days() > MAX_RANGE_DAYS {
        return Err(ApiError::bad_request(format!(
            "range longer than {MAX_RANGE_DAYS} days"
        )));
    }

    let report = report::build(state.store(), query.from, query.to, query.group_by)?;
    Ok(Json(ReportView::from(report)))
}
