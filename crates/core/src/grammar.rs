//! Shared scanners for the list-item grammars.
//!
//! Every structured line in the tree is a markdown list item built from the
//! same handful of pieces — a time range, an optional parenthesised duration, an
//! optional `[[project]]` wikilink, an optional `!5m` reminder lead, and free
//! text. Keeping the scanners here means the day and schedule grammars cannot
//! drift apart.

use chrono::{NaiveTime, Timelike};

use crate::error::ParseErrorKind;
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;

const SECONDS_PER_DAY: u32 = 24 * 60 * 60;

/// Strips the markdown bullet, returning the item's content.
pub fn list_item(line: &str) -> Result<&str, ParseErrorKind> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .map(str::trim)
        .ok_or_else(|| ParseErrorKind::NotAListItem {
            found: line.trim().to_owned(),
        })
}

pub fn time(text: &str) -> Result<NaiveTime, ParseErrorKind> {
    let invalid = || ParseErrorKind::InvalidTime {
        found: text.to_owned(),
    };
    let (hours, minutes) = text.split_once(':').ok_or_else(invalid)?;
    if hours.len() != 2 || minutes.len() != 2 {
        return Err(invalid());
    }
    let hours: u32 = hours.parse().map_err(|_| invalid())?;
    let minutes: u32 = minutes.parse().map_err(|_| invalid())?;
    NaiveTime::from_hms_opt(hours, minutes, 0).ok_or_else(invalid)
}

pub fn format_time(value: NaiveTime) -> String {
    format!("{:02}:{:02}", value.hour(), value.minute())
}

/// Consumes a leading `HH:MM-HH:MM`, returning the range and the remainder.
pub fn time_range(text: &str) -> Result<((NaiveTime, NaiveTime), &str), ParseErrorKind> {
    let (head, rest) = split_token(text);
    let (start, end) = head
        .split_once('-')
        .ok_or_else(|| ParseErrorKind::MissingTimeRange {
            found: text.to_owned(),
        })?;
    Ok(((time(start)?, time(end)?), rest))
}

/// Wall-clock minutes between two times, treating `end < start` as crossing
/// midnight.
///
/// This is deliberately wall-clock arithmetic: the files carry no UTC offsets,
/// which is what keeps them readable and hand-editable. The cost is that the
/// two clock changes a year distort those days' totals by an hour.
pub fn span(start: NaiveTime, end: NaiveTime) -> Minutes {
    let start = start.num_seconds_from_midnight();
    let end = end.num_seconds_from_midnight();
    let seconds = if end >= start {
        end - start
    } else {
        end + SECONDS_PER_DAY - start
    };
    Minutes::new(seconds / 60)
}

/// Skips a leading `(...)` group when it parses as a duration.
///
/// The duration is derived from the times, so it is display-only. Requiring it
/// to actually parse means a note that merely starts with a parenthesis —
/// `(draft) rewrite the intro` — survives as note text.
pub fn skip_duration_group(text: &str) -> &str {
    let Some(rest) = text.strip_prefix('(') else {
        return text;
    };
    let Some(close) = rest.find(')') else {
        return text;
    };
    if rest[..close].parse::<Minutes>().is_ok() {
        rest[close + 1..].trim_start()
    } else {
        text
    }
}

/// Consumes a leading `[[slug]]` wikilink.
///
/// An unparseable slug is left in place as note text rather than failing the
/// line, per the lenient-read rule.
pub fn wikilink(text: &str) -> (Option<ProjectSlug>, &str) {
    let Some(rest) = text.strip_prefix("[[") else {
        return (None, text);
    };
    let Some(close) = rest.find("]]") else {
        return (None, text);
    };
    match ProjectSlug::new(&rest[..close]) {
        Ok(slug) => (Some(slug), rest[close + 2..].trim_start()),
        Err(_) => (None, text),
    }
}

pub fn format_wikilink(project: Option<&ProjectSlug>) -> String {
    project.map_or_else(String::new, |slug| format!("[[{slug}]] "))
}

/// Consumes a trailing `!5m` reminder lead time.
pub fn reminder_suffix(text: &str) -> (Option<Minutes>, &str) {
    let trimmed = text.trim_end();
    let Some((head, last)) = trimmed.rsplit_once(char::is_whitespace) else {
        return match trimmed.strip_prefix('!').map(str::parse::<Minutes>) {
            Some(Ok(lead)) => (Some(lead), ""),
            _ => (None, trimmed),
        };
    };
    match last.strip_prefix('!').map(str::parse::<Minutes>) {
        Some(Ok(lead)) => (Some(lead), head.trim_end()),
        _ => (None, trimmed),
    }
}

/// Splits off the first whitespace-delimited token.
pub fn split_token(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    match text.find(char::is_whitespace) {
        Some(index) => (&text[..index], text[index..].trim_start()),
        None => (text, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    #[test]
    fn strips_either_bullet_style() {
        assert_eq!(list_item("- content"), Ok("content"));
        assert_eq!(list_item("  * content  "), Ok("content"));
        assert!(list_item("content").is_err());
        assert!(list_item("-no space").is_err());
    }

    #[test]
    fn parses_zero_padded_times_only() {
        assert_eq!(time("09:00"), Ok(at(9, 0)));
        assert_eq!(time("23:59"), Ok(at(23, 59)));
        for candidate in ["9:00", "09:0", "24:00", "09:60", "0900", "ab:cd", ""] {
            assert!(time(candidate).is_err(), "{candidate:?} should be rejected");
        }
    }

    #[test]
    fn consumes_a_time_range_and_returns_the_rest() {
        let ((start, end), rest) = time_range("09:00-09:25 (25m) [[timemd]] note").expect("parses");
        assert_eq!((start, end), (at(9, 0), at(9, 25)));
        assert_eq!(rest, "(25m) [[timemd]] note");
    }

    #[test]
    fn spans_forward_and_across_midnight() {
        assert_eq!(span(at(9, 0), at(9, 25)), Minutes::new(25));
        assert_eq!(span(at(23, 50), at(0, 20)), Minutes::new(30));
        assert_eq!(span(at(9, 0), at(9, 0)), Minutes::new(0));
    }

    #[test]
    fn skips_a_real_duration_group_but_keeps_a_parenthesised_note() {
        assert_eq!(skip_duration_group("(25m) rest"), "rest");
        assert_eq!(skip_duration_group("(1h30m) rest"), "rest");
        assert_eq!(skip_duration_group("(draft) rewrite"), "(draft) rewrite");
        assert_eq!(skip_duration_group("(unclosed rest"), "(unclosed rest");
        assert_eq!(skip_duration_group("no group"), "no group");
    }

    #[test]
    fn consumes_a_wikilink_when_the_slug_is_valid() {
        let (project, rest) = wikilink("[[timemd]] file store");
        assert_eq!(
            project.map(|slug| slug.to_string()),
            Some("timemd".to_owned())
        );
        assert_eq!(rest, "file store");
    }

    #[test]
    fn leaves_an_invalid_or_absent_wikilink_as_note_text() {
        assert_eq!(
            wikilink("[[Not A Slug]] rest"),
            (None, "[[Not A Slug]] rest")
        );
        assert_eq!(wikilink("[[unclosed rest"), (None, "[[unclosed rest"));
        assert_eq!(wikilink("plain note"), (None, "plain note"));
    }

    #[test]
    fn consumes_a_trailing_reminder_lead() {
        assert_eq!(
            reminder_suffix("Deep work !5m"),
            (Some(Minutes::new(5)), "Deep work")
        );
        assert_eq!(reminder_suffix("!15m"), (Some(Minutes::new(15)), ""));
        assert_eq!(reminder_suffix("Deep work"), (None, "Deep work"));
        assert_eq!(
            reminder_suffix("Deep work !soon"),
            (None, "Deep work !soon")
        );
    }
}
