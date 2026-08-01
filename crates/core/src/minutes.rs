//! Minute-resolution durations.
//!
//! Everything in the files is whole minutes — pomodoro lengths, reminder lead
//! times, session totals. Modelling that directly (rather than reusing a
//! general-purpose duration) makes sub-minute values unrepresentable instead of
//! merely discouraged, and gives one canonical text form.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::ParseErrorKind;

const MINUTES_PER_HOUR: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Minutes(u32);

impl Minutes {
    pub const fn new(total: u32) -> Self {
        Self(total)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Splits into whole hours and the remaining minutes, for display.
    pub const fn split(self) -> (u32, u32) {
        (self.0 / MINUTES_PER_HOUR, self.0 % MINUTES_PER_HOUR)
    }
}

impl std::ops::Add for Minutes {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl std::iter::Sum for Minutes {
    fn sum<I: Iterator<Item = Self>>(iterator: I) -> Self {
        iterator.fold(Self(0), |total, next| total + next)
    }
}

/// Canonical form: `1h30m`, `2h`, `25m`, `0m`.
impl fmt::Display for Minutes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (hours, minutes) = self.split();
        match (hours, minutes) {
            (0, _) => write!(formatter, "{minutes}m"),
            (_, 0) => write!(formatter, "{hours}h"),
            _ => write!(formatter, "{hours}h{minutes}m"),
        }
    }
}

impl FromStr for Minutes {
    type Err = ParseErrorKind;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || ParseErrorKind::InvalidDuration {
            found: text.to_owned(),
        };

        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(invalid());
        }

        let mut total: u32 = 0;
        let mut digits = String::new();
        let mut saw_unit = false;

        for character in trimmed.chars() {
            match character {
                '0'..='9' => digits.push(character),
                'h' | 'm' => {
                    let value: u32 = digits.parse().map_err(|_| invalid())?;
                    digits.clear();
                    saw_unit = true;
                    let scaled = if character == 'h' {
                        value.checked_mul(MINUTES_PER_HOUR).ok_or_else(invalid)?
                    } else {
                        value
                    };
                    total = total.checked_add(scaled).ok_or_else(invalid)?;
                }
                _ => return Err(invalid()),
            }
        }

        // Trailing digits with no unit ("90") are rejected: an unlabelled number
        // is exactly the kind of ambiguity this format exists to avoid.
        if !digits.is_empty() || !saw_unit {
            return Err(invalid());
        }

        Ok(Self(total))
    }
}

impl Serialize for Minutes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Minutes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_canonical_forms() {
        assert_eq!("25m".parse(), Ok(Minutes::new(25)));
        assert_eq!("1h".parse(), Ok(Minutes::new(60)));
        assert_eq!("1h30m".parse(), Ok(Minutes::new(90)));
        assert_eq!("0m".parse(), Ok(Minutes::new(0)));
    }

    #[test]
    fn parses_non_canonical_but_unambiguous_input() {
        assert_eq!("90m".parse(), Ok(Minutes::new(90)));
        assert_eq!("2h0m".parse(), Ok(Minutes::new(120)));
    }

    #[test]
    fn rejects_unlabelled_and_malformed_input() {
        for candidate in ["", "90", "1h30", "h", "m", "1d", "-5m", "1.5h", "1 h"] {
            assert!(
                candidate.parse::<Minutes>().is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_overflow_instead_of_wrapping() {
        assert!("999999999999h".parse::<Minutes>().is_err());
    }

    #[test]
    fn renders_the_canonical_form() {
        assert_eq!(Minutes::new(25).to_string(), "25m");
        assert_eq!(Minutes::new(60).to_string(), "1h");
        assert_eq!(Minutes::new(90).to_string(), "1h30m");
        assert_eq!(Minutes::new(0).to_string(), "0m");
    }

    #[test]
    fn display_then_parse_is_identity() {
        for total in [0_u32, 1, 25, 59, 60, 61, 90, 120, 1439, 100_000] {
            let original = Minutes::new(total);
            assert_eq!(original.to_string().parse(), Ok(original));
        }
    }

    #[test]
    fn sums_without_overflowing() {
        let total: Minutes = [Minutes::new(25), Minutes::new(25), Minutes::new(40)]
            .into_iter()
            .sum();
        assert_eq!(total, Minutes::new(90));
        assert_eq!(Minutes::new(u32::MAX) + Minutes::new(10), Minutes::new(u32::MAX));
    }
}
