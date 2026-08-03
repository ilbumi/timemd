//! Domain types and the markdown-file store that backs timemd.
//!
//! The markdown tree is not a serialisation of the database — it *is* the
//! database, and its grammar is a public interface that agents write against by
//! hand. Two rules follow from that and shape everything here:
//!
//! 1. **Reads are lenient, writes are strict.** A line the app cannot parse is
//!    preserved verbatim and reported, never rejected. One typo in a
//!    hand-edited file must not make the day unreadable.
//! 2. **The app owns only what it understands.** Unknown sections and unknown
//!    frontmatter keys survive a write untouched.

pub mod active;
pub mod day;
pub mod document;
pub mod error;
pub mod grammar;
pub mod ids;
pub mod minutes;
pub mod project;
pub mod push;
pub mod reminders;
pub mod report;
pub mod schedule;
pub mod settings;
pub mod store;
pub mod timer;

pub use active::{ActiveSession, SessionKind};
pub use day::{Day, Session};
pub use document::{Document, Section};
pub use error::{Error, ParseError, ParseErrorKind, Result};
pub use ids::{BlockId, ProjectSlug};
pub use minutes::Minutes;
pub use project::{Color, Mark, Milestone, MilestoneEdit, Project, ProjectStatus};
pub use push::{PushState, Subscription};
pub use reminders::{Reminder, SentLog};
pub use report::{Bucket, DateRange, GroupBy, Report};
pub use schedule::{
    DayBlock, DaySet, Occurrence, Recurring, RecurringBlock, planned, planned_range,
};
pub use settings::{Settings, SettingsPatch};
pub use store::{Store, Tx};
pub use timer::{StartRequest, Stopped, Timer, TimerState};
