//! Fixtures shared by the command modules' test suites.
//!
//! Time is injected rather than read, so every assertion about a day, a total
//! or a remaining duration is the same on any machine on any day.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use timemd_core::Store;

use crate::Command;

pub fn store() -> (tempfile::TempDir, Store) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = Store::new(directory.path());
    (directory, store)
}

/// 2026-08-01 is a Saturday.
pub fn moment(hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 8, 1)
        .expect("valid date")
        .and_hms_opt(hour, minute, 0)
        .expect("valid time")
}

pub fn at(hour: u32, minute: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(hour, minute, 0).expect("valid time")
}

pub fn start(project: Option<&str>, duration: Option<&str>) -> Command {
    Command::Start {
        project: project.map(ToOwned::to_owned),
        note: String::new(),
        duration: duration.map(ToOwned::to_owned),
        todo: None,
    }
}

pub fn log(
    project: Option<&str>,
    from: NaiveTime,
    to: NaiveTime,
    date: Option<NaiveDate>,
) -> Command {
    Command::Log {
        project: project.map(ToOwned::to_owned),
        from,
        to,
        note: String::new(),
        date,
    }
}
