//! `data/settings.md` — pomodoro lengths, timezone and reminder defaults.
//!
//! The timezone lives here and nowhere else. Files store bare wall-clock times,
//! so this single value is what turns them into instants.

use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Tz;

use crate::active::SessionKind;
use crate::document::Document;
use crate::minutes::Minutes;

const DEFAULT_FOCUS: Minutes = Minutes::new(25);
const DEFAULT_SHORT_BREAK: Minutes = Minutes::new(5);
const DEFAULT_LONG_BREAK: Minutes = Minutes::new(15);
const DEFAULT_LONG_BREAK_EVERY: u32 = 4;
const DEFAULT_REMIND_BEFORE: Minutes = Minutes::new(5);

#[derive(Debug, Clone)]
pub struct Settings {
    pub timezone: Tz,
    pub focus: Minutes,
    pub short_break: Minutes,
    pub long_break: Minutes,
    /// Number of focus sessions between long breaks.
    pub long_break_every: u32,
    /// Default reminder lead time for schedule blocks that do not set their own.
    pub remind_before: Minutes,
    document: Document,
}

impl Settings {
    /// Reads settings, falling back to a default for any value that is missing
    /// or unreadable.
    pub fn parse(text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        Ok(Self {
            timezone: document
                .front_key::<String>("timezone")
                .and_then(|raw| raw.parse().ok())
                .unwrap_or_else(system_timezone),
            // A zero-length block would start and immediately retire itself, so
            // it falls back like any other unusable value — the rule lives with
            // the value rather than with whichever writer happens to set it.
            focus: document
                .front_key("focus")
                .filter(usable)
                .unwrap_or(DEFAULT_FOCUS),
            short_break: document
                .front_key("short_break")
                .filter(usable)
                .unwrap_or(DEFAULT_SHORT_BREAK),
            long_break: document
                .front_key("long_break")
                .filter(usable)
                .unwrap_or(DEFAULT_LONG_BREAK),
            long_break_every: document
                .front_key::<u32>("long_break_every")
                .filter(|every| *every > 0)
                .unwrap_or(DEFAULT_LONG_BREAK_EVERY),
            remind_before: document
                .front_key("remind_before")
                .unwrap_or(DEFAULT_REMIND_BEFORE),
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();
        document.set_front_key("timezone", &self.timezone.name());
        document.set_front_key("focus", &self.focus);
        document.set_front_key("short_break", &self.short_break);
        document.set_front_key("long_break", &self.long_break);
        document.set_front_key("long_break_every", &self.long_break_every);
        document.set_front_key("remind_before", &self.remind_before);
        document.render()
    }

    /// Which break follows the `completed`-th focus session of the day, counting
    /// from one.
    ///
    /// Returns the kind rather than the length: two lengths that happen to be
    /// equal — which `short_break: 5m` and `long_break: 5m` legally are — must
    /// not make the app call a short break a long one.
    pub fn break_after(&self, completed: u32) -> SessionKind {
        if completed > 0 && completed % self.long_break_every == 0 {
            SessionKind::LongBreak
        } else {
            SessionKind::ShortBreak
        }
    }

    /// How long a block of this kind runs by default.
    pub fn length_of(&self, kind: SessionKind) -> Minutes {
        match kind {
            SessionKind::Focus => self.focus,
            SessionKind::ShortBreak => self.short_break,
            SessionKind::LongBreak => self.long_break,
        }
    }

    /// An instant as wall-clock time here.
    ///
    /// The timezone lives on the settings, so the conversion does too — a caller
    /// holding settings already can convert without reading the file again.
    pub fn wall_clock(&self, instant: DateTime<Utc>) -> NaiveDateTime {
        instant.with_timezone(&self.timezone).naive_local()
    }

    /// Applies `patch`, leaving every field it does not name alone.
    ///
    /// The write-side half of the rule `parse` reads leniently: a zero-length
    /// session would start and immediately retire itself, so `parse` falls back
    /// when it finds one and this refuses to write one. Refusing rather than
    /// silently defaulting is what tells the caller nothing happened.
    ///
    /// Here rather than at each surface because all three settable-length rules
    /// are the same rule, and the two surfaces that wrote the loop out for
    /// themselves both left the gate behind.
    pub fn apply(&mut self, patch: SettingsPatch) -> crate::error::Result<()> {
        for (length, name) in [
            (patch.focus, "focus"),
            (patch.short_break, "short_break"),
            (patch.long_break, "long_break"),
        ] {
            if length.is_some_and(Minutes::is_zero) {
                return Err(crate::error::Error::Invalid(format!(
                    "{name} must be more than zero minutes"
                )));
            }
        }

        if let Some(focus) = patch.focus {
            self.focus = focus;
        }
        if let Some(short_break) = patch.short_break {
            self.short_break = short_break;
        }
        if let Some(long_break) = patch.long_break {
            self.long_break = long_break;
        }
        // A zero lead is meaningful: remind me as it starts.
        if let Some(remind_before) = patch.remind_before {
            self.remind_before = remind_before;
        }
        Ok(())
    }
}

/// What may be changed about the settings. Every field omitted leaves the
/// setting exactly as it was.
///
/// `timezone` and `long_break_every` are absent deliberately: the timezone is
/// what turns every bare wall-clock time in the tree into an instant, so it is
/// read-only on every surface and changed by editing `settings.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsPatch {
    pub focus: Option<Minutes>,
    pub short_break: Option<Minutes>,
    pub long_break: Option<Minutes>,
    pub remind_before: Option<Minutes>,
}

impl SettingsPatch {
    /// True when there is nothing to write, so a caller can answer a pure read
    /// without touching a git-tracked file.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn usable(length: &Minutes) -> bool {
    !length.is_zero()
}

/// Compares the settings themselves, not the carried-through frontmatter.
///
/// Two `Settings` that would drive the app identically are equal even if one was
/// read from a file with extra keys and the other built from defaults.
impl PartialEq for Settings {
    fn eq(&self, other: &Self) -> bool {
        self.timezone == other.timezone
            && self.focus == other.focus
            && self.short_break == other.short_break
            && self.long_break == other.long_break
            && self.long_break_every == other.long_break_every
            && self.remind_before == other.remind_before
    }
}

impl Default for Settings {
    fn default() -> Self {
        let mut document = Document::new();
        document.set_preamble(vec![String::new(), "# Settings".to_owned(), String::new()]);
        Self {
            timezone: system_timezone(),
            focus: DEFAULT_FOCUS,
            short_break: DEFAULT_SHORT_BREAK,
            long_break: DEFAULT_LONG_BREAK,
            long_break_every: DEFAULT_LONG_BREAK_EVERY,
            remind_before: DEFAULT_REMIND_BEFORE,
            document,
        }
    }
}

/// The host's IANA timezone, or UTC if it cannot be determined.
fn system_timezone() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse().ok())
        .unwrap_or(Tz::UTC)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads are lenient: a zero-length block would start and immediately retire
    /// itself, so a hand-edited one falls back rather than breaking the timer.
    #[test]
    fn falls_back_on_a_zero_length_block() {
        let settings = Settings::parse("---\nfocus: 0m\nshort_break: 0m\nlong_break: 0m\n---\n")
            .expect("parses");
        assert_eq!(settings.focus, DEFAULT_FOCUS);
        assert_eq!(settings.short_break, DEFAULT_SHORT_BREAK);
        assert_eq!(settings.long_break, DEFAULT_LONG_BREAK);
    }

    #[test]
    fn reads_every_value() {
        let text = "---\ntimezone: Europe/Berlin\nfocus: 50m\nshort_break: 10m\nlong_break: 30m\nlong_break_every: 3\nremind_before: 15m\n---\n";
        let settings = Settings::parse(text).expect("parses");
        assert_eq!(settings.timezone, Tz::Europe__Berlin);
        assert_eq!(settings.focus, Minutes::new(50));
        assert_eq!(settings.short_break, Minutes::new(10));
        assert_eq!(settings.long_break, Minutes::new(30));
        assert_eq!(settings.long_break_every, 3);
        assert_eq!(settings.remind_before, Minutes::new(15));
    }

    #[test]
    fn falls_back_for_missing_and_unreadable_values() {
        let settings =
            Settings::parse("---\ntimezone: Not/AZone\nfocus: banana\n---\n").expect("parses");
        assert_eq!(settings.focus, DEFAULT_FOCUS);
        assert_eq!(settings.short_break, DEFAULT_SHORT_BREAK);
        assert_eq!(settings.long_break_every, DEFAULT_LONG_BREAK_EVERY);
    }

    #[test]
    fn rejects_a_zero_long_break_interval_that_would_divide_by_zero() {
        let settings = Settings::parse("---\nlong_break_every: 0\n---\n").expect("parses");
        assert_eq!(settings.long_break_every, DEFAULT_LONG_BREAK_EVERY);
        assert_eq!(settings.break_after(4), SessionKind::LongBreak);
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let text = "---\ntimezone: Europe/Berlin\nfocus: 50m\nshort_break: 10m\nlong_break: 30m\nlong_break_every: 3\nremind_before: 15m\n---\n";
        let settings = Settings::parse(text).expect("parses");
        let reparsed = Settings::parse(&settings.render()).expect("parses");
        assert_eq!(settings, reparsed);
    }

    #[test]
    fn preserves_unknown_keys() {
        let settings = Settings::parse("---\nfocus: 25m\ntheme: dark\n---\n").expect("parses");
        assert!(settings.render().contains("theme: dark"));
    }

    #[test]
    fn picks_the_long_break_on_the_interval() {
        let settings = Settings {
            long_break_every: 4,
            ..Settings::default()
        };
        assert_eq!(settings.break_after(1), SessionKind::ShortBreak);
        assert_eq!(settings.break_after(3), SessionKind::ShortBreak);
        assert_eq!(settings.break_after(4), SessionKind::LongBreak);
        assert_eq!(settings.break_after(8), SessionKind::LongBreak);
        assert_eq!(settings.break_after(0), SessionKind::ShortBreak);

        // Equal lengths must not blur the two kinds together.
        let same = Settings {
            short_break: DEFAULT_LONG_BREAK,
            ..Settings::default()
        };
        assert_eq!(same.length_of(same.break_after(1)), DEFAULT_LONG_BREAK);
        assert_eq!(same.break_after(1), SessionKind::ShortBreak);
    }

    #[test]
    fn defaults_render_and_reparse_identically() {
        let settings = Settings::default();
        assert_eq!(
            Settings::parse(&settings.render()).expect("parses"),
            settings
        );
    }

    /// The write-side half of the leniency `parse` shows a zero. Every surface
    /// goes through here, so none of them can put a length in the file that the
    /// next read would silently ignore.
    #[test]
    fn a_zero_session_length_is_refused_but_a_zero_lead_is_not() {
        for patch in [
            SettingsPatch {
                focus: Some(Minutes::new(0)),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                short_break: Some(Minutes::new(0)),
                ..SettingsPatch::default()
            },
            SettingsPatch {
                long_break: Some(Minutes::new(0)),
                ..SettingsPatch::default()
            },
        ] {
            let mut settings = Settings::default();
            assert!(settings.apply(patch.clone()).is_err(), "{patch:?}");
            assert_eq!(settings, Settings::default(), "a refusal changes nothing");
        }

        let mut settings = Settings::default();
        settings
            .apply(SettingsPatch {
                // Zero is a meaningful lead: remind me as the block starts.
                remind_before: Some(Minutes::new(0)),
                ..SettingsPatch::default()
            })
            .expect("a zero lead is allowed");
        assert_eq!(settings.remind_before, Minutes::new(0));
    }

    #[test]
    fn an_empty_patch_is_empty_and_changes_nothing() {
        let mut settings = Settings::default();
        assert!(SettingsPatch::default().is_empty());
        assert!(
            !SettingsPatch {
                focus: Some(Minutes::new(50)),
                ..SettingsPatch::default()
            }
            .is_empty()
        );

        settings.apply(SettingsPatch::default()).expect("applies");
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn a_patch_only_touches_the_lengths_it_names() {
        let mut settings = Settings::default();
        settings
            .apply(SettingsPatch {
                focus: Some(Minutes::new(50)),
                ..SettingsPatch::default()
            })
            .expect("applies");

        assert_eq!(settings.focus, Minutes::new(50));
        assert_eq!(settings.short_break, DEFAULT_SHORT_BREAK);
        assert_eq!(settings.long_break, DEFAULT_LONG_BREAK);
        assert_eq!(settings.remind_before, DEFAULT_REMIND_BEFORE);
    }
}
