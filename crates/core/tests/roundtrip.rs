//! Property tests for the file grammar.
//!
//! The markdown tree is the public interface agents write against, so the
//! guarantees worth pinning are structural rather than example-shaped: whatever
//! goes in comes back out, and a second write changes nothing.

use chrono::{NaiveDate, NaiveTime};
use proptest::prelude::*;
use timemd_core::day::{Day, Session};
use timemd_core::document::Document;
use timemd_core::ids::ProjectSlug;
use timemd_core::project::{Milestone, Project};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
}

fn slug() -> ProjectSlug {
    ProjectSlug::new("thesis").expect("valid slug")
}

/// Notes may contain grammar characters, but not *open* with them: a note
/// starting with `[[slug]]` or `(25m)` is indistinguishable from a real project
/// link or duration group, which the dedicated tests below pin down.
fn note() -> impl Strategy<Value = String> {
    r"[a-zA-Z0-9 ()\[\],.:;'!?-]{0,40}"
        .prop_map(|text| text.trim().to_owned())
        .prop_filter("must not open with a grammar token", |text| {
            !text.starts_with("[[") && !text.starts_with('(')
        })
}

fn project() -> impl Strategy<Value = Option<ProjectSlug>> {
    prop_oneof![
        Just(None),
        prop::sample::select(vec!["timemd", "admin", "reading", "a1"])
            .prop_map(|slug| Some(ProjectSlug::new(slug).expect("valid slug"))),
    ]
}

fn session() -> impl Strategy<Value = Session> {
    (
        0_u32..24,
        0_u32..60,
        0_u32..24,
        0_u32..60,
        project(),
        note(),
    )
        .prop_map(
            |(start_hour, start_minute, end_hour, end_minute, project, note)| {
                Session::new(
                    NaiveTime::from_hms_opt(start_hour, start_minute, 0).expect("valid time"),
                    NaiveTime::from_hms_opt(end_hour, end_minute, 0).expect("valid time"),
                    project,
                    note,
                )
            },
        )
}

fn milestone() -> impl Strategy<Value = Milestone> {
    (
        any::<bool>(),
        r"[a-zA-Z0-9 .,'—-]{1,40}".prop_filter("a milestone needs a title", |title| {
            !title.trim().is_empty()
        }),
    )
        .prop_map(|(done, title)| Milestone::new(done, title).expect("valid milestone"))
}

/// Lines a hand-edited file might realistically contain, valid or not.
fn body_line() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("- not a session at all".to_owned()),
        Just("Some prose.".to_owned()),
        Just("## Retrospective".to_owned()),
        Just("- [ ] a task".to_owned()),
        r"[a-zA-Z0-9 #*_\-]{0,30}".prop_map(|text| text.trim_end().to_owned()),
    ]
}

proptest! {
    /// Rendering a parsed day and parsing it again must reproduce the same
    /// sessions — the core guarantee the API, CLI and MCP server all lean on.
    #[test]
    fn sessions_survive_a_write_and_a_read(sessions in prop::collection::vec(session(), 0..8)) {
        let mut day = Day::new(date());
        for session in &sessions {
            day.add_session(session.clone());
        }

        let reparsed = Day::parse(date(), &day.render()).expect("parses");

        let mut expected = sessions;
        expected.sort_by_key(|session| session.start);
        prop_assert_eq!(reparsed.sessions(), expected.as_slice());
        prop_assert!(reparsed.problems().is_empty());
    }

    /// Writing twice must change nothing. This is the property that makes the
    /// store safe to run against a git-tracked directory.
    #[test]
    fn writing_a_day_is_idempotent(lines in prop::collection::vec(body_line(), 0..12)) {
        let text = format!("---\ndate: 2026-08-01\n---\n\n## Sessions\n\n{}\n", lines.join("\n"));

        let once = Day::parse(date(), &text).expect("parses").render();
        let twice = Day::parse(date(), &once).expect("parses").render();

        prop_assert_eq!(once, twice);
    }

    /// Content the app does not understand comes back byte-for-byte.
    #[test]
    fn unknown_sections_survive_a_write(
        lines in prop::collection::vec(body_line(), 1..8),
        sessions in prop::collection::vec(session(), 0..4),
    ) {
        let preserved = lines.join("\n");
        let text = format!(
            "---\ndate: 2026-08-01\nagent_key: kept\n---\n\n## Sessions\n\n## Retrospective\n\n{preserved}\n"
        );

        let mut day = Day::parse(date(), &text).expect("parses");
        for session in sessions {
            day.add_session(session);
        }
        let rendered = day.render();

        prop_assert!(rendered.contains("agent_key: kept"));
        prop_assert!(
            rendered.contains(&format!("## Retrospective\n\n{preserved}\n")),
            "retrospective lost\n--- rendered ---\n{}",
            rendered
        );
    }

    /// The same guarantee as sessions, for the other owned body list.
    #[test]
    fn milestones_survive_a_write_and_a_read(
        milestones in prop::collection::vec(milestone(), 0..8),
    ) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.milestones = milestones.clone();

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");

        prop_assert!(reparsed.problems().is_empty());
        prop_assert_eq!(reparsed.milestones, milestones);
    }

    /// Reordering may only permute. A move that dropped, duplicated or mangled
    /// a milestone would be invisible in the returned index and obvious only in
    /// the file, so the property is checked against a re-read.
    #[test]
    fn moving_a_milestone_is_a_permutation(
        milestones in prop::collection::vec(milestone(), 1..8),
        from in 0_usize..8,
        to in 0_usize..8,
    ) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.milestones = milestones.clone();

        let moved = project.move_milestone(from, to);
        prop_assert_eq!(moved.is_some(), from < milestones.len());

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty());

        let mut before = milestones;
        let mut after = reparsed.milestones;
        before.sort_by(|a, b| (a.done, a.title()).cmp(&(b.done, b.title())));
        after.sort_by(|a, b| (a.done, a.title()).cmp(&(b.done, b.title())));
        prop_assert_eq!(after, before);
    }

    /// The write-side gate, as a property: any title `rename_milestone` accepts
    /// must come back byte for byte. A rename that wrote a line the reader could
    /// not read would silently demote a milestone to a preserved-verbatim
    /// problem line.
    #[test]
    fn renaming_never_writes_a_line_the_reader_cannot_read(
        milestones in prop::collection::vec(milestone(), 1..6),
        title in r"[a-zA-Z0-9 .,'—\[\]()-]{0,40}",
    ) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.milestones = milestones;

        let Ok(()) = project.rename_milestone(0, &title) else {
            // Refused, so nothing was written and there is nothing to check.
            return Ok(());
        };

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty());
        prop_assert_eq!(reparsed.milestones[0].title(), title.trim());
    }

    /// A project file the app rewrites must reach a fixed point too, with the
    /// prose and any agent-authored key still in it.
    #[test]
    fn writing_a_project_is_idempotent(lines in prop::collection::vec(body_line(), 0..12)) {
        let text = format!(
            "---\nname: Thesis\nagent_key: kept\n---\n\n# Thesis\n\nProse.\n\n## Milestones\n\n{}\n",
            lines.join("\n"),
        );

        let once = Project::parse(slug(), &text).expect("parses").render();
        let twice = Project::parse(slug(), &once).expect("parses").render();

        prop_assert!(once.contains("agent_key: kept"), "{}", once);
        prop_assert!(once.contains("Prose."), "{}", once);
        prop_assert_eq!(once, twice);
    }

    /// Any document the parser accepts must survive a render/parse cycle
    /// unchanged, whether or not the app understands its contents.
    #[test]
    fn documents_reach_a_fixed_point(lines in prop::collection::vec(body_line(), 0..12)) {
        let text = format!("---\nkey: value\n---\n\n{}\n", lines.join("\n"));

        let once = Document::parse(&text).expect("parses").render();
        let twice = Document::parse(&once).expect("parses").render();

        prop_assert_eq!(once, twice);
    }
}

/// A note that opens with a wikilink is read as a project reference. The grammar
/// cannot tell the two apart, and reading it as a link is the useful choice —
/// pinned here so the behaviour is deliberate rather than incidental.
#[test]
fn a_note_opening_with_a_wikilink_is_read_as_the_project() {
    let mut day = Day::new(date());
    day.add_session(Session::new(
        NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
        NaiveTime::from_hms_opt(9, 25, 0).expect("valid time"),
        None,
        "[[timemd]] see also",
    ));

    let reparsed = Day::parse(date(), &day.render()).expect("parses");
    let session = &reparsed.sessions()[0];

    assert_eq!(
        session.project.as_ref().map(ProjectSlug::as_str),
        Some("timemd")
    );
    assert_eq!(session.note, "see also");
}

/// A note opening with a duration-shaped group survives anything the app
/// writes, because the app emits its own duration group first and that is the
/// one the parser consumes.
#[test]
fn a_note_opening_with_a_duration_group_survives_an_app_write() {
    let mut day = Day::new(date());
    day.add_session(Session::new(
        NaiveTime::from_hms_opt(9, 0, 0).expect("valid time"),
        NaiveTime::from_hms_opt(9, 25, 0).expect("valid time"),
        None,
        "(1h) budgeted",
    ));

    let reparsed = Day::parse(date(), &day.render()).expect("parses");
    assert_eq!(reparsed.sessions()[0].note, "(1h) budgeted");
}

/// The ambiguity is only reachable by hand: written without the display
/// duration, a leading duration-shaped group is read as that duration and drops
/// out of the note. Writing the line back adds the canonical group, so this
/// costs the user the parenthesis once and never again.
#[test]
fn a_hand_written_leading_duration_group_is_consumed() {
    let text = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (1h) budgeted\n";
    let day = Day::parse(date(), text).expect("parses");

    assert_eq!(day.sessions()[0].note, "budgeted");
    assert!(day.render().contains("- 09:00-09:25 (25m) budgeted"));
}
