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
    /// Private so that `new` really is the only way a title reaches a file.
    /// `done` stays public: a bool has no rule to break.
    title: String,
}

impl Milestone {
    /// The one way to build a milestone, and so the one place the write-side
    /// rule lives: a title that is blank or spans lines could not be rendered
    /// and read back, so it is refused rather than written.
    pub fn new(done: bool, title: impl AsRef<str>) -> Result<Self, ParseErrorKind> {
        let title = title.as_ref().trim();
        if title.is_empty() || title.contains(['\n', '\r']) {
            return Err(ParseErrorKind::MissingCheckbox {
                found: title.to_owned(),
            });
        }
        Ok(Self {
            done,
            title: title.to_owned(),
        })
    }

    pub fn title(&self) -> &str {
        &self.title
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
        Self::new(done, title).map_err(|_| invalid())
    }

    fn render(&self) -> String {
        // No trailing trim: `new` is the only constructor and it rejects a blank
        // title, so the line always ends in one.
        format!("- [{}] {}", if self.done { "x" } else { " " }, self.title)
    }
}

/// What [`Project::update_milestone`] may change about one milestone. Every
/// field omitted leaves it exactly as it was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MilestoneEdit {
    pub done: Option<bool>,
    pub title: Option<String>,
    pub position: Option<usize>,
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

    /// The position of the one milestone carrying this title.
    ///
    /// The addressing scheme for every surface that does not hold the list in
    /// hand. A milestone has no id — [`docs/format.md`] keeps it that way
    /// deliberately — and an index is a position another writer can invalidate
    /// between two calls, whereas a title is what the agent, the file and the
    /// user all already see.
    ///
    /// Reads stay lenient, so a file with the same title twice parses and lists
    /// fine. Writes are strict, so addressing one of them is refused rather than
    /// resolved by picking whichever came first.
    pub fn milestone_titled(&self, title: &str) -> crate::error::Result<usize> {
        let title = title.trim();
        let mut found = self
            .milestones
            .iter()
            .enumerate()
            .filter(|(_, milestone)| milestone.title == title);

        match (found.next(), found.count()) {
            (Some((index, _)), 0) => Ok(index),
            (None, _) => Err(crate::error::Error::Invalid(format!(
                "no milestone titled {title:?} on {}",
                self.slug
            ))),
            (Some(_), rest) => Err(crate::error::Error::Invalid(format!(
                "{} milestones on {} are titled {title:?}; rename one first",
                rest + 1,
                self.slug,
            ))),
        }
    }

    /// Refuses a title already carried by a milestone other than the one at
    /// `except`.
    ///
    /// The other half of [`Project::milestone_titled`], and the reason it can
    /// promise a single match: a title two milestones share is addressable by
    /// nobody, so no surface may write one. Enforced here rather than at each
    /// caller because it is the same rule for adding, renaming and replacing
    /// the whole list, and the surface that replaces the whole list is the one
    /// that had forgotten it.
    fn refuse_duplicate(&self, title: &str, except: Option<usize>) -> crate::error::Result<()> {
        let taken = self
            .milestones
            .iter()
            .enumerate()
            .any(|(index, milestone)| milestone.title == title && Some(index) != except);

        if taken {
            return Err(crate::error::Error::Invalid(format!(
                "{} already has a milestone titled {title:?}",
                self.slug
            )));
        }
        Ok(())
    }

    /// Adds a milestone at `position`, or last when `position` is past the end.
    /// Returns where it landed.
    ///
    /// Here rather than at the caller for the clamp: `Vec::insert` panics past
    /// the end, and three surfaces each writing `position.min(len)` is three
    /// chances to forget.
    pub fn insert_milestone(
        &mut self,
        position: usize,
        milestone: Milestone,
    ) -> crate::error::Result<usize> {
        self.refuse_duplicate(milestone.title(), None)?;
        let position = position.min(self.milestones.len());
        self.milestones.insert(position, milestone);
        Ok(position)
    }

    /// Removes the milestone carrying `title`, giving it back.
    ///
    /// Here rather than at the caller because MCP and the shell had each
    /// written the same two lines — look the title up, then remove that index —
    /// and that second line is the only reason the list needed to be public.
    /// Addressing is one rule, so it belongs behind one door.
    pub fn remove_milestone(&mut self, title: &str) -> crate::error::Result<Milestone> {
        let index = self.milestone_titled(title)?;
        Ok(self.milestones.remove(index))
    }

    /// Replaces the whole list, refusing one that carries a title twice.
    ///
    /// The door for the whole-list `PATCH`: the web app holds the list and
    /// sends it back entire, which is the one shape that can introduce a
    /// duplicate without ever naming a milestone.
    pub fn set_milestones(&mut self, milestones: Vec<Milestone>) -> crate::error::Result<()> {
        for (index, milestone) in milestones.iter().enumerate() {
            if let Some(clash) = milestones[..index]
                .iter()
                .find(|earlier| earlier.title == milestone.title)
            {
                return Err(crate::error::Error::Invalid(format!(
                    "{} would have two milestones titled {:?}",
                    self.slug,
                    clash.title()
                )));
            }
        }
        self.milestones = milestones;
        Ok(())
    }

    /// Retitles the milestone at `index`.
    ///
    /// Here rather than at the caller because [`Milestone::new`] is the
    /// write-side gate and `Milestone::title` is private, so this is the only
    /// door in — a rename cannot walk past the rule the way a field assignment
    /// once could.
    pub fn rename_milestone(
        &mut self,
        index: usize,
        title: impl AsRef<str>,
    ) -> crate::error::Result<()> {
        let renamed = Milestone::new(
            self.milestones
                .get(index)
                .ok_or_else(|| {
                    crate::error::Error::Invalid(format!(
                        "no milestone at index {index} on {}",
                        self.slug
                    ))
                })?
                .done,
            title,
        )?;
        self.refuse_duplicate(renamed.title(), Some(index))?;
        self.milestones[index] = renamed;
        Ok(())
    }

    /// Retitles, ticks and moves the milestone carrying `title`, in that order,
    /// returning where it ended up.
    ///
    /// One method rather than one per verb because the three share an address,
    /// a transaction and an ordering — `position` is read against the list as
    /// it stands *after* the rename — and that ordering is a decision to make
    /// once. MCP and the CLI had each made it separately.
    ///
    /// The rename goes first because it is the only step that can refuse, and a
    /// refusal must not leave half of itself behind: ticking first handed the
    /// caller an error over a project that was already ticked, and whatever
    /// summarised that same `&mut Project` afterwards reported the tick.
    pub fn update_milestone(
        &mut self,
        title: &str,
        edit: MilestoneEdit,
    ) -> crate::error::Result<usize> {
        let index = self.milestone_titled(title)?;

        if let Some(new_title) = &edit.title {
            self.rename_milestone(index, new_title)?;
        }
        // `rename_milestone` carries the existing tick forward, so applying it
        // after the rename lands the same value it would have before.
        if let Some(done) = edit.done {
            self.milestones[index].done = done;
        }
        match edit.position {
            Some(position) => Ok(self.move_milestone(index, position).unwrap_or(index)),
            None => Ok(index),
        }
    }

    /// Moves the milestone at `from` to sit at `to` in the *resulting* list,
    /// landing it last when `to` is past the end. Returns where it landed, or
    /// `None` when `from` names nothing.
    ///
    /// Here rather than at the caller because "`to` in the old list's
    /// coordinates or the new one's" is a decision that has to be made once and
    /// tested once: remove-then-insert is off by one in exactly one direction.
    pub fn move_milestone(&mut self, from: usize, to: usize) -> Option<usize> {
        if from >= self.milestones.len() {
            return None;
        }
        let milestone = self.milestones.remove(from);
        // `remove` already shifted everything after `from` down, so `to` is
        // read against the shortened list — which is the resulting list.
        let to = to.min(self.milestones.len());
        self.milestones.insert(to, milestone);
        Some(to)
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

    fn milestone(done: bool, title: &str) -> Milestone {
        Milestone::new(done, title).expect("valid milestone")
    }

    fn titles(project: &Project) -> Vec<&str> {
        project
            .milestones
            .iter()
            .map(Milestone::title)
            .collect::<Vec<_>>()
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
                milestone(true, "Ch. 1 — lit review"),
                milestone(false, "Ch. 4 — first draft"),
            ]
        );
        assert!(project.problems().is_empty());
    }

    #[test]
    fn keeps_and_reports_a_malformed_milestone_line() {
        let source = "---\nname: timemd\n---\n\n## Milestones\n\n- [x] done\n- forgot the box\n";
        let project = Project::parse(slug(), source).expect("parses");

        assert_eq!(project.milestones, vec![milestone(true, "done")]);
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
        project.milestones.push(milestone(false, "Ch. 5"));

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
        for entry in [milestone(true, "done"), milestone(false, "todo")] {
            let line = entry.render();
            let content = crate::grammar::list_item(&line).expect("is a list item");
            assert_eq!(Milestone::parse(content), Ok(entry));
        }
    }

    #[test]
    fn rejects_a_milestone_line_with_no_checkbox_or_no_title() {
        for candidate in ["[x]", "[ ]", "[x] ", "[y] mistyped", "no box at all"] {
            assert!(
                Milestone::parse(candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    /// Writes are strict, and this is where that is enforced for milestones: a
    /// title the reader could not get back must never reach a file.
    #[test]
    fn refuses_to_build_a_milestone_it_could_not_write() {
        for candidate in ["", "   ", "two\nlines", "carriage\rreturn"] {
            assert!(
                Milestone::new(false, candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
        assert_eq!(milestone(true, "  padded  ").title, "padded");
    }

    /// The addressing scheme every surface that does not hold the list uses.
    #[test]
    fn finds_a_milestone_by_its_title() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");

        assert_eq!(project.milestone_titled("Ch. 1 — lit review").ok(), Some(0));
        assert_eq!(
            project.milestone_titled("Ch. 4 — first draft").ok(),
            Some(1)
        );
    }

    /// `Milestone::new` trims, so a lookup that did not would never match a
    /// title the caller read back from us and handed straight in again.
    #[test]
    fn matches_a_title_after_trimming_it() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        assert_eq!(
            project.milestone_titled("  Ch. 1 — lit review  ").ok(),
            Some(0)
        );
    }

    #[test]
    fn refuses_a_title_that_names_no_milestone() {
        let project = Project::parse(slug(), SAMPLE).expect("parses");
        let error = project
            .milestone_titled("Ch. 9")
            .expect_err("no such title");
        assert!(error.to_string().contains("Ch. 9"), "{error}");
    }

    /// Reads are lenient, so a hand-written duplicate parses and lists fine.
    /// Writes are strict, so addressing one is refused rather than resolved by
    /// picking whichever came first.
    #[test]
    fn refuses_a_title_two_milestones_share() {
        let source = "---\nname: timemd\n---\n\n## Milestones\n\n- [ ] Ch. 4\n- [x] Ch. 4\n";
        let project = Project::parse(slug(), source).expect("parses");

        assert_eq!(project.milestones.len(), 2);
        let error = project.milestone_titled("Ch. 4").expect_err("ambiguous");
        assert!(error.to_string().contains('2'), "{error}");
    }

    /// The other half of addressing by title, and the last reason the list
    /// itself had to be reachable from outside core.
    #[test]
    fn removes_a_milestone_by_its_title() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        let removed = project
            .remove_milestone("Ch. 1 — lit review")
            .expect("removes");
        assert_eq!(removed.title(), "Ch. 1 — lit review");
        assert!(removed.done);
        assert_eq!(titles(&project), ["Ch. 4 — first draft"]);

        let error = project.remove_milestone("Ch. 9").expect_err("no such title");
        assert!(error.to_string().contains("Ch. 9"), "{error}");
        assert_eq!(
            titles(&project),
            ["Ch. 4 — first draft"],
            "a refused removal changes nothing"
        );
    }

    #[test]
    fn inserts_a_milestone_at_a_position_and_past_the_end() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        assert_eq!(
            project.insert_milestone(0, milestone(false, "Ch. 0")).ok(),
            Some(0)
        );
        assert_eq!(
            project.insert_milestone(99, milestone(false, "Ch. 9")).ok(),
            Some(3)
        );
        assert_eq!(
            titles(&project),
            [
                "Ch. 0",
                "Ch. 1 — lit review",
                "Ch. 4 — first draft",
                "Ch. 9"
            ]
        );
    }

    /// A title two milestones share is addressable by nobody, so every door in
    /// refuses one — including the whole-list replace, which is the door the
    /// web app uses and the only one that can collide without naming anything.
    #[test]
    fn no_door_writes_a_title_twice() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        assert!(
            project
                .insert_milestone(0, milestone(false, "Ch. 4 — first draft"))
                .is_err(),
            "adding a duplicate"
        );
        assert!(
            project.rename_milestone(0, "Ch. 4 — first draft").is_err(),
            "renaming onto a duplicate"
        );
        assert!(
            project
                .update_milestone(
                    "Ch. 1 — lit review",
                    MilestoneEdit {
                        title: Some("Ch. 4 — first draft".to_owned()),
                        ..MilestoneEdit::default()
                    }
                )
                .is_err(),
            "updating onto a duplicate"
        );
        assert!(
            project
                .set_milestones(vec![milestone(false, "same"), milestone(true, "same")])
                .is_err(),
            "replacing the whole list with a duplicate"
        );

        assert_eq!(
            titles(&project),
            ["Ch. 1 — lit review", "Ch. 4 — first draft"],
            "a refused write changes nothing"
        );

        project
            .rename_milestone(0, "Ch. 1 — lit review")
            .expect("renaming a milestone to the title it already has");
    }

    /// Tick, retitle and move are one call because they share an address and a
    /// transaction — and because `position` has to be read against the list as
    /// it stands after the rename.
    #[test]
    fn one_update_ticks_retitles_and_moves() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        let landed = project
            .update_milestone(
                "Ch. 1 — lit review",
                MilestoneEdit {
                    done: Some(false),
                    title: Some("Ch. 1".to_owned()),
                    position: Some(1),
                },
            )
            .expect("updates");

        assert_eq!(landed, 1);
        assert_eq!(titles(&project), ["Ch. 4 — first draft", "Ch. 1"]);
        assert!(!project.milestones[1].done);

        assert!(
            project
                .update_milestone("Ch. 9", MilestoneEdit::default())
                .is_err(),
            "a title nothing carries"
        );
    }

    /// A refusal must not leave half of itself behind. The tick is applied by
    /// assignment and cannot fail; the rename can, so it goes first — otherwise
    /// the caller is handed an error holding a project that was already ticked,
    /// and anything summarising that same `&mut Project` reports the tick.
    #[test]
    fn a_refused_rename_leaves_the_tick_alone() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        let error = project
            .update_milestone(
                "Ch. 4 — first draft",
                MilestoneEdit {
                    done: Some(true),
                    title: Some("Ch. 1 — lit review".to_owned()),
                    position: None,
                },
            )
            .expect_err("the title is taken");
        assert!(error.to_string().contains("Ch. 1 — lit review"), "{error}");

        assert!(!project.milestones[1].done, "the tick must not have landed");
        assert_eq!(
            titles(&project),
            ["Ch. 1 — lit review", "Ch. 4 — first draft"]
        );
    }

    /// Renaming is the only reason `Milestone::title` is private: this is the
    /// one door in, so the gate cannot be walked past.
    #[test]
    fn renaming_goes_through_the_write_side_gate() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        for candidate in ["", "   ", "two\nlines", "carriage\rreturn"] {
            assert!(
                project.rename_milestone(0, candidate).is_err(),
                "{candidate:?} should be rejected"
            );
        }
        assert_eq!(project.milestones[0].title(), "Ch. 1 — lit review");

        project.rename_milestone(0, "  Ch. 1  ").expect("renames");
        assert_eq!(project.milestones[0].title(), "Ch. 1");
        assert!(project.milestones[0].done, "renaming must not untick it");
    }

    #[test]
    fn refuses_to_rename_a_milestone_that_is_not_there() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");
        assert!(project.rename_milestone(7, "Ch. 7").is_err());
    }

    /// `to` is a position in the *resulting* list, which is the half of this
    /// that remove-then-insert gets wrong in exactly one direction.
    #[test]
    fn moves_a_milestone_forwards_and_backwards() {
        let source =
            "---\nname: timemd\n---\n\n## Milestones\n\n- [ ] a\n- [ ] b\n- [ ] c\n- [ ] d\n";

        let mut forwards = Project::parse(slug(), source).expect("parses");
        assert_eq!(forwards.move_milestone(0, 2), Some(2));
        assert_eq!(titles(&forwards), ["b", "c", "a", "d"]);

        let mut backwards = Project::parse(slug(), source).expect("parses");
        assert_eq!(backwards.move_milestone(3, 1), Some(1));
        assert_eq!(titles(&backwards), ["a", "d", "b", "c"]);
    }

    #[test]
    fn moving_past_the_end_lands_last_and_moving_nothing_is_none() {
        let mut project = Project::parse(slug(), SAMPLE).expect("parses");

        assert_eq!(project.move_milestone(0, 99), Some(1));
        assert_eq!(
            titles(&project),
            ["Ch. 4 — first draft", "Ch. 1 — lit review"]
        );
        assert_eq!(project.move_milestone(7, 0), None);
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
