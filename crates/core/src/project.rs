//! Project files: `data/projects/<slug>.md`.
//!
//! Project metadata genuinely is key/value, so it lives in frontmatter — unlike
//! milestones, whose canonical form is a body list. The rest of the body is free
//! prose the app never touches.

use std::fmt;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::Document;
use crate::error::{ParseError, ParseErrorKind};
use crate::ids::ProjectSlug;
use crate::minutes::Minutes;

const SECTION_MILESTONES: &str = "Milestones";

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

/// The geometric mark a project is drawn as.
///
/// Colour alone stops distinguishing projects the moment two of them are similar
/// or the screen is glanced at from across a desk, so shape carries the identity
/// and colour reinforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mark {
    #[default]
    Square,
    Circle,
    Triangle,
    Diamond,
    Bar,
}

impl fmt::Display for Mark {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Square => "square",
            Self::Circle => "circle",
            Self::Triangle => "triangle",
            Self::Diamond => "diamond",
            Self::Bar => "bar",
        })
    }
}

impl FromStr for Mark {
    type Err = crate::error::Error;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        match raw {
            "square" => Ok(Self::Square),
            "circle" => Ok(Self::Circle),
            "triangle" => Ok(Self::Triangle),
            "diamond" => Ok(Self::Diamond),
            "bar" => Ok(Self::Bar),
            other => Err(crate::error::Error::Invalid(format!(
                "unknown mark {other:?}; expected square, circle, triangle, diamond or bar"
            ))),
        }
    }
}

/// One entry in a project's `## Milestones` list.
///
/// Deliberately just a checkbox and a title: a milestone is a thing you can tick
/// off at the end of a session, not a task manager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Milestone {
    pub done: bool,
    pub title: String,
}

impl Milestone {
    pub fn new(done: bool, title: impl Into<String>) -> Self {
        Self {
            done,
            title: title.into(),
        }
    }

    /// Reads `[x] Title` or `[ ] Title`, the bullet already stripped.
    fn parse(content: &str) -> Result<Self, ParseErrorKind> {
        let invalid = || ParseErrorKind::MissingCheckbox {
            found: content.to_owned(),
        };

        let (box_text, title) = content.split_at_checked(3).ok_or_else(invalid)?;
        let done = match box_text {
            "[x]" | "[X]" => true,
            "[ ]" => false,
            _ => return Err(invalid()),
        };

        let title = title.trim();
        if title.is_empty() {
            return Err(invalid());
        }
        Ok(Self::new(done, title))
    }

    fn render(&self) -> String {
        format!("- [{}] {}", if self.done { "x" } else { " " }, self.title)
    }
}

/// A project, with its prose body and any agent-authored frontmatter intact.
#[derive(Debug, Clone)]
pub struct Project {
    slug: ProjectSlug,
    pub name: String,
    pub color: Option<Color>,
    pub mark: Mark,
    /// Hours to spend on this project each week, or `None` for no target.
    pub target: Option<Minutes>,
    pub status: ProjectStatus,
    pub created: Option<NaiveDate>,
    pub milestones: Vec<Milestone>,
    /// Milestone lines that failed to parse, kept verbatim and re-emitted at the
    /// end of the section rather than being dropped.
    unparsed_milestones: Vec<String>,
    problems: Vec<ParseError>,
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
            mark: Mark::default(),
            target: None,
            status: ProjectStatus::default(),
            created: Some(created),
            milestones: Vec::new(),
            unparsed_milestones: Vec::new(),
            problems: Vec::new(),
            document,
        }
    }

    /// Reads a project file. Unreadable individual values fall back to defaults
    /// rather than failing the file.
    pub fn parse(slug: ProjectSlug, text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let mut problems = Vec::new();
        let (milestones, unparsed_milestones) =
            document.parse_list_section(SECTION_MILESTONES, Milestone::parse, &mut problems);

        Ok(Self {
            name: document
                .front_key::<String>("name")
                .unwrap_or_else(|| slug.to_string()),
            color: document.front_key("color"),
            mark: document.front_key("mark").unwrap_or_default(),
            target: document.front_key("target"),
            status: document.front_key("status").unwrap_or_default(),
            created: document
                .front_key::<String>("created")
                .and_then(|raw| raw.parse().ok()),
            slug,
            milestones,
            unparsed_milestones,
            problems,
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
        document.set_front_key("mark", &self.mark);
        match self.target {
            Some(target) => document.set_front_key("target", &target),
            None => document.remove_front_key("target"),
        }
        document.set_front_key("status", &self.status);
        match self.created {
            Some(created) => document.set_front_key("created", &created.to_string()),
            None => document.remove_front_key("created"),
        }
        document.write_list_section(
            SECTION_MILESTONES,
            &self.milestones,
            &self.unparsed_milestones,
            &[],
            Milestone::render,
        );
        document.render()
    }

    pub fn slug(&self) -> &ProjectSlug {
        &self.slug
    }

    /// Milestone lines the app could not read, so a broken file is visible
    /// rather than silently half-loaded.
    pub fn problems(&self) -> &[ParseError] {
        &self.problems
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

    const SAMPLE: &str = "---\nname: timemd\ncolor: '#4f46e5'\nmark: square\ntarget: 10h\nstatus: active\ncreated: 2026-08-01\n---\n\n# timemd\n\nFree-form project notes.\n\n## Milestones\n\n- [x] Ch. 1 — lit review\n- [ ] Ch. 4 — first draft\n";

    #[test]
    fn parses_the_frontmatter() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        assert_eq!(project.name, "timemd");
        assert_eq!(project.color.as_ref().map(Color::as_str), Some("#4f46e5"));
        assert_eq!(project.mark, Mark::Square);
        assert_eq!(project.target, Some(Minutes::new(600)));
        assert_eq!(project.status, ProjectStatus::Active);
        assert_eq!(project.created, Some(date()));
        assert_eq!(project.slug(), &slug());
    }

    #[test]
    fn parses_the_milestone_list() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        assert_eq!(
            project.milestones,
            vec![
                Milestone::new(true, "Ch. 1 — lit review"),
                Milestone::new(false, "Ch. 4 — first draft"),
            ]
        );
        assert!(project.problems().is_empty());
    }

    #[test]
    fn keeps_and_reports_a_malformed_milestone_line() {
        let source = "---\nname: timemd\n---\n\n## Milestones\n\n- [x] done\n- forgot the box\n";
        let project = Project::parse(slug(), source).expect("parses");

        assert_eq!(project.milestones, vec![Milestone::new(true, "done")]);
        assert_eq!(project.problems().len(), 1);
        assert!(
            project.render().contains("- forgot the box"),
            "{}",
            project.render()
        );
    }

    #[test]
    fn writing_milestones_leaves_the_prose_alone() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");
        project.milestones.push(Milestone::new(false, "Ch. 5"));

        let rendered = project.render();
        assert!(rendered.contains("- [ ] Ch. 5"), "{rendered}");
        assert!(rendered.contains("Free-form project notes."), "{rendered}");
    }

    #[test]
    fn falls_back_to_a_square_on_an_unrecognised_mark() {
        let project = Project::parse(slug(), "---\nmark: hexagon\n---\n").expect("parses");
        assert_eq!(project.mark, Mark::Square);
    }

    #[test]
    fn drops_an_unreadable_target_rather_than_failing() {
        let project = Project::parse(slug(), "---\ntarget: loads\n---\n").expect("parses");
        assert_eq!(project.target, None);
    }

    #[test]
    fn marks_round_trip_through_text() {
        for mark in [
            Mark::Square,
            Mark::Circle,
            Mark::Triangle,
            Mark::Diamond,
            Mark::Bar,
        ] {
            assert_eq!(mark.to_string().parse().ok(), Some(mark));
        }
        assert!("hexagon".parse::<Mark>().is_err());
    }

    #[test]
    fn milestones_round_trip_through_their_line() {
        for milestone in [Milestone::new(true, "done"), Milestone::new(false, "todo")] {
            let line = milestone.render();
            let content = crate::grammar::list_item(&line).expect("is a list item");
            assert_eq!(Milestone::parse(content), Ok(milestone));
        }
    }

    #[test]
    fn rejects_a_milestone_with_no_title() {
        for candidate in ["[x]", "[ ]", "[x] ", "[y] mistyped", "no box at all"] {
            assert!(
                Milestone::parse(candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
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
            "---\nname: timemd\nmark: square\nstatus: active\ncreated: 2026-08-01\n---\n\n# timemd\n"
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
