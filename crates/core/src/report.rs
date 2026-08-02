//! Totals over a date range, computed from the day files.
//!
//! No index and no cache: a year is a few hundred small reads, and reading
//! through means a file an agent edited a second ago is already reflected.
//!
//! Each day is read once and counted twice: its sessions are what happened, and
//! `schedule::planned` on that same `Day` is what was meant to. Reaching for
//! `planned_range` instead would open every file in the range a second time.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::Serialize;

use crate::error::{Error, Result};
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;
use crate::schedule;
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
    /// What the schedule set aside for this key over the same range. Carried
    /// beside `tracked` rather than derived from it: the two are independent,
    /// and a bucket that is all plan and no work is the interesting case.
    pub planned: Minutes,
    pub sessions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub group_by: GroupBy,
    pub total: Minutes,
    /// Everything the schedule set aside over the range. It sits beside `total`
    /// rather than renaming it, because the headline figure of a report is still
    /// the time that actually happened; the plan is the context for it.
    pub planned: Minutes,
    /// Buckets, largest first when grouped by project and chronological when
    /// grouped by day — each is the order that reads naturally for its grouping.
    pub buckets: Vec<Bucket>,
}

/// Totals every session in the range, and everything scheduled across it.
pub fn build(store: &Store, range: DateRange, group_by: GroupBy) -> Result<Report> {
    // One read for the whole range: the repeating blocks do not vary by date,
    // only each day's skip list does.
    let recurring = store.read_recurring()?;

    let mut buckets: Vec<Bucket> = Vec::new();
    let mut total = Minutes::default();
    let mut planned = Minutes::default();

    for date in range.dates() {
        let day = store.read_day(date)?;
        // Sessions and blocks name their project the same way, so they share one
        // key space — including the `None` bucket for time against no project.
        let key_for = |project: Option<&ProjectSlug>| match group_by {
            GroupBy::Project => project.map(ProjectSlug::to_string),
            GroupBy::Day => Some(date.to_string()),
        };

        for session in day.sessions() {
            let bucket = bucket_for(&mut buckets, key_for(session.project.as_ref()));
            bucket.tracked = bucket.tracked + session.duration();
            bucket.sessions += 1;
            total = total + session.duration();
        }

        // The same `Day`, already in hand — see the module doc.
        for occurrence in schedule::planned(&day, &recurring) {
            let bucket = bucket_for(&mut buckets, key_for(occurrence.project.as_ref()));
            bucket.planned = bucket.planned + occurrence.duration();
            planned = planned + occurrence.duration();
        }
    }

    // Projects sort by size — the question is "where did the time go". A bucket
    // that was only ever planned has no tracked time to sort by, so it falls to
    // the plan and lands below everything that was actually worked on. Days sort
    // chronologically, because a report out of date order is unreadable.
    match group_by {
        GroupBy::Project => buckets.sort_by(|left, right| {
            right
                .tracked
                .cmp(&left.tracked)
                .then_with(|| right.planned.cmp(&left.planned))
                .then_with(|| left.key.cmp(&right.key))
        }),
        GroupBy::Day => buckets.sort_by(|left, right| left.key.cmp(&right.key)),
    }

    Ok(Report {
        from: range.from(),
        to: range.to(),
        group_by,
        total,
        planned,
        buckets,
    })
}

/// The bucket for `key`, created empty if this is the first thing to land in it.
fn bucket_for(buckets: &mut Vec<Bucket>, key: Option<String>) -> &mut Bucket {
    // Grouping by day appends in date order, so the live bucket is almost always
    // the last one; checking it first keeps a year-long report linear. The search
    // below is what makes it correct — this only makes it fast.
    let found = if buckets.last().is_some_and(|bucket| bucket.key == key) {
        Some(buckets.len() - 1)
    } else {
        buckets.iter().position(|bucket| bucket.key == key)
    };

    let index = found.unwrap_or_else(|| {
        buckets.push(Bucket {
            key,
            tracked: Minutes::default(),
            planned: Minutes::default(),
            sessions: 0,
        });
        buckets.len() - 1
    });

    &mut buckets[index]
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveTime;

    use crate::day::Session;
    use crate::ids::BlockId;
    use crate::schedule::{DayBlock, DaySet, RecurringBlock};

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

    /// Writes a one-off block into a day's `## Schedule`, spelled as the grammar.
    fn plan(store: &Store, day: u32, line: &str) {
        let block = DayBlock::parse(line).expect("parses");
        store
            .update_day(date(day), |file| file.add_block(block))
            .expect("writes");
    }

    /// Writes a block that repeats every day, so no test needs weekday arithmetic.
    fn repeats(store: &Store, id: &str, line: &str) {
        let block = DayBlock::parse(line).expect("parses");
        store
            .update_recurring(|recurring| {
                recurring.upsert(RecurringBlock {
                    id: BlockId::new(id).expect("valid id"),
                    days: DaySet::ALL,
                    start: block.start,
                    end: block.end,
                    project: block.project,
                    title: block.title,
                    remind_before: block.remind_before,
                });
            })
            .expect("writes");
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
        assert_eq!(report.planned, Minutes::new(0));
        assert!(report.buckets.is_empty());
    }

    #[test]
    fn an_empty_store_reports_nothing() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

        // A missing `recurring.md` is an empty schedule, not an error.
        assert_eq!(report.total, Minutes::new(0));
        assert_eq!(report.planned, Minutes::new(0));
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
    fn planned_time_lands_in_the_same_bucket_as_the_work_done_on_it() {
        let (_directory, store) = filled();
        plan(&store, 1, "09:00-11:00 [[timemd]] Block");

        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

        assert_eq!(report.total, Minutes::new(255));
        assert_eq!(report.planned, Minutes::new(120));
        assert_eq!(report.buckets[0].key, Some("timemd".to_owned()));
        assert_eq!(report.buckets[0].tracked, Minutes::new(180));
        assert_eq!(report.buckets[0].planned, Minutes::new(120));
    }

    #[test]
    fn a_project_that_was_only_planned_gets_a_bucket_and_sorts_last() {
        let (_directory, store) = filled();
        plan(&store, 2, "09:00-10:00 [[russian]] Block");

        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

        // The three worked-on buckets keep exactly the order they had before.
        assert_eq!(
            report
                .buckets
                .iter()
                .map(|bucket| bucket.key.clone())
                .collect::<Vec<_>>(),
            vec![
                Some("timemd".to_owned()),
                None,
                Some("admin".to_owned()),
                Some("russian".to_owned()),
            ]
        );

        let only_planned = report.buckets.last().expect("has a bucket");
        assert_eq!(only_planned.tracked, Minutes::new(0));
        assert_eq!(only_planned.planned, Minutes::new(60));
        assert_eq!(only_planned.sessions, 0);
    }

    #[test]
    fn buckets_with_nothing_tracked_order_by_how_much_was_planned() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        plan(&store, 1, "09:00-10:00 [[russian]] Block");
        plan(&store, 1, "14:00-16:00 [[piano]] Block");

        let report = build(&store, span(1, 31), GroupBy::Project).expect("builds");

        assert_eq!(
            report
                .buckets
                .iter()
                .map(|bucket| (bucket.key.clone(), bucket.planned))
                .collect::<Vec<_>>(),
            vec![
                (Some("piano".to_owned()), Minutes::new(120)),
                (Some("russian".to_owned()), Minutes::new(60)),
            ]
        );
    }

    /// Both arms of `schedule::planned` reach the report, on every date in range.
    #[test]
    fn a_repeat_counts_on_every_day_it_falls_on_and_one_offs_join_it() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        repeats(&store, "deep-work", "09:00-11:00 [[timemd]] Block");
        plan(&store, 1, "14:00-15:00 [[timemd]] Block");

        let report = build(&store, span(1, 3), GroupBy::Project).expect("builds");
        assert_eq!(report.planned, Minutes::new(420));
        assert_eq!(report.buckets.len(), 1);
    }

    /// Guards the tempting wrong shortcut: reading `recurring.blocks()` straight
    /// rather than going through `schedule::planned`, which is what applies skips.
    #[test]
    fn a_skipped_repeat_is_not_planned_time() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        repeats(&store, "deep-work", "09:00-11:00 [[timemd]] Block");
        store
            .update_day(date(2), |day| {
                day.skip(BlockId::new("deep-work").expect("valid id"));
            })
            .expect("writes");

        let report = build(&store, span(1, 3), GroupBy::Project).expect("builds");
        assert_eq!(report.planned, Minutes::new(240));
    }

    #[test]
    fn a_day_that_was_planned_and_never_worked_still_lands_in_date_order() {
        let (_directory, store) = filled();
        plan(&store, 2, "09:00-10:00 [[timemd]] Block");

        let report = build(&store, span(1, 31), GroupBy::Day).expect("builds");

        assert_eq!(
            report
                .buckets
                .iter()
                .map(|bucket| (bucket.key.clone(), bucket.tracked, bucket.planned))
                .collect::<Vec<_>>(),
            vec![
                (
                    Some("2026-08-01".to_owned()),
                    Minutes::new(150),
                    Minutes::new(0)
                ),
                (
                    Some("2026-08-02".to_owned()),
                    Minutes::new(0),
                    Minutes::new(60)
                ),
                (
                    Some("2026-08-03".to_owned()),
                    Minutes::new(105),
                    Minutes::new(0)
                ),
            ]
        );
    }

    #[test]
    fn a_block_crossing_midnight_counts_once_on_its_start_day() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        plan(&store, 1, "23:50-00:20 [[timemd]] Block");

        let report = build(&store, span(1, 2), GroupBy::Day).expect("builds");
        assert_eq!(report.planned, Minutes::new(30));
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
