//! Validated identifiers.
//!
//! A [`ProjectSlug`] becomes a filename, so its charset is the only thing
//! standing between a user-supplied name and a path traversal. Validation lives
//! in the constructor and nowhere else.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::error::ParseErrorKind;

const MAX_LEN: usize = 64;

/// Canonical identity of a project. Equal to the stem of its markdown file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProjectSlug(String);

/// Stable identity of a recurring schedule block, referenced by day files when
/// an occurrence is skipped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BlockId(String);

/// Shared rule: lowercase alphanumerics and dashes, no leading or trailing
/// dash, non-empty, bounded length.
fn validate(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= MAX_LEN
        && !candidate.starts_with('-')
        && !candidate.ends_with('-')
        && candidate.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Best-effort conversion of a human display name into a valid identifier.
fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.truncate(MAX_LEN);
    slug.trim_matches('-').to_owned()
}

macro_rules! identifier {
    ($type:ty, $label:literal) => {
        impl $type {
            /// Validates and wraps an identifier.
            pub fn new(candidate: impl Into<String>) -> Result<Self, ParseErrorKind> {
                let candidate = candidate.into();
                if validate(&candidate) {
                    Ok(Self(candidate))
                } else {
                    Err(ParseErrorKind::InvalidSlug { found: candidate })
                }
            }

            /// Derives an identifier from a human-facing name.
            ///
            /// Returns `None` when the name contains nothing usable, e.g. it is
            /// entirely punctuation.
            pub fn from_name(name: &str) -> Option<Self> {
                Self::new(slugify(name)).ok()
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $type {
            type Err = ParseErrorKind;

            fn from_str(candidate: &str) -> Result<Self, Self::Err> {
                Self::new(candidate)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                Self::new(raw).map_err(|_| serde::de::Error::custom(concat!("invalid ", $label)))
            }
        }
    };
}

identifier!(ProjectSlug, "project slug");
identifier!(BlockId, "block id");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_lowercase_alphanumeric_and_dashes() {
        assert!(ProjectSlug::new("timemd").is_ok());
        assert!(ProjectSlug::new("side-project-2").is_ok());
        assert!(ProjectSlug::new("a").is_ok());
    }

    #[test]
    fn rejects_uppercase_spaces_and_dots() {
        for candidate in ["TimeMD", "two words", "with.dot", "with_underscore"] {
            assert!(
                ProjectSlug::new(candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_path_traversal() {
        for candidate in ["..", "../etc", "/etc/passwd", "a/b"] {
            assert!(
                ProjectSlug::new(candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_empty_edge_dashes_and_overlong() {
        assert!(ProjectSlug::new("").is_err());
        assert!(ProjectSlug::new("-leading").is_err());
        assert!(ProjectSlug::new("trailing-").is_err());
        assert!(ProjectSlug::new("x".repeat(MAX_LEN + 1)).is_err());
        assert!(ProjectSlug::new("x".repeat(MAX_LEN)).is_ok());
    }

    #[test]
    fn derives_slug_from_display_name() {
        assert_eq!(
            ProjectSlug::from_name("Deep Work!").map(|slug| slug.as_str().to_owned()),
            Some("deep-work".to_owned())
        );
        assert_eq!(
            ProjectSlug::from_name("  Réview  2026 ").map(|slug| slug.as_str().to_owned()),
            Some("r-view-2026".to_owned())
        );
    }

    #[test]
    fn derives_nothing_from_an_unusable_name() {
        assert_eq!(ProjectSlug::from_name("!!!"), None);
        assert_eq!(ProjectSlug::from_name(""), None);
    }

    #[test]
    fn round_trips_through_string() {
        let slug: ProjectSlug = "timemd".parse().expect("valid");
        assert_eq!(slug.to_string(), "timemd");
        assert_eq!(slug.as_str(), "timemd");
    }

    #[test]
    fn block_ids_share_the_rule() {
        assert!(BlockId::new("deep-work").is_ok());
        assert!(BlockId::new("Deep Work").is_err());
    }
}
