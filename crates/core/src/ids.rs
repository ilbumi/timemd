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

/// Length of a minted todo id. Six base-36 symbols is what Obsidian Tasks
/// generates, and it is short enough to type into a `⛔` list by hand.
const ID_LEN: usize = 6;

/// Canonical identity of a project. Equal to the stem of its markdown file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ProjectSlug(String);

/// Stable identity of a recurring schedule block, referenced by day files when
/// an occurrence is skipped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct BlockId(String);

/// Stable identity of a todo, written to the line as `🆔 dcf64c`.
///
/// Looser than a slug because the charset is not ours: Obsidian Tasks writes
/// these, and it uses mixed case and underscores. Nothing here becomes a
/// filename, so the traversal argument that narrows a slug does not apply.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct TodoId(String);

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

/// The todo-id rule: ASCII alphanumerics, dashes and underscores, non-empty,
/// bounded length. No case fold — an id is compared, never displayed as prose,
/// and lowering it would stop matching the file Obsidian wrote.
fn validate_id(candidate: &str) -> bool {
    !candidate.is_empty()
        && candidate.len() <= MAX_LEN
        && candidate
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

macro_rules! identifier {
    ($type:ty, $label:literal, $validate:path, $error:ident) => {
        impl $type {
            /// Validates and wraps an identifier.
            pub fn new(candidate: impl Into<String>) -> Result<Self, ParseErrorKind> {
                let candidate = candidate.into();
                if $validate(&candidate) {
                    Ok(Self(candidate))
                } else {
                    Err(ParseErrorKind::$error { found: candidate })
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

identifier!(ProjectSlug, "project slug", validate, InvalidSlug);
identifier!(BlockId, "block id", validate, InvalidSlug);
identifier!(TodoId, "todo id", validate_id, InvalidTodoId);

impl TodoId {
    /// Mints an id for a todo that has none, skipping any `taken`.
    ///
    /// Derived from the description by hashing rather than drawn at random, so
    /// there is no `rand` in core and a proptest that writes the same tree twice
    /// gets the same file. Collisions are resolved by salting and rehashing,
    /// which is also what makes two todos sharing a description addressable.
    pub fn mint(seed: &str, taken: impl Fn(&Self) -> bool) -> Self {
        use std::hash::{DefaultHasher, Hash, Hasher};

        for salt in 0u64.. {
            let mut hasher = DefaultHasher::new();
            seed.hash(&mut hasher);
            salt.hash(&mut hasher);
            let mut digest = hasher.finish();

            let mut id = String::with_capacity(ID_LEN);
            for _ in 0..ID_LEN {
                // Base 36 over `0-9a-z`, which `validate_id` accepts.
                let symbol = u32::try_from(digest % 36).expect("a remainder below 36");
                id.push(char::from_digit(symbol, 36).expect("a digit below the radix"));
                digest /= 36;
            }

            let candidate = Self(id);
            if !taken(&candidate) {
                return candidate;
            }
        }
        unreachable!("the salt range is exhausted only after 2^64 collisions")
    }
}

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
