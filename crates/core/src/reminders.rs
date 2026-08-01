//! Which schedule blocks are due a reminder, and which have already had one.
//!
//! The sent log is what makes reminders restart-safe. Without it, a server that
//! restarts inside a block's lead window would notify again on the next tick,
//! and the phone would buzz every thirty seconds until the block began.

use chrono::{NaiveDateTime, TimeDelta};

use crate::day::Day;
use crate::document::Document;
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;
use crate::schedule::{Recurring, planned};
use crate::settings::Settings;

const SECTION_SENT: &str = "Sent";
/// Entries older than this are pruned on write. Long enough that a clock skewed
/// backwards by an hour cannot resurrect a reminder, short enough that the file
/// stays small forever.
const KEEP_DAYS: i64 = 3;

/// A notification that should go out now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reminder {
    /// Stable identity, used to avoid sending twice.
    pub key: String,
    pub title: String,
    pub project: Option<ProjectSlug>,
    pub starts_at: NaiveDateTime,
    pub lead: Minutes,
}

impl Reminder {
    /// What the notification says.
    pub fn body(&self) -> String {
        let when = self.starts_at.format("%H:%M");
        match &self.project {
            Some(project) => format!("{when} · {project}"),
            None => format!("starts at {when}"),
        }
    }
}

/// `data/state/reminders.md` — the keys already notified.
#[derive(Debug, Clone, Default)]
pub struct SentLog {
    keys: Vec<String>,
    document: Option<Document>,
}

impl SentLog {
    pub fn parse(text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let keys = document
            .section(SECTION_SENT)
            .map(|section| {
                section
                    .content()
                    .filter_map(|(_, line)| line.trim().strip_prefix("- ").map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            keys,
            document: Some(document),
        })
    }

    /// Drops entries whose date is well in the past, so the file cannot grow
    /// without bound on a server that runs for months.
    pub fn render(&self, now: NaiveDateTime) -> String {
        let cutoff = (now - TimeDelta::days(KEEP_DAYS)).date().to_string();
        let mut document = self.document.clone().unwrap_or_else(|| {
            let mut fresh = Document::new();
            fresh.set_preamble(vec![
                String::new(),
                "# Reminders already sent".to_owned(),
                String::new(),
            ]);
            fresh
        });

        let lines: Vec<String> = self
            .keys
            .iter()
            .filter(|key| key.as_str() >= cutoff.as_str())
            .map(|key| format!("- {key}"))
            .collect();

        document.upsert_section(SECTION_SENT, lines, &[]);
        document.render()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.iter().any(|sent| sent == key)
    }

    pub fn record(&mut self, key: impl Into<String>) {
        let key = key.into();
        if !self.contains(&key) {
            self.keys.push(key);
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Blocks whose lead window contains `now` and that have not been notified.
///
/// A block whose start has already passed is *not* due: a reminder that arrives
/// after the thing began is noise, so a server that was down through the whole
/// window stays quiet rather than buzzing late.
pub fn due(
    day: &Day,
    recurring: &Recurring,
    settings: &Settings,
    now: NaiveDateTime,
    sent: &SentLog,
) -> Vec<Reminder> {
    planned(day, recurring)
        .into_iter()
        .filter_map(|occurrence| {
            let lead = occurrence.remind_before.unwrap_or(settings.remind_before);
            if lead.is_zero() {
                return None;
            }

            let starts_at = occurrence.date.and_time(occurrence.start);
            let window_opens = starts_at - TimeDelta::minutes(i64::from(lead.get()));
            if now < window_opens || now >= starts_at {
                return None;
            }

            let label = occurrence
                .block
                .as_ref()
                .map_or_else(|| occurrence.title.clone(), ToString::to_string);
            let key = format!(
                "{}T{} {label}",
                occurrence.date,
                occurrence.start.format("%H:%M"),
            );
            if sent.contains(&key) {
                return None;
            }

            Some(Reminder {
                key,
                title: if occurrence.title.is_empty() {
                    label
                } else {
                    occurrence.title.clone()
                },
                project: occurrence.project.clone(),
                starts_at,
                lead,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    const RECURRING: &str =
        "---\n---\n\n## Blocks\n\n- `deep-work` mon-fri 09:00-11:00 [[timemd]] Deep work !5m\n";

    fn date() -> NaiveDate {
        // A Wednesday.
        NaiveDate::from_ymd_opt(2026, 8, 5).expect("valid date")
    }

    fn moment(hour: u32, minute: u32) -> NaiveDateTime {
        date().and_hms_opt(hour, minute, 0).expect("valid time")
    }

    fn fixtures() -> (Day, Recurring, Settings) {
        (
            Day::new(date()),
            Recurring::parse(RECURRING).expect("parses"),
            Settings::default(),
        )
    }

    #[test]
    fn nothing_is_due_outside_the_lead_window() {
        let (day, recurring, settings) = fixtures();
        let sent = SentLog::default();

        assert!(due(&day, &recurring, &settings, moment(8, 50), &sent).is_empty());
    }

    #[test]
    fn a_block_is_due_inside_its_lead_window() {
        let (day, recurring, settings) = fixtures();
        let sent = SentLog::default();

        let reminders = due(&day, &recurring, &settings, moment(8, 56), &sent);
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].title, "Deep work");
        assert_eq!(reminders[0].lead, Minutes::new(5));
        assert_eq!(reminders[0].key, "2026-08-05T09:00 deep-work");
        assert_eq!(reminders[0].body(), "09:00 · timemd");
    }

    #[test]
    fn the_window_opens_exactly_at_the_lead_time() {
        let (day, recurring, settings) = fixtures();
        let sent = SentLog::default();

        assert_eq!(
            due(&day, &recurring, &settings, moment(8, 55), &sent).len(),
            1
        );
        assert!(due(&day, &recurring, &settings, moment(8, 54), &sent).is_empty());
    }

    /// A reminder that arrives after the block began is noise, so the window
    /// closes at the start rather than staying open.
    #[test]
    fn nothing_is_due_once_the_block_has_started() {
        let (day, recurring, settings) = fixtures();
        let sent = SentLog::default();

        assert!(due(&day, &recurring, &settings, moment(9, 0), &sent).is_empty());
        assert!(due(&day, &recurring, &settings, moment(9, 30), &sent).is_empty());
    }

    #[test]
    fn an_already_sent_reminder_is_not_due_again() {
        let (day, recurring, settings) = fixtures();
        let mut sent = SentLog::default();
        sent.record("2026-08-05T09:00 deep-work");

        assert!(due(&day, &recurring, &settings, moment(8, 56), &sent).is_empty());
    }

    #[test]
    fn a_skipped_block_is_never_due() {
        let (mut day, recurring, settings) = fixtures();
        day.skip(crate::ids::BlockId::new("deep-work").expect("valid id"));

        assert!(
            due(
                &day,
                &recurring,
                &settings,
                moment(8, 56),
                &SentLog::default()
            )
            .is_empty()
        );
    }

    #[test]
    fn a_one_off_block_uses_its_own_lead_and_its_title_as_the_key() {
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 16:00-17:00 [[reading]] Paper club !15m\n";
        let day = Day::parse(date(), text).expect("parses");
        let recurring = Recurring::default();

        let reminders = due(
            &day,
            &recurring,
            &Settings::default(),
            moment(15, 50),
            &SentLog::default(),
        );
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].key, "2026-08-05T16:00 Paper club");
        assert_eq!(reminders[0].lead, Minutes::new(15));
    }

    #[test]
    fn a_block_without_its_own_lead_takes_the_settings_default() {
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 16:00-17:00 Lunch\n";
        let day = Day::parse(date(), text).expect("parses");
        let settings = Settings::default();

        let reminders = due(
            &day,
            &Recurring::default(),
            &settings,
            moment(15, 56),
            &SentLog::default(),
        );
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].lead, settings.remind_before);
    }

    #[test]
    fn a_zero_lead_turns_reminders_off_for_that_block() {
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 16:00-17:00 Quiet block !0m\n";
        let day = Day::parse(date(), text).expect("parses");

        for minute in [45, 55, 59] {
            assert!(
                due(
                    &day,
                    &Recurring::default(),
                    &Settings::default(),
                    moment(15, minute),
                    &SentLog::default()
                )
                .is_empty(),
                "15:{minute} should be quiet"
            );
        }
    }

    #[test]
    fn a_reminder_without_a_project_says_when_it_starts() {
        let reminder = Reminder {
            key: "k".to_owned(),
            title: "Lunch".to_owned(),
            project: None,
            starts_at: moment(12, 0),
            lead: Minutes::new(5),
        };
        assert_eq!(reminder.body(), "starts at 12:00");
    }

    #[test]
    fn the_sent_log_round_trips() {
        let mut log = SentLog::default();
        log.record("2026-08-05T09:00 deep-work");
        log.record("2026-08-05T09:00 deep-work");
        assert_eq!(log.len(), 1);

        let rendered = log.render(moment(9, 0));
        let reparsed = SentLog::parse(&rendered).expect("parses");

        assert!(reparsed.contains("2026-08-05T09:00 deep-work"));
        assert_eq!(reparsed.len(), 1);
    }

    #[test]
    fn an_empty_log_reads_and_writes_cleanly() {
        let log = SentLog::default();
        assert!(log.is_empty());
        assert!(!log.contains("anything"));

        let reparsed = SentLog::parse(&log.render(moment(9, 0))).expect("parses");
        assert!(reparsed.is_empty());
    }

    #[test]
    fn old_entries_are_pruned_on_write() {
        let mut log = SentLog::default();
        log.record("2026-07-01T09:00 ancient");
        log.record("2026-08-05T09:00 deep-work");

        let reparsed = SentLog::parse(&log.render(moment(9, 0))).expect("parses");

        assert!(!reparsed.contains("2026-07-01T09:00 ancient"));
        assert!(reparsed.contains("2026-08-05T09:00 deep-work"));
    }
}
