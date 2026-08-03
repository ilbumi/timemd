//! Day files: `data/days/YYYY/YYYY-MM-DD.md`.
//!
//! One file per day holds what was tracked (`## Sessions`), later joined by what
//! was planned (`## Schedule`, `## Skipped`), with `## Notes` and anything an
//! agent adds carried straight through.

use chrono::{NaiveDate, NaiveTime};
use serde::Serialize;

use crate::document::Document;
use crate::error::{ParseError, ParseErrorKind};
use crate::grammar;
use crate::ids::{BlockId, ProjectSlug};
use crate::minutes::Minutes;
use crate::schedule::DayBlock;

const SECTION_SCHEDULE: &str = "Schedule";
const SECTION_SKIPPED: &str = "Skipped";
const SECTION_SESSIONS: &str = "Sessions";

/// Sections that must precede each owned section when it is created, so a file
/// built up over time keeps the same order as one written in one go.
const BEFORE_SKIPPED: &[&str] = &[SECTION_SCHEDULE];
const BEFORE_SESSIONS: &[&str] = &[SECTION_SKIPPED, SECTION_SCHEDULE];

/// A tracked interval.
///
/// Pomodoro focus blocks and hand-entered meetings are the same thing — there is
/// one concept here, not two. Breaks are timer state and never become sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub project: Option<ProjectSlug>,
    pub note: String,
}

impl Session {
    pub fn new(
        start: NaiveTime,
        end: NaiveTime,
        project: Option<ProjectSlug>,
        note: impl Into<String>,
    ) -> Self {
        Self {
            start,
            end,
            project,
            note: note.into(),
        }
    }

    /// Wall-clock length, treating `end < start` as crossing midnight.
    pub fn duration(&self) -> Minutes {
        grammar::span(self.start, self.end)
    }

    fn parse(content: &str) -> Result<Self, ParseErrorKind> {
        let ((start, end), rest) = grammar::time_range(content)?;
        let rest = grammar::skip_duration_group(rest);
        let (project, note) = grammar::wikilink(rest);
        Ok(Self::new(start, end, project, note.trim()))
    }

    fn render(&self) -> String {
        let line = format!(
            "- {}-{} ({}) {}{}",
            grammar::format_time(self.start),
            grammar::format_time(self.end),
            self.duration(),
            grammar::format_wikilink(self.project.as_ref()),
            self.note,
        );
        line.trim_end().to_owned()
    }
}

/// A day's file, with everything the app does not understand held intact.
#[derive(Debug, Clone)]
pub struct Day {
    date: NaiveDate,
    /// One-off blocks planned for this day only.
    schedule: Vec<DayBlock>,
    /// Repeating blocks suppressed for this day.
    skipped: Vec<BlockId>,
    sessions: Vec<Session>,
    /// Lines that failed to parse, per owned section. Preserved verbatim and
    /// re-emitted at the end of their section, so a typo costs the user one
    /// misplaced line rather than the whole day.
    unparsed_sessions: Vec<String>,
    unparsed_schedule: Vec<String>,
    unparsed_skipped: Vec<String>,
    problems: Vec<ParseError>,
    document: Document,
}

impl Day {
    /// A fresh day file with the standard title and no sessions.
    pub fn new(date: NaiveDate) -> Self {
        let mut document = Document::new();
        document.set_front_key("date", &date.to_string());
        document.set_preamble(vec![String::new(), format!("# {date}"), String::new()]);
        Self {
            date,
            schedule: Vec::new(),
            skipped: Vec::new(),
            sessions: Vec::new(),
            unparsed_sessions: Vec::new(),
            unparsed_schedule: Vec::new(),
            unparsed_skipped: Vec::new(),
            problems: Vec::new(),
            document,
        }
    }

    /// Reads a day file. Only malformed YAML frontmatter fails.
    pub fn parse(date: NaiveDate, text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let mut problems = Vec::new();

        let (mut sessions, unparsed_sessions) =
            document.parse_list_section(SECTION_SESSIONS, Session::parse, &mut problems);
        sessions.sort_by_key(|session| session.start);

        let (mut schedule, unparsed_schedule) =
            document.parse_list_section(SECTION_SCHEDULE, DayBlock::parse, &mut problems);
        schedule.sort_by_key(|block| block.start);

        let (skipped, unparsed_skipped) =
            document.parse_list_section(SECTION_SKIPPED, grammar::backtick_id, &mut problems);

        Ok(Self {
            date,
            schedule,
            skipped,
            sessions,
            unparsed_sessions,
            unparsed_schedule,
            unparsed_skipped,
            problems,
            document,
        })
    }

    /// Writes the owned sections back and reassembles the file text.
    pub fn render(&self) -> String {
        let mut document = self.document.clone();

        document.write_list_section(
            SECTION_SCHEDULE,
            &self.schedule,
            &self.unparsed_schedule,
            &[],
            DayBlock::render,
        );
        document.write_list_section(
            SECTION_SKIPPED,
            &self.skipped,
            &self.unparsed_skipped,
            BEFORE_SKIPPED,
            |id| format!("- `{id}`"),
        );
        document.write_list_section(
            SECTION_SESSIONS,
            &self.sessions,
            &self.unparsed_sessions,
            BEFORE_SESSIONS,
            Session::render,
        );

        document.render()
    }

    pub fn date(&self) -> NaiveDate {
        self.date
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// Lines the app could not parse, with the reason. Surfaced to the UI so a
    /// broken file is visible rather than silently half-loaded.
    pub fn problems(&self) -> &[ParseError] {
        &self.problems
    }

    /// Total tracked time for the day.
    pub fn total(&self) -> Minutes {
        self.sessions.iter().map(Session::duration).sum()
    }

    /// Inserts a session, keeping the list ordered by start time.
    pub fn add_session(&mut self, session: Session) {
        let position = self
            .sessions
            .partition_point(|existing| existing.start <= session.start);
        self.sessions.insert(position, session);
    }

    pub fn replace_session(&mut self, index: usize, session: Session) -> Option<Session> {
        let previous = self.sessions.get(index)?.clone();
        self.sessions[index] = session;
        self.sessions.sort_by_key(|session| session.start);
        Some(previous)
    }

    pub fn remove_session(&mut self, index: usize) -> Option<Session> {
        (index < self.sessions.len()).then(|| self.sessions.remove(index))
    }

    pub fn schedule(&self) -> &[DayBlock] {
        &self.schedule
    }

    pub fn skipped(&self) -> &[BlockId] {
        &self.skipped
    }

    /// Adds a one-off block, keeping the list ordered by start time.
    pub fn add_block(&mut self, block: DayBlock) {
        let position = self
            .schedule
            .partition_point(|existing| existing.start <= block.start);
        self.schedule.insert(position, block);
    }

    /// Replaces the one-off block at `index`, keeping the list ordered by start.
    ///
    /// Mirrors [`Day::replace_session`], including its consequence: the list is
    /// re-sorted, so an index held across the call may afterwards name a
    /// different block. Callers re-read rather than reusing it.
    pub fn replace_block(&mut self, index: usize, block: DayBlock) -> Option<DayBlock> {
        let previous = self.schedule.get(index)?.clone();
        self.schedule[index] = block;
        self.schedule.sort_by_key(|block| block.start);
        Some(previous)
    }

    pub fn remove_block(&mut self, index: usize) -> Option<DayBlock> {
        (index < self.schedule.len()).then(|| self.schedule.remove(index))
    }

    /// Suppresses a repeating block for this day. Idempotent.
    pub fn skip(&mut self, id: BlockId) {
        if !self.skipped.contains(&id) {
            self.skipped.push(id);
        }
    }

    /// Restores a suppressed block. Returns whether it had been skipped.
    pub fn unskip(&mut self, id: &BlockId) -> bool {
        let before = self.skipped.len();
        self.skipped.retain(|candidate| candidate != id);
        self.skipped.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
    }

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    fn slug(text: &str) -> Option<ProjectSlug> {
        Some(ProjectSlug::new(text).expect("valid slug"))
    }

    const SAMPLE: &str = "---\ndate: 2026-08-01\n---\n\n# 2026-08-01\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] file store layer\n- 10:30-10:45 (15m) email\n\n## Notes\n\nFree-form prose.\n";

    #[test]
    fn parses_sessions_with_and_without_a_project() {
        let day = Day::parse(date(), SAMPLE).expect("parses");
        assert_eq!(day.sessions().len(), 2);

        let first = &day.sessions()[0];
        assert_eq!((first.start, first.end), (at(9, 0), at(9, 25)));
        assert_eq!(first.project, slug("timemd"));
        assert_eq!(first.note, "file store layer");

        let second = &day.sessions()[1];
        assert_eq!(second.project, None);
        assert_eq!(second.note, "email");
    }

    #[test]
    fn round_trips_a_canonical_day_byte_for_byte() {
        let day = Day::parse(date(), SAMPLE).expect("parses");
        assert_eq!(day.render(), SAMPLE);
    }

    #[test]
    fn recomputes_the_display_duration_from_the_times() {
        let text =
            "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (99m) [[timemd]] wrong\n";
        let day = Day::parse(date(), text).expect("parses");
        assert_eq!(day.sessions()[0].duration(), Minutes::new(25));
        assert!(
            day.render()
                .contains("- 09:00-09:25 (25m) [[timemd]] wrong")
        );
    }

    #[test]
    fn totals_the_day() {
        let day = Day::parse(date(), SAMPLE).expect("parses");
        assert_eq!(day.total(), Minutes::new(40));
    }

    #[test]
    fn sorts_sessions_by_start_time_on_read() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 14:00-14:30 (30m) later\n- 09:00-09:25 (25m) earlier\n";
        let day = Day::parse(date(), text).expect("parses");
        assert_eq!(day.sessions()[0].note, "earlier");
        assert_eq!(day.sessions()[1].note, "later");
    }

    #[test]
    fn keeps_unparseable_lines_and_reports_them() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] good\n- this line is nonsense\n- 9:00-10:00 bad time format\n";
        let day = Day::parse(date(), text).expect("parses");

        assert_eq!(day.sessions().len(), 1);
        assert_eq!(day.problems().len(), 2);

        let rendered = day.render();
        assert!(rendered.contains("- this line is nonsense"), "{rendered}");
        assert!(
            rendered.contains("- 9:00-10:00 bad time format"),
            "{rendered}"
        );
        assert!(
            rendered.contains("- 09:00-09:25 (25m) [[timemd]] good"),
            "{rendered}"
        );
    }

    #[test]
    fn preserves_notes_and_agent_authored_sections() {
        let text = "---\ndate: 2026-08-01\nmood: focused\n---\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] work\n\n## Retrospective\n\nWrote by an agent.\n";
        let mut day = Day::parse(date(), text).expect("parses");
        day.add_session(Session::new(at(11, 0), at(11, 25), slug("timemd"), "more"));

        let rendered = day.render();
        assert!(rendered.contains("mood: focused"), "{rendered}");
        assert!(
            rendered.contains("## Retrospective\n\nWrote by an agent.\n"),
            "{rendered}"
        );
        assert!(
            rendered.contains("- 11:00-11:25 (25m) [[timemd]] more"),
            "{rendered}"
        );
    }

    #[test]
    fn a_new_day_renders_the_template() {
        let day = Day::new(date());
        assert_eq!(day.render(), "---\ndate: 2026-08-01\n---\n\n# 2026-08-01\n");
        assert_eq!(day.total(), Minutes::new(0));
    }

    #[test]
    fn adding_to_a_new_day_creates_the_sessions_section() {
        let mut day = Day::new(date());
        day.add_session(Session::new(at(9, 0), at(9, 25), slug("timemd"), "first"));
        assert_eq!(
            day.render(),
            "---\ndate: 2026-08-01\n---\n\n# 2026-08-01\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] first\n"
        );
    }

    #[test]
    fn sessions_land_after_schedule_sections() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Schedule\n\n- 16:00-17:00 talk\n";
        let mut day = Day::parse(date(), text).expect("parses");
        day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));

        let rendered = day.render();
        let schedule = rendered.find("## Schedule").expect("present");
        let sessions = rendered.find("## Sessions").expect("present");
        assert!(schedule < sessions, "{rendered}");
    }

    #[test]
    fn add_keeps_the_list_sorted() {
        let mut day = Day::new(date());
        day.add_session(Session::new(at(14, 0), at(14, 30), None, "later"));
        day.add_session(Session::new(at(9, 0), at(9, 25), None, "earlier"));
        assert_eq!(day.sessions()[0].note, "earlier");
    }

    #[test]
    fn removes_and_replaces_by_index() {
        let mut day = Day::parse(date(), SAMPLE).expect("parses");

        let replaced = day.replace_session(1, Session::new(at(11, 0), at(11, 30), None, "swapped"));
        assert_eq!(
            replaced.map(|session| session.note),
            Some("email".to_owned())
        );
        assert_eq!(day.sessions()[1].note, "swapped");

        let removed = day.remove_session(0);
        assert_eq!(
            removed.map(|session| session.note),
            Some("file store layer".to_owned())
        );
        assert_eq!(day.sessions().len(), 1);

        assert!(day.remove_session(9).is_none());
        assert!(
            day.replace_session(9, Session::new(at(1, 0), at(2, 0), None, ""))
                .is_none()
        );
    }

    #[test]
    fn a_session_crossing_midnight_stays_on_one_line() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 23:50-00:20 (30m) [[timemd]] late night\n";
        let day = Day::parse(date(), text).expect("parses");
        assert_eq!(day.sessions()[0].duration(), Minutes::new(30));
        assert_eq!(day.render(), text);
    }

    #[test]
    fn a_note_beginning_with_parentheses_survives() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (draft) rewrite the intro\n";
        let day = Day::parse(date(), text).expect("parses");
        assert_eq!(day.sessions()[0].note, "(draft) rewrite the intro");
        assert!(
            day.render()
                .contains("- 09:00-09:25 (25m) (draft) rewrite the intro")
        );
    }

    #[test]
    fn removing_every_session_removes_the_section() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (25m) work\n\n## Notes\n\nkeep me\n";
        let mut day = Day::parse(date(), text).expect("parses");
        day.remove_session(0);

        let rendered = day.render();
        assert!(!rendered.contains("## Sessions"), "{rendered}");
        assert!(rendered.contains("keep me"), "{rendered}");
    }

    #[test]
    fn parses_schedule_and_skipped_sections() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Schedule\n\n- 16:00-17:00 [[reading]] Paper club !15m\n\n## Skipped\n\n- `deep-work`\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] work\n";
        let day = Day::parse(date(), text).expect("parses");

        assert_eq!(day.schedule().len(), 1);
        assert_eq!(day.schedule()[0].title, "Paper club");
        assert_eq!(
            day.skipped(),
            &[BlockId::new("deep-work").expect("valid id")]
        );
        assert_eq!(day.sessions().len(), 1);
        assert_eq!(day.render(), text);
    }

    #[test]
    fn owned_sections_keep_their_canonical_order_when_added_late() {
        let mut day = Day::new(date());
        day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));
        day.skip(BlockId::new("deep-work").expect("valid id"));
        day.add_block(DayBlock::parse("16:00-17:00 Talk").expect("parses"));

        let rendered = day.render();
        let schedule = rendered.find("## Schedule").expect("present");
        let skipped = rendered.find("## Skipped").expect("present");
        let sessions = rendered.find("## Sessions").expect("present");
        assert!(schedule < skipped && skipped < sessions, "{rendered}");
    }

    #[test]
    fn blocks_are_kept_in_start_order() {
        let mut day = Day::new(date());
        day.add_block(DayBlock::parse("16:00-17:00 Later").expect("parses"));
        day.add_block(DayBlock::parse("09:00-10:00 Earlier").expect("parses"));

        assert_eq!(day.schedule()[0].title, "Earlier");
        assert_eq!(
            day.remove_block(0).map(|block| block.title),
            Some("Earlier".to_owned())
        );
        assert!(day.remove_block(5).is_none());
    }

    /// The mirror of `removes_and_replaces_by_index`, for the other owned list.
    #[test]
    fn replaces_a_block_by_index_and_re_sorts() {
        let mut day = Day::new(date());
        day.add_block(DayBlock::parse("09:00-10:00 Earlier").expect("parses"));
        day.add_block(DayBlock::parse("16:00-17:00 Later").expect("parses"));

        let replaced = day.replace_block(0, DayBlock::parse("18:00-19:00 Moved").expect("parses"));
        assert_eq!(
            replaced.map(|block| block.title),
            Some("Earlier".to_owned())
        );

        // Moving it past the other one re-sorts the day, so the index the
        // caller just used now names a different block.
        assert_eq!(day.schedule()[0].title, "Later");
        assert_eq!(day.schedule()[1].title, "Moved");

        assert!(
            day.replace_block(9, DayBlock::parse("08:00-09:00 Nowhere").expect("parses"))
                .is_none()
        );
    }

    #[test]
    fn skipping_is_idempotent_and_reversible() {
        let mut day = Day::new(date());
        let id = BlockId::new("deep-work").expect("valid id");

        day.skip(id.clone());
        day.skip(id.clone());
        assert_eq!(day.skipped().len(), 1);

        assert!(day.unskip(&id));
        assert!(!day.unskip(&id));
        assert!(!day.render().contains("## Skipped"));
    }

    /// Guarantee 1 in docs/format.md, for the section that used to break it:
    /// `## Skipped` reported a malformed line and then erased it on the next
    /// write, because its parse loop was hand-rolled separately from the others.
    #[test]
    fn an_unreadable_skip_entry_is_reported_and_kept() {
        let text = "---\ndate: 2026-08-01\n---\n\n## Skipped\n\n- `deep-work`\n- not-backticked\n";
        let mut day = Day::parse(date(), text).expect("parses");

        assert_eq!(day.skipped().len(), 1);
        assert_eq!(day.problems().len(), 1);

        day.skip(BlockId::new("review").expect("valid id"));
        let rendered = day.render();

        assert!(rendered.contains("- not-backticked"), "{rendered}");
        assert!(rendered.contains("- `review`"), "{rendered}");
    }

    #[test]
    fn an_unreadable_schedule_line_is_preserved() {
        let text =
            "---\ndate: 2026-08-01\n---\n\n## Schedule\n\n- 16:00-17:00 Fine\n- nonsense here\n";
        let day = Day::parse(date(), text).expect("parses");

        assert_eq!(day.schedule().len(), 1);
        assert_eq!(day.problems().len(), 1);
        assert!(day.render().contains("- nonsense here"));
    }

    /// Durations are wall-clock, so the two clock changes a year distort those
    /// days. This is a documented consequence of keeping offsets out of the
    /// files, not an accident — pinned here so it cannot change silently.
    #[test]
    fn documents_wall_clock_arithmetic_across_a_clock_change() {
        let spring_forward = Session::new(at(1, 50), at(3, 15), None, "across the jump");
        assert_eq!(spring_forward.duration(), Minutes::new(85));
    }
}
