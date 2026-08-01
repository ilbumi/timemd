//! `data/settings.md` — pomodoro lengths, timezone and reminder defaults.
//!
//! The timezone lives here and nowhere else. Files store bare wall-clock times,
//! so this single value is what turns them into instants.

use chrono_tz::Tz;

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
            focus: document.front_key("focus").unwrap_or(DEFAULT_FOCUS),
            short_break: document.front_key("short_break").unwrap_or(DEFAULT_SHORT_BREAK),
            long_break: document.front_key("long_break").unwrap_or(DEFAULT_LONG_BREAK),
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

    /// Length of the break that follows the `completed`-th focus session of the
    /// day, counting from one.
    pub fn break_after(&self, completed: u32) -> Minutes {
        if completed > 0 && completed % self.long_break_every == 0 {
            self.long_break
        } else {
            self.short_break
        }
    }
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
        let settings = Settings::parse("---\ntimezone: Not/AZone\nfocus: banana\n---\n").expect("parses");
        assert_eq!(settings.focus, DEFAULT_FOCUS);
        assert_eq!(settings.short_break, DEFAULT_SHORT_BREAK);
        assert_eq!(settings.long_break_every, DEFAULT_LONG_BREAK_EVERY);
    }

    #[test]
    fn rejects_a_zero_long_break_interval_that_would_divide_by_zero() {
        let settings = Settings::parse("---\nlong_break_every: 0\n---\n").expect("parses");
        assert_eq!(settings.long_break_every, DEFAULT_LONG_BREAK_EVERY);
        assert_eq!(settings.break_after(4), DEFAULT_LONG_BREAK);
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
        assert_eq!(settings.break_after(1), DEFAULT_SHORT_BREAK);
        assert_eq!(settings.break_after(3), DEFAULT_SHORT_BREAK);
        assert_eq!(settings.break_after(4), DEFAULT_LONG_BREAK);
        assert_eq!(settings.break_after(8), DEFAULT_LONG_BREAK);
        assert_eq!(settings.break_after(0), DEFAULT_SHORT_BREAK);
    }

    #[test]
    fn defaults_render_and_reparse_identically() {
        let settings = Settings::default();
        assert_eq!(Settings::parse(&settings.render()).expect("parses"), settings);
    }
}
