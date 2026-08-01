//! Totals over a date range, computed from the day files.
//!
//! No index and no cache: a year is a few hundred small reads, and reading
//! through means a file an agent edited a second ago is already reflected.

use chrono::NaiveDate;
use serde::Serialize;

use crate::day::Session;
use crate::error::Result;
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;
use crate::store::Store;

/// How to bucket the sessions in a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    Project,
    Day,
}

/// One bucket's total.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Bucket {
    /// The project slug or the date, depending on how the report was grouped.
    /// `None` is time tracked against no project.
    pub key: Option<String>,
    pub tracked: Minutes,
    pub sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub group_by: GroupBy,
    pub total: Minutes,
    /// Buckets, largest first when grouped by project and chronological when
    /// grouped by day — each is the order that reads naturally for its grouping.
    pub buckets: Vec<Bucket>,
}

/// Totals every session in `from..=to`.
pub fn build(store: &Store, from: NaiveDate, to: NaiveDate, group_by: GroupBy) -> Result<Report> {
    let mut buckets: Vec<(Option<String>, Minutes, u32)> = Vec::new();
    let mut total = Minutes::default();

    for date in store.recorded_days(from, to) {
        let day = store.read_day(date)?;
        for session in day.sessions() {
            let key = match group_by {
                GroupBy::Project => session.project.as_ref().map(ProjectSlug::to_string),
                GroupBy::Day => Some(date.to_string()),
            };
            accumulate(&mut buckets, key, session);
            total = total + session.duration();
        }
    }

    // Projects sort by size — the question is "where did the time go". Days sort
    // chronologically, because a report out of date order is unreadable.
    match group_by {
        GroupBy::Project => {
            buckets.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)))
        }
        GroupBy::Day => buckets.sort_by(|left, right| left.0.cmp(&right.0)),
    }

    Ok(Report {
        from,
        to,
        group_by,
        total,
        buckets: buckets
            .into_iter()
            .map(|(key, tracked, sessions)| Bucket {
                key,
                tracked,
                sessions,
            })
            .collect(),
    })
}

fn accumulate(
    buckets: &mut Vec<(Option<String>, Minutes, u32)>,
    key: Option<String>,
    session: &Session,
) {
    match buckets.iter_mut().find(|bucket| bucket.0 == key) {
        Some(bucket) => {
            bucket.1 = bucket.1 + session.duration();
            bucket.2 += 1;
        }
        None => buckets.push((key, session.duration(), 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, day).expect("valid date")
    }

    fn slug(text: &str) -> Option<ProjectSlug> {
        Some(ProjectSlug::new(text).expect("valid slug"))
    }

    /// Two days of work: 2h on timemd and 30m on admin, then 1h on timemd.
    fn filled() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());

        store
            .update_day(date(1), |day| {
                day.add_session(Session::new(at(9, 0), at(11, 0), slug("timemd"), "build"));
                day.add_session(Session::new(at(11, 30), at(12, 0), slug("admin"), "email"));
            })
            .expect("writes");
        store
            .update_day(date(3), |day| {
                day.add_session(Session::new(at(9, 0), at(10, 0), slug("timemd"), "more"));
                day.add_session(Session::new(at(14, 0), at(14, 45), None, "untracked"));
            })
            .expect("writes");

        (directory, store)
    }

    #[test]
    fn groups_by_project_largest_first() {
        let (_directory, store) = filled();
        let report = build(&store, date(1), date(31), GroupBy::Project).expect("builds");

        assert_eq!(report.total, Minutes::new(255));
        assert_eq!(
            report
                .buckets
                .iter()
                .map(|bucket| (bucket.key.clone(), bucket.tracked))
                .collect::<Vec<_>>(),
            vec![
                (Some("timemd".to_owned()), Minutes::new(180)),
                (None, Minutes::new(45)),
                (Some("admin".to_owned()), Minutes::new(30)),
            ]
        );
    }

    #[test]
    fn counts_sessions_per_bucket() {
        let (_directory, store) = filled();
        let report = build(&store, date(1), date(31), GroupBy::Project).expect("builds");

        assert_eq!(report.buckets[0].sessions, 2);
        assert_eq!(report.buckets[1].sessions, 1);
    }

    #[test]
    fn groups_by_day_in_date_order() {
        let (_directory, store) = filled();
        let report = build(&store, date(1), date(31), GroupBy::Day).expect("builds");

        assert_eq!(
            report
                .buckets
                .iter()
                .map(|bucket| (bucket.key.clone(), bucket.tracked))
                .collect::<Vec<_>>(),
            vec![
                (Some("2026-08-01".to_owned()), Minutes::new(150)),
                (Some("2026-08-03".to_owned()), Minutes::new(105)),
            ]
        );
    }

    #[test]
    fn the_range_is_inclusive_at_both_ends() {
        let (_directory, store) = filled();

        let just_first = build(&store, date(1), date(1), GroupBy::Day).expect("builds");
        assert_eq!(just_first.total, Minutes::new(150));

        let just_third = build(&store, date(3), date(3), GroupBy::Day).expect("builds");
        assert_eq!(just_third.total, Minutes::new(105));
    }

    #[test]
    fn a_range_with_nothing_in_it_totals_zero() {
        let (_directory, store) = filled();
        let report = build(&store, date(10), date(20), GroupBy::Project).expect("builds");

        assert_eq!(report.total, Minutes::new(0));
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn an_empty_store_reports_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        let report = build(&store, date(1), date(31), GroupBy::Project).expect("builds");

        assert_eq!(report.total, Minutes::new(0));
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn a_session_crossing_midnight_counts_once_on_its_start_day() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        store
            .update_day(date(1), |day| {
                day.add_session(Session::new(at(23, 50), at(0, 20), slug("timemd"), "late"));
            })
            .expect("writes");

        let report = build(&store, date(1), date(2), GroupBy::Day).expect("builds");
        assert_eq!(report.total, Minutes::new(30));
        assert_eq!(report.buckets.len(), 1);
        assert_eq!(report.buckets[0].key, Some("2026-08-01".to_owned()));
    }

    #[test]
    fn hand_written_sessions_are_counted_like_any_other() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        let path = store.day_path(date(1));
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("creates dir");
        std::fs::write(
            &path,
            "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-10:30 (90m) [[timemd]] by hand\n",
        )
        .expect("writes");

        let report = build(&store, date(1), date(1), GroupBy::Project).expect("builds");
        assert_eq!(report.total, Minutes::new(90));
    }
}
