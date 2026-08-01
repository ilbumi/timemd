//! Totals over a date range, computed from the day files.
//!
//! No index and no cache: a year is a few hundred small reads, and reading
//! through means a file an agent edited a second ago is already reflected.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::Serialize;

use crate::day::Session;
use crate::error::{Error, Result};
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;
use crate::store::Store;

/// The longest span any range request may cover.
///
/// A range is a scan over day files, which is a fact about the store rather than
/// about HTTP — so the bound lives here and every front door inherits it.
pub const MAX_RANGE_DAYS: i64 = 366;

/// A validated, inclusive span of dates.
///
/// Constructing one is the only way to ask for a range, so the ordering rule and
/// the length bound cannot be forgotten by a new caller — which is exactly what
/// happened when they lived in the handlers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
    from: NaiveDate,
    to: NaiveDate,
}

impl DateRange {
    pub fn new(from: NaiveDate, to: NaiveDate) -> Result<Self> {
        if to < from {
            return Err(Error::Invalid("`to` is before `from`".to_owned()));
        }
        if (to - from).num_days() > MAX_RANGE_DAYS {
            return Err(Error::Invalid(format!(
                "range longer than {MAX_RANGE_DAYS} days"
            )));
        }
        Ok(Self { from, to })
    }

    pub fn from(self) -> NaiveDate {
        self.from
    }

    pub fn to(self) -> NaiveDate {
        self.to
    }

    /// Every date in the span, oldest first.
    pub fn dates(self) -> impl Iterator<Item = NaiveDate> {
        self.from
            .iter_days()
            .take_while(move |date| *date <= self.to)
    }
}

/// How to bucket the sessions in a range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupBy {
    Project,
    Day,
}

impl FromStr for GroupBy {
    type Err = Error;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw {
            "project" => Ok(Self::Project),
            "day" => Ok(Self::Day),
            other => Err(Error::Invalid(format!(
                "unknown grouping {other:?}; expected `project` or `day`"
            ))),
        }
    }
}

impl fmt::Display for GroupBy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Project => "project",
            Self::Day => "day",
        })
    }
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

/// Totals every session in the range.
pub fn build(store: &Store, range: DateRange, group_by: GroupBy) -> Result<Report> {
    let mut buckets: Vec<Bucket> = Vec::new();
    let mut total = Minutes::default();

    for date in range.dates() {
        let day = store.read_day(date)?;
        // Hoisted: the key is per day, not per session.
        let day_key = matches!(group_by, GroupBy::Day).then(|| date.to_string());

        for session in day.sessions() {
            let key = match group_by {
                GroupBy::Project => session.project.as_ref().map(ProjectSlug::to_string),
                GroupBy::Day => day_key.clone(),
            };
            accumulate(&mut buckets, key, session);
            total = total + session.duration();
        }
    }

    // Projects sort by size — the question is "where did the time go". Days sort
    // chronologically, because a report out of date order is unreadable.
    match group_by {
        GroupBy::Project => buckets.sort_by(|left, right| {
            right
                .tracked
                .cmp(&left.tracked)
                .then_with(|| left.key.cmp(&right.key))
        }),
        GroupBy::Day => buckets.sort_by(|left, right| left.key.cmp(&right.key)),
    }

    Ok(Report {
        from: range.from(),
        to: range.to(),
        group_by,
        total,
        buckets,
    })
}

fn accumulate(buckets: &mut Vec<Bucket>, key: Option<String>, session: &Session) {
    // Grouping by day appends in date order, so the live bucket is almost always
    // the last one; checking it first keeps a year-long report linear.
    if let Some(bucket) = buckets.last_mut().filter(|bucket| bucket.key == key) {
        bucket.tracked = bucket.tracked + session.duration();
        bucket.sessions += 1;
        return;
    }

    match buckets.iter_mut().find(|bucket| bucket.key == key) {
        Some(bucket) => {
            bucket.tracked = bucket.tracked + session.duration();
            bucket.sessions += 1;
        }
        None => buckets.push(Bucket {
            key,
            tracked: session.duration(),
            sessions: 1,
        }),
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

    fn span(from: u32, to: u32) -> DateRange {
        DateRange::new(date(from), date(to)).expect("valid range")
    }

    #[test]
    fn a_range_enforces_its_own_ordering_and_bound() {
        assert!(DateRange::new(date(31), date(1)).is_err());
        assert!(
            DateRange::new(
                NaiveDate::from_ymd_opt(2020, 1, 1).expect("valid date"),
                date(1)
            )
            .is_err()
        );

        let range = span(1, 3);
        assert_eq!(range.dates().count(), 3);
        assert_eq!(range.from(), date(1));
        assert_eq!(range.to(), date(3));
    }

    #[test]
    fn groupings_round_trip_through_text() {
        assert_eq!("project".parse::<GroupBy>().ok(), Some(GroupBy::Project));
        assert_eq!("day".parse::<GroupBy>().ok(), Some(GroupBy::Day));
        assert_eq!(GroupBy::Project.to_string(), "project");
        assert_eq!(GroupBy::Day.to_string(), "day");
        assert!("colour".parse::<GroupBy>().is_err());
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
        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

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
        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

        assert_eq!(report.buckets[0].sessions, 2);
        assert_eq!(report.buckets[1].sessions, 1);
    }

    #[test]
    fn groups_by_day_in_date_order() {
        let (_directory, store) = filled();
        let report = build(&store, span(1, 31), GroupBy::Day).expect("builds");

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

        let just_first = build(&store, span(1, 1), GroupBy::Day).expect("builds");
        assert_eq!(just_first.total, Minutes::new(150));

        let just_third = build(&store, span(3, 3), GroupBy::Day).expect("builds");
        assert_eq!(just_third.total, Minutes::new(105));
    }

    #[test]
    fn a_range_with_nothing_in_it_totals_zero() {
        let (_directory, store) = filled();
        let report = build(&store, span(10, 20), GroupBy::Project).expect("builds");

        assert_eq!(report.total, Minutes::new(0));
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn an_empty_store_reports_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

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

        let report = build(&store, span(1, 2), GroupBy::Day).expect("builds");
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

        let report = build(&store, span(1, 1), GroupBy::Project).expect("builds");
        assert_eq!(report.total, Minutes::new(90));
    }
}
