//! Property tests for the file grammar.
//!
//! The markdown tree is the public interface agents write against, so the
//! guarantees worth pinning are structural rather than example-shaped: whatever
//! goes in comes back out, and a second write changes nothing.

use std::collections::HashSet;
use std::ops::Range;

use chrono::{NaiveDate, NaiveTime};
use proptest::prelude::*;
use timemd_core::day::{Day, Session};
use timemd_core::document::Document;
use timemd_core::ids::ProjectSlug;
use timemd_core::ids::TodoId;
use timemd_core::minutes::Minutes;
use timemd_core::project::{Milestone, Project};
use timemd_core::schedule::DayBlock;
use timemd_core::todo::{Priority, Stamp, Todo, TodoStatus, Todos};

/// The signifier characters, repeated here rather than exported from core: a
/// test that reached for the real list could not catch the list itself losing
/// one, which is the failure that would silently swallow a field.
const SIGNIFIERS: [char; 15] = [
    '🔺', '⏫', '🔼', '🔽', '⏬', '➕', '🛫', '⏳', '📅', '✅', '❌', '🔁', '🆔', '⛔', '🏁',
];

/// Splits a rendered todo line into its description half and its `(signifier,
/// value)` fields, so a property can put the fields back in another order.
fn take_apart(line: &str) -> (&str, Vec<&str>) {
    let Some(start) = line.find(SIGNIFIERS) else {
        return (line, Vec::new());
    };
    let (head, mut rest) = line.split_at(start);

    let mut fields = Vec::new();
    while !rest.is_empty() {
        let width = rest.chars().next().map_or(0, char::len_utf8);
        let end = rest[width..]
            .find(SIGNIFIERS)
            .map_or(rest.len(), |offset| offset + width);
        fields.push(rest[..end].trim_end());
        rest = &rest[end..];
    }
    (head.trim_end(), fields)
}

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

/// A block's title sits where a session's note does, so it carries the same
/// rule: grammar characters are fine inside it, but opening with one makes it
/// indistinguishable from a project link.
fn day_block() -> impl Strategy<Value = DayBlock> {
    (
        0_u32..24,
        0_u32..60,
        0_u32..24,
        0_u32..60,
        project(),
        r"[a-zA-Z0-9 ,.:;'?-]{1,40}"
            .prop_filter("a block needs a title", |title| !title.trim().is_empty()),
        prop::option::of((1_u32..120).prop_map(Minutes::new)),
    )
        .prop_map(
            |(start_hour, start_minute, end_hour, end_minute, project, title, remind_before)| {
                DayBlock {
                    start: NaiveTime::from_hms_opt(start_hour, start_minute, 0)
                        .expect("valid time"),
                    end: NaiveTime::from_hms_opt(end_hour, end_minute, 0).expect("valid time"),
                    project,
                    title: title.trim().to_owned(),
                    remind_before,
                }
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

/// Milestone lists a writer could actually produce: no title carried twice.
///
/// De-duplicated rather than filtered. Proptest shrinks towards short titles
/// from a small alphabet, so a filter over the whole vector would reject most
/// draws exactly when the list gets long — quietly stopping the coverage of the
/// long lists these properties were written for.
///
/// It is also the more faithful shape. `Project::set_milestones` refuses a
/// repeated title, so a list carrying one is not something any surface can
/// write; it is a file somebody hand-edited. That case is a *read* guarantee and
/// has its own tests — `refuses_a_title_two_milestones_share` in core, and
/// `a_hand_written_duplicate_title_lists_but_is_not_addressable` in MCP. What
/// this file pins is that whatever the writer accepts comes back out.
fn milestones(count: Range<usize>) -> impl Strategy<Value = Vec<Milestone>> {
    prop::collection::vec(milestone(), count).prop_map(|drawn| {
        let mut seen = HashSet::new();
        drawn
            .into_iter()
            .filter(|milestone| seen.insert(milestone.title().to_owned()))
            .collect()
    })
}

/// Lines a hand-edited file might realistically contain, valid or not.
fn body_line() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("- not a session at all".to_owned()),
        Just("Some prose.".to_owned()),
        Just("## Retrospective".to_owned()),
        Just("- [ ] a task".to_owned()),
        Just("- [ ] a task 📅 2026-01-01".to_owned()),
        r"[a-zA-Z0-9 #*_\-]{0,30}".prop_map(|text| text.trim_end().to_owned()),
    ]
}

fn stamp() -> impl Strategy<Value = Stamp> {
    (
        2020_i32..2030,
        1_u32..13,
        1_u32..29,
        prop::option::of((0_u32..24, 0_u32..60)),
    )
        .prop_map(|(year, month, day, time)| {
            let date = NaiveDate::from_ymd_opt(year, month, day).expect("valid date");
            match time {
                None => Stamp::on(date),
                Some((hours, minutes)) => Stamp::at(
                    date,
                    NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time"),
                ),
            }
        })
}

fn todo_id() -> impl Strategy<Value = TodoId> {
    r"[a-zA-Z0-9_-]{1,8}".prop_map(|raw| TodoId::new(raw).expect("valid id"))
}

/// A todo carrying an arbitrary subset of the signifiers.
///
/// The description excludes `#` on purpose: it is legal in one, but a trailing
/// `#` is the one character `split_whitespace` would hand back as an empty tag,
/// and tags have their own example test. Everything a signifier would claim is
/// refused by `Todo::new`, so the strategy never has to filter for it.
fn todo() -> impl Strategy<Value = Todo> {
    (
        prop::sample::select(vec![
            TodoStatus::Open,
            TodoStatus::Done,
            TodoStatus::Cancelled,
            TodoStatus::Other('/'),
        ]),
        r"[a-zA-Z0-9 .,'()-]{1,40}".prop_filter("a todo needs a description", |text| {
            !text.trim().is_empty() && !text.trim().starts_with("[[")
        }),
        project(),
        prop::sample::select(vec![
            Priority::Highest,
            Priority::High,
            Priority::Medium,
            Priority::Normal,
            Priority::Low,
            Priority::Lowest,
        ]),
        prop::option::of(r"every [a-z0-9 ]{1,20}[a-z0-9]"),
        prop::collection::vec(todo_id(), 0..3),
        prop::option::of(stamp()),
        prop::option::of(stamp()),
        prop::option::of(stamp()),
        prop::option::of(stamp()),
    )
        .prop_map(
            |(
                status,
                description,
                project,
                priority,
                recurrence,
                depends_on,
                created,
                scheduled,
                due,
                done,
            )| {
                let mut todo = Todo::new(status, description).expect("valid todo");
                todo.project = project;
                todo.priority = priority;
                todo.recurrence = recurrence;
                todo.depends_on = depends_on;
                todo.created = created;
                todo.scheduled = scheduled;
                todo.due = due;
                todo.done = done;
                todo
            },
        )
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

    /// The same guarantee as sessions, for the day's other owned list. `##
    /// Schedule` had no property test at all, which is why editing a one-off
    /// block was the operation missing from every surface at once.
    #[test]
    fn blocks_survive_a_write_and_a_read(blocks in prop::collection::vec(day_block(), 0..8)) {
        let mut day = Day::new(date());
        for block in &blocks {
            day.add_block(block.clone());
        }

        let reparsed = Day::parse(date(), &day.render()).expect("parses");

        let mut expected = blocks;
        expected.sort_by_key(|block| block.start);
        prop_assert_eq!(reparsed.schedule(), expected.as_slice());
        prop_assert!(reparsed.problems().is_empty());
    }

    /// Replacing a block must leave the day readable and correctly ordered,
    /// whatever the new start time does to the list.
    #[test]
    fn replacing_a_block_keeps_the_day_ordered(
        blocks in prop::collection::vec(day_block(), 1..6),
        index in 0_usize..6,
        replacement in day_block(),
    ) {
        let mut day = Day::new(date());
        for block in &blocks {
            day.add_block(block.clone());
        }

        let replaced = day.replace_block(index, replacement.clone());
        prop_assert_eq!(replaced.is_some(), index < blocks.len());

        let reparsed = Day::parse(date(), &day.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty());
        prop_assert!(reparsed.schedule().is_sorted_by_key(|block| block.start));
        prop_assert_eq!(reparsed.schedule().len(), blocks.len());
        if replaced.is_some() {
            prop_assert!(reparsed.schedule().contains(&replacement));
        }
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
    fn milestones_survive_a_write_and_a_read(milestones in milestones(0..8)) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.set_milestones(milestones.clone()).expect("distinct titles");

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");

        prop_assert!(reparsed.problems().is_empty());
        prop_assert_eq!(reparsed.milestones(), milestones.as_slice());
    }

    /// Reordering may only permute. A move that dropped, duplicated or mangled
    /// a milestone would be invisible in the returned index and obvious only in
    /// the file, so the property is checked against a re-read.
    #[test]
    fn moving_a_milestone_is_a_permutation(
        milestones in milestones(1..8),
        from in 0_usize..8,
        to in 0_usize..8,
    ) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.set_milestones(milestones.clone()).expect("distinct titles");

        let moved = project.move_milestone(from, to);
        prop_assert_eq!(moved.is_some(), from < milestones.len());

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty());

        let mut before = milestones;
        let mut after = reparsed.milestones().to_vec();
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
        milestones in milestones(1..6),
        title in r"[a-zA-Z0-9 .,'—\[\]()-]{0,40}",
    ) {
        let mut project = Project::new(slug(), "Thesis", date());
        project.set_milestones(milestones).expect("distinct titles");

        let Ok(()) = project.rename_milestone(0, &title) else {
            // Refused, so nothing was written and there is nothing to check.
            return Ok(());
        };

        let reparsed = Project::parse(slug(), &project.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty());
        prop_assert_eq!(reparsed.milestones()[0].title(), title.trim());
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

    /// The same guarantee as sessions and milestones, for the widest line in
    /// the tree: every signifier a todo can carry must come back off the disk.
    #[test]
    fn todos_survive_a_write_and_a_read(drawn in prop::collection::vec(todo(), 0..8)) {
        let mut todos = Todos::default();
        for todo in drawn {
            todos.add(todo).expect("a fresh id");
        }

        let reparsed = Todos::parse(&todos.render()).expect("parses");

        prop_assert!(reparsed.problems().is_empty());
        prop_assert_eq!(reparsed.todos(), todos.todos());
    }

    /// The property the canonical write order exists for. Reads take the fields
    /// in any order, so without one order on the way out, two clients would
    /// rewrite each other's lines forever.
    #[test]
    fn signifiers_read_the_same_in_any_order(drawn in todo(), rotate in 0_usize..12) {
        let mut todos = Todos::default();
        todos.add(drawn).expect("a fresh id");
        let canonical = todos.render();

        let jumbled: String = canonical
            .lines()
            .map(|line| {
                let (head, mut fields) = take_apart(line);
                if fields.is_empty() {
                    return line.to_owned();
                }
                let by = rotate % fields.len();
                fields.rotate_left(by);
                format!("{head} {}", fields.join(" "))
            })
            .collect::<Vec<_>>()
            .join("\n");

        let reparsed = Todos::parse(&jumbled).expect("parses");
        prop_assert!(reparsed.problems().is_empty(), "{}", jumbled);
        prop_assert_eq!(reparsed.render(), canonical);
    }

    /// A todo file the app rewrites must reach a fixed point, with the prose,
    /// any agent-authored key and any line it could not read still in it.
    #[test]
    fn writing_todos_is_idempotent(lines in prop::collection::vec(body_line(), 0..12)) {
        let text = format!(
            "---\nagent_key: kept\n---\n\n# Todos\n\nProse.\n\n## Todos\n\n{}\n",
            lines.join("\n"),
        );

        let once = Todos::parse(&text).expect("parses").render();
        let twice = Todos::parse(&once).expect("parses").render();

        prop_assert!(once.contains("agent_key: kept"), "{}", once);
        prop_assert!(once.contains("Prose."), "{}", once);
        prop_assert_eq!(once, twice);
    }

    /// The write-side gate, as a property: any description `Todo::new` accepts
    /// must come back byte for byte. One it accepted but could not read back
    /// would silently demote a todo to a preserved-verbatim problem line.
    #[test]
    fn a_todo_write_never_produces_a_line_the_reader_cannot_read(
        description in r"[a-zA-Z0-9 .,'\[\]()#!?📅⏫-]{0,40}",
    ) {
        let Ok(todo) = Todo::new(TodoStatus::Open, &description) else {
            // Refused, so nothing was written and there is nothing to check.
            return Ok(());
        };

        let mut todos = Todos::default();
        todos.add(todo).expect("a fresh id");

        let reparsed = Todos::parse(&todos.render()).expect("parses");
        prop_assert!(reparsed.problems().is_empty(), "{}", todos.render());
        prop_assert_eq!(reparsed.todos()[0].description(), description.trim());
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
