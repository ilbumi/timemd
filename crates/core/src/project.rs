//! Project files: `data/projects/<slug>.md`.
//!
//! Project metadata genuinely is key/value, so it lives in frontmatter — unlike
//! sessions, whose canonical form is a body list. The body is free prose the app
//! never touches.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::Document;
use crate::error::ParseErrorKind;
use crate::ids::ProjectSlug;

/// A `#rrggbb` colour, used to tint the project in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Color(String);

impl Color {
    pub fn new(candidate: impl Into<String>) -> Result<Self, ParseErrorKind> {
        let candidate = candidate.into();
        let valid = candidate.len() == 7
            && candidate.starts_with('#')
            && candidate[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit());
        if valid {
            Ok(Self(candidate.to_ascii_lowercase()))
        } else {
            Err(ParseErrorKind::InvalidSlug { found: candidate })
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl FromStr for Color {
    type Err = ParseErrorKind;

    fn from_str(candidate: &str) -> Result<Self, Self::Err> {
        Self::new(candidate)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Active,
    Archived,
}

impl ProjectStatus {
    pub fn is_archived(self) -> bool {
        matches!(self, Self::Archived)
    }
}

impl FromStr for ProjectStatus {
    type Err = crate::error::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            other => Err(crate::error::Error::Invalid(format!(
                "unknown status {other:?}; expected `active` or `archived`"
            ))),
        }
    }
}

impl fmt::Display for ProjectStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Active => "active",
            Self::Archived => "archived",
        })
    }
}

/// A project, with its prose body and any agent-authored frontmatter intact.
#[derive(Debug, Clone)]
pub struct Project {
    slug: ProjectSlug,
    pub name: String,
    pub color: Option<Color>,
    pub status: ProjectStatus,
    pub created: Option<NaiveDate>,
    document: Document,
}

impl Project {
    pub fn new(slug: ProjectSlug, name: impl Into<String>, created: NaiveDate) -> Self {
        let name = name.into();
        let mut document = Document::new();
        document.set_preamble(vec![String::new(), format!("# {name}"), String::new()]);
        Self {
            slug,
            name,
            color: None,
            status: ProjectStatus::default(),
            created: Some(created),
            document,
        }
    }

    /// Reads a project file. Unreadable individual values fall back to defaults
    /// rather than failing the file.
    pub fn parse(slug: ProjectSlug, text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        Ok(Self {
            name: document
                .front_key::<String>("name")
                .unwrap_or_else(|| slug.to_string()),
            color: document.front_key("color"),
            status: document.front_key("status").unwrap_or_default(),
            created: document
                .front_key::<String>("created")
                .and_then(|raw| raw.parse().ok()),
            slug,
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();
        document.set_front_key("name", &self.name);
        match &self.color {
            Some(color) => document.set_front_key("color", color),
            None => document.remove_front_key("color"),
        }
        document.set_front_key("status", &self.status);
        match self.created {
            Some(created) => document.set_front_key("created", &created.to_string()),
            None => document.remove_front_key("created"),
        }
        document.render()
    }

    pub fn slug(&self) -> &ProjectSlug {
        &self.slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug() -> ProjectSlug {
        ProjectSlug::new("timemd").expect("valid slug")
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
    }

    const SAMPLE: &str = "---\nname: timemd\ncolor: '#4f46e5'\nstatus: active\ncreated: 2026-08-01\n---\n\n# timemd\n\nFree-form project notes.\n";

    #[test]
    fn parses_the_frontmatter() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        assert_eq!(project.name, "timemd");
        assert_eq!(project.color.as_ref().map(Color::as_str), Some("#4f46e5"));
        assert_eq!(project.status, ProjectStatus::Active);
        assert_eq!(project.created, Some(date()));
        assert_eq!(project.slug(), &slug());
    }

    #[test]
    fn round_trips_a_canonical_project() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        assert_eq!(project.render(), SAMPLE);
    }

    #[test]
    fn preserves_body_prose_and_unknown_keys_when_edited() {
        let source = "---\nname: timemd\nstatus: active\nnotion_id: abc-123\n---\n\n# timemd\n\nLong-standing notes.\n";
        let mut project = Project::parse(slug(), source).expect("parses");
        project.status = ProjectStatus::Archived;

        let rendered = project.render();
        assert!(rendered.contains("notion_id: abc-123"), "{rendered}");
        assert!(rendered.contains("Long-standing notes."), "{rendered}");
        assert!(rendered.contains("status: archived"), "{rendered}");
    }

    #[test]
    fn falls_back_to_the_slug_when_the_name_is_missing() {
        let project = Project::parse(slug(), "---\nstatus: active\n---\n").expect("parses");
        assert_eq!(project.name, "timemd");
    }

    #[test]
    fn falls_back_to_active_on_an_unrecognised_status() {
        let project = Project::parse(slug(), "---\nstatus: banana\n---\n").expect("parses");
        assert_eq!(project.status, ProjectStatus::Active);
    }

    #[test]
    fn drops_an_unreadable_colour_rather_than_failing() {
        let project = Project::parse(slug(), "---\ncolor: not-a-colour\n---\n").expect("parses");
        assert_eq!(project.color, None);
    }

    #[test]
    fn a_new_project_renders_a_template() {
        let project = Project::new(slug(), "timemd", date());
        assert_eq!(
            project.render(),
            "---\nname: timemd\nstatus: active\ncreated: 2026-08-01\n---\n\n# timemd\n"
        );
    }

    #[test]
    fn validates_colours() {
        assert!(Color::new("#4F46E5").is_ok());
        assert_eq!(Color::new("#4F46E5").expect("valid").as_str(), "#4f46e5");
        for candidate in ["4f46e5", "#4f46e", "#gggggg", "", "#4f46e55"] {
            assert!(
                Color::new(candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    #[test]
    fn archived_is_reported() {
        assert!(ProjectStatus::Archived.is_archived());
        assert!(!ProjectStatus::Active.is_archived());
    }

    #[test]
    fn statuses_round_trip_through_text() {
        assert_eq!(
            "active".parse::<ProjectStatus>().ok(),
            Some(ProjectStatus::Active)
        );
        assert_eq!(
            "archived".parse::<ProjectStatus>().ok(),
            Some(ProjectStatus::Archived)
        );
        assert_eq!(ProjectStatus::Active.to_string(), "active");
        assert_eq!(ProjectStatus::Archived.to_string(), "archived");
        assert!("hibernating".parse::<ProjectStatus>().is_err());
    }
}
