//! Planned time blocks.
//!
//! Repeating blocks live once in `data/schedule/recurring.md`; a day file adds
//! its own one-offs under `## Schedule` and suppresses a repeat under
//! `## Skipped`. Expanding those three sources over a date range is what the
//! schedule screen and the reminder scheduler both read.

use std::fmt;
use std::str::FromStr;

use chrono::{Datelike, NaiveDate, NaiveTime, Weekday};

use crate::document::Document;
use crate::error::{ParseError, ParseErrorKind};
use crate::grammar;
use crate::ids::{BlockId, ProjectSlug};
use crate::minutes::Minutes;

const SECTION_BLOCKS: &str = "Blocks";
const DAY_NAMES: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
/// Runs at least this long render as a range rather than a list.
const RANGE_THRESHOLD: usize = 3;

/// Which weekdays a block repeats on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DaySet(u8);

impl DaySet {
    pub const ALL: Self = Self(0b0111_1111);

    pub fn from_weekdays(days: impl IntoIterator<Item = Weekday>) -> Self {
        Self(
            days.into_iter()
                .fold(0, |bits, day| bits | 1 << day.num_days_from_monday()),
        )
    }

    pub fn contains(self, day: Weekday) -> bool {
        self.0 & (1 << day.num_days_from_monday()) != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The selected days as their canonical names, Monday first.
    ///
    /// The spelling is part of the file grammar, so it belongs here rather than
    /// being reinvented by whatever wants to show a day picker. Feeding the
    /// names back through `parse` round-trips, which is how a caller edits a set
    /// without knowing about ranges or `daily`.
    pub fn names(self) -> Vec<&'static str> {
        self.indices().map(|index| DAY_NAMES[index]).collect()
    }

    fn indices(self) -> impl Iterator<Item = usize> {
        (0..7).filter(move |index| self.0 & (1 << index) != 0)
    }
}

fn weekday_index(name: &str) -> Option<usize> {
    DAY_NAMES.iter().position(|candidate| *candidate == name)
}

impl FromStr for DaySet {
    type Err = ParseErrorKind;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let invalid = || ParseErrorKind::InvalidWeekday {
            found: text.to_owned(),
        };
        let text = text.trim();
        if text.eq_ignore_ascii_case("daily") {
            return Ok(Self::ALL);
        }

        let mut bits = 0_u8;
        for part in text.split(',') {
            let part = part.trim().to_ascii_lowercase();
            match part.split_once('-') {
                Some((from, to)) => {
                    let (from, to) = (
                        weekday_index(from).ok_or_else(invalid)?,
                        weekday_index(to).ok_or_else(invalid)?,
                    );
                    if from > to {
                        return Err(invalid());
                    }
                    for index in from..=to {
                        bits |= 1 << index;
                    }
                }
                None => bits |= 1 << weekday_index(&part).ok_or_else(invalid)?,
            }
        }

        if bits == 0 {
            return Err(invalid());
        }
        Ok(Self(bits))
    }
}

/// Canonical form: `daily`, `mon-fri`, `mon,wed,fri`, `mon-fri,sun`.
impl fmt::Display for DaySet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::ALL {
            return formatter.write_str("daily");
        }

        let selected: Vec<usize> = self.indices().collect();
        let mut parts: Vec<String> = Vec::new();
        let mut run: Vec<usize> = Vec::new();

        let flush = |run: &mut Vec<usize>, parts: &mut Vec<String>| {
            if run.len() >= RANGE_THRESHOLD {
                parts.push(format!(
                    "{}-{}",
                    DAY_NAMES[run[0]],
                    DAY_NAMES[run[run.len() - 1]]
                ));
            } else {
                parts.extend(run.iter().map(|index| DAY_NAMES[*index].to_owned()));
            }
            run.clear();
        };

        for index in selected {
            if run.last().is_some_and(|previous| index != previous + 1) {
                flush(&mut run, &mut parts);
            }
            run.push(index);
        }
        flush(&mut run, &mut parts);

        formatter.write_str(&parts.join(","))
    }
}

/// A repeating block, defined once in `recurring.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurringBlock {
    pub id: BlockId,
    pub days: DaySet,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub project: Option<ProjectSlug>,
    pub title: String,
    pub remind_before: Option<Minutes>,
}

impl RecurringBlock {
    fn parse(content: &str) -> Result<Self, ParseErrorKind> {
        let (id, rest) = grammar::take_backtick_id(content)?;
        let (days, rest) = grammar::split_token(rest);
        let days: DaySet = days.parse()?;
        let ((start, end), rest) = grammar::time_range(rest)?;
        let (project, rest) = grammar::wikilink(rest);
        let (remind_before, title) = grammar::reminder_suffix(rest);

        Ok(Self {
            id,
            days,
            start,
            end,
            project,
            title: title.trim().to_owned(),
            remind_before,
        })
    }

    fn render(&self) -> String {
        format!(
            "- `{}` {} {}-{} {}{}{}",
            self.id,
            self.days,
            grammar::format_time(self.start),
            grammar::format_time(self.end),
            grammar::format_wikilink(self.project.as_ref()),
            self.title,
            self.remind_before
                .map_or_else(String::new, |lead| format!(" !{lead}")),
        )
        .replace("  ", " ")
        .trim_end()
        .to_owned()
    }
}

/// A block written directly into a day file, happening only that once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayBlock {
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub project: Option<ProjectSlug>,
    pub title: String,
    pub remind_before: Option<Minutes>,
}

impl DayBlock {
    pub fn parse(content: &str) -> Result<Self, ParseErrorKind> {
        let ((start, end), rest) = grammar::time_range(content)?;
        let (project, rest) = grammar::wikilink(rest);
        let (remind_before, title) = grammar::reminder_suffix(rest);

        Ok(Self {
            start,
            end,
            project,
            title: title.trim().to_owned(),
            remind_before,
        })
    }

    pub fn render(&self) -> String {
        format!(
            "- {}-{} {}{}{}",
            grammar::format_time(self.start),
            grammar::format_time(self.end),
            grammar::format_wikilink(self.project.as_ref()),
            self.title,
            self.remind_before
                .map_or_else(String::new, |lead| format!(" !{lead}")),
        )
        .trim_end()
        .to_owned()
    }
}

/// One concrete instance of a block on a date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub project: Option<ProjectSlug>,
    pub title: String,
    pub remind_before: Option<Minutes>,
    /// The repeating block this came from, or `None` for a one-off.
    pub block: Option<BlockId>,
    /// Position among that day's one-offs — the handle `Day::replace_block` and
    /// `Day::remove_block` take — or `None` for a repeat.
    ///
    /// Carried here rather than counted by each caller because it is a position
    /// among *one day's* one-offs, and a caller holding a merged range has
    /// nothing left to count it from. Three surfaces wrote that pass out; the
    /// one handed a range counted straight through the day boundary.
    pub one_off_index: Option<usize>,
}

impl Occurrence {
    pub fn duration(&self) -> Minutes {
        grammar::span(self.start, self.end)
    }
}

/// `data/schedule/recurring.md`.
#[derive(Debug, Clone)]
pub struct Recurring {
    blocks: Vec<RecurringBlock>,
    unparsed: Vec<String>,
    problems: Vec<ParseError>,
    document: Document,
}

impl Recurring {
    pub fn parse(text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let mut problems = Vec::new();
        let (blocks, unparsed) =
            document.parse_list_section(SECTION_BLOCKS, RecurringBlock::parse, &mut problems);

        Ok(Self {
            blocks,
            unparsed,
            problems,
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();
        document.write_list_section(
            SECTION_BLOCKS,
            &self.blocks,
            &self.unparsed,
            &[],
            RecurringBlock::render,
        );
        document.render()
    }

    pub fn blocks(&self) -> &[RecurringBlock] {
        &self.blocks
    }

    pub fn problems(&self) -> &[ParseError] {
        &self.problems
    }

    /// Adds a block, or replaces the one with the same id.
    pub fn upsert(&mut self, block: RecurringBlock) {
        match self
            .blocks
            .iter_mut()
            .find(|existing| existing.id == block.id)
        {
            Some(existing) => *existing = block,
            None => self.blocks.push(block),
        }
    }

    /// Replaces every block. Says what the caller means, without the
    /// remove-each-then-upsert-each dance that is quadratic and hard to read.
    pub fn replace_all(&mut self, blocks: Vec<RecurringBlock>) {
        self.blocks.clear();
        for block in blocks {
            self.upsert(block);
        }
    }

    pub fn remove(&mut self, id: &BlockId) -> bool {
        let before = self.blocks.len();
        self.blocks.retain(|block| &block.id != id);
        self.blocks.len() != before
    }

    /// Instances of the repeating blocks on `date`, minus anything skipped.
    pub fn on(&self, date: NaiveDate, skipped: &[BlockId]) -> Vec<Occurrence> {
        let weekday = date.weekday();
        self.blocks
            .iter()
            .filter(|block| block.days.contains(weekday))
            .filter(|block| !skipped.contains(&block.id))
            .map(|block| Occurrence {
                date,
                start: block.start,
                end: block.end,
                project: block.project.clone(),
                title: block.title.clone(),
                remind_before: block.remind_before,
                block: Some(block.id.clone()),
                one_off_index: None,
            })
            .collect()
    }
}

impl Default for Recurring {
    fn default() -> Self {
        let mut document = Document::new();
        document.set_preamble(vec![
            String::new(),
            "# Recurring schedule".to_owned(),
            String::new(),
        ]);
        Self {
            blocks: Vec::new(),
            unparsed: Vec::new(),
            problems: Vec::new(),
            document,
        }
    }
}

/// Everything planned for one day: the repeating blocks that survive its skip
/// list, plus its own one-offs, in start order.
///
/// This is the single definition of "what is scheduled", shared by the schedule
/// screen and the reminder scheduler — two readers that must never disagree.
pub fn planned(day: &crate::day::Day, recurring: &Recurring) -> Vec<Occurrence> {
    let mut occurrences = recurring.on(day.date(), day.skipped());

    // Numbered here, from the day's own list, so the index is the one
    // `Day::replace_block` takes rather than a position recovered by counting
    // entries in the merged list — which would depend on how the sort below
    // breaks a tie.
    occurrences.extend(
        day.schedule()
            .iter()
            .enumerate()
            .map(|(index, block)| Occurrence {
                date: day.date(),
                start: block.start,
                end: block.end,
                project: block.project.clone(),
                title: block.title.clone(),
                remind_before: block.remind_before,
                block: None,
                one_off_index: Some(index),
            }),
    );

    occurrences.sort_by_key(|occurrence| (occurrence.start, occurrence.end));
    occurrences
}

/// Everything planned across a range, oldest first.
///
/// The range counterpart to [`planned`]; both the HTTP API and the MCP server
/// were expanding this loop themselves.
pub fn planned_range(
    store: &crate::store::Store,
    range: crate::report::DateRange,
) -> crate::error::Result<Vec<Occurrence>> {
    let recurring = store.read_recurring()?;
    let mut occurrences = Vec::new();

    for date in range.dates() {
        let day = store.read_day(date)?;
        occurrences.extend(planned(&day, &recurring));
    }

    Ok(occurrences)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    fn date(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, day).expect("valid date")
    }

    fn id(text: &str) -> BlockId {
        BlockId::new(text).expect("valid id")
    }

    const SAMPLE: &str = "---\n---\n\n# Recurring schedule\n\n## Blocks\n\n- `deep-work` mon-fri 09:00-11:00 [[timemd]] Deep work !5m\n- `review` wed 14:00-15:00 [[admin]] Weekly review !10m\n";

    /// The names are the editable form: a day picker sends them back joined,
    /// and the canonical spelling — ranges, `daily` — is re-derived here.
    #[test]
    fn day_names_round_trip_through_the_spec() {
        for spec in ["daily", "mon-fri", "mon,wed,fri", "mon-fri,sun", "sun"] {
            let parsed: DaySet = spec.parse().expect("parses");
            assert_eq!(parsed.names().join(",").parse(), Ok(parsed), "{spec}");
            assert_eq!(
                parsed
                    .names()
                    .join(",")
                    .parse::<DaySet>()
                    .map(|set| set.to_string()),
                Ok(spec.to_owned())
            );
        }
        assert_eq!(
            "mon-fri".parse::<DaySet>().expect("parses").names(),
            vec!["mon", "tue", "wed", "thu", "fri"]
        );
        assert!(DaySet::default().names().is_empty());
    }

    #[test]
    fn parses_day_specs() {
        assert_eq!("daily".parse(), Ok(DaySet::ALL));
        assert_eq!(
            "mon-fri".parse::<DaySet>().expect("parses"),
            DaySet::from_weekdays([
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri
            ])
        );
        assert_eq!(
            "mon,wed,fri".parse::<DaySet>().expect("parses"),
            DaySet::from_weekdays([Weekday::Mon, Weekday::Wed, Weekday::Fri])
        );
        assert_eq!(
            "mon-fri,sun".parse::<DaySet>().expect("parses"),
            DaySet::from_weekdays([
                Weekday::Mon,
                Weekday::Tue,
                Weekday::Wed,
                Weekday::Thu,
                Weekday::Fri,
                Weekday::Sun
            ])
        );
    }

    #[test]
    fn rejects_nonsense_day_specs() {
        for candidate in ["", "funday", "fri-mon", "mon-", "-fri", "mon..fri"] {
            assert!(
                candidate.parse::<DaySet>().is_err(),
                "{candidate:?} should be rejected"
            );
        }
    }

    #[test]
    fn renders_day_specs_canonically() {
        for (input, expected) in [
            ("daily", "daily"),
            ("mon-fri", "mon-fri"),
            ("mon,wed,fri", "mon,wed,fri"),
            ("sat,sun", "sat,sun"),
            ("mon-fri,sun", "mon-fri,sun"),
            ("mon", "mon"),
            ("mon-tue", "mon,tue"),
        ] {
            let parsed: DaySet = input.parse().expect("parses");
            assert_eq!(parsed.to_string(), expected, "from {input:?}");
        }
    }

    #[test]
    fn day_specs_round_trip() {
        for bits in 1_u8..0b1000_0000 {
            let original = "mon-fri"
                .parse::<DaySet>()
                .map(|_| DaySet(bits))
                .expect("constructed");
            let rendered = original.to_string();
            assert_eq!(rendered.parse(), Ok(original), "{rendered}");
        }
    }

    #[test]
    fn parses_recurring_blocks() {
        let recurring = Recurring::parse(SAMPLE).expect("parses");
        assert_eq!(recurring.blocks().len(), 2);

        let first = &recurring.blocks()[0];
        assert_eq!(first.id, id("deep-work"));
        assert_eq!((first.start, first.end), (at(9, 0), at(11, 0)));
        assert_eq!(
            first.project.as_ref().map(ProjectSlug::as_str),
            Some("timemd")
        );
        assert_eq!(first.title, "Deep work");
        assert_eq!(first.remind_before, Some(Minutes::new(5)));
        assert!(recurring.problems().is_empty());
    }

    #[test]
    fn round_trips_a_canonical_file() {
        let recurring = Recurring::parse(SAMPLE).expect("parses");
        assert_eq!(recurring.render(), SAMPLE);
    }

    #[test]
    fn a_block_without_a_project_or_reminder_round_trips() {
        let text = "---\n---\n\n## Blocks\n\n- `gym` mon,thu 18:00-19:00 Training\n";
        let recurring = Recurring::parse(text).expect("parses");

        assert_eq!(recurring.blocks()[0].project, None);
        assert_eq!(recurring.blocks()[0].remind_before, None);
        assert_eq!(recurring.blocks()[0].title, "Training");
        assert_eq!(recurring.render(), text);
    }

    #[test]
    fn keeps_unparseable_block_lines_and_reports_them() {
        let text = "---\n---\n\n## Blocks\n\n- `ok` mon 09:00-10:00 Fine\n- missing an id mon 09:00-10:00\n- `bad-days` funday 09:00-10:00 Nope\n";
        let recurring = Recurring::parse(text).expect("parses");

        assert_eq!(recurring.blocks().len(), 1);
        assert_eq!(recurring.problems().len(), 2);

        let rendered = recurring.render();
        assert!(
            rendered.contains("- missing an id mon 09:00-10:00"),
            "{rendered}"
        );
        assert!(
            rendered.contains("- `bad-days` funday 09:00-10:00 Nope"),
            "{rendered}"
        );
    }

    #[test]
    fn upserts_and_removes_by_id() {
        let mut recurring = Recurring::parse(SAMPLE).expect("parses");

        recurring.upsert(RecurringBlock {
            id: id("deep-work"),
            days: DaySet::from_weekdays([Weekday::Mon]),
            start: at(8, 0),
            end: at(9, 0),
            project: None,
            title: "Shorter".to_owned(),
            remind_before: None,
        });
        assert_eq!(recurring.blocks().len(), 2);
        assert_eq!(recurring.blocks()[0].title, "Shorter");

        assert!(recurring.remove(&id("review")));
        assert!(!recurring.remove(&id("review")));
        assert_eq!(recurring.blocks().len(), 1);
    }

    #[test]
    fn expands_only_matching_weekdays() {
        let recurring = Recurring::parse(SAMPLE).expect("parses");

        // 2026-08-05 is a Wednesday; 2026-08-08 a Saturday.
        let wednesday = recurring.on(date(5), &[]);
        assert_eq!(wednesday.len(), 2);

        let saturday = recurring.on(date(8), &[]);
        assert!(saturday.is_empty());
    }

    #[test]
    fn expansion_honours_skips() {
        let recurring = Recurring::parse(SAMPLE).expect("parses");
        let remaining = recurring.on(date(5), &[id("deep-work")]);

        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].block, Some(id("review")));
        assert_eq!(remaining[0].duration(), Minutes::new(60));
    }

    #[test]
    fn parses_and_renders_one_off_day_blocks() {
        let block = DayBlock::parse("16:00-17:00 [[reading]] Paper club !15m").expect("parses");

        assert_eq!((block.start, block.end), (at(16, 0), at(17, 0)));
        assert_eq!(
            block.project.as_ref().map(ProjectSlug::as_str),
            Some("reading")
        );
        assert_eq!(block.title, "Paper club");
        assert_eq!(block.remind_before, Some(Minutes::new(15)));
        assert_eq!(block.render(), "- 16:00-17:00 [[reading]] Paper club !15m");
    }

    #[test]
    fn a_bare_day_block_renders_without_trailing_space() {
        let block = DayBlock::parse("16:00-17:00 Lunch").expect("parses");
        assert_eq!(block.render(), "- 16:00-17:00 Lunch");
    }

    #[test]
    fn a_titleless_block_still_renders_cleanly() {
        let block = DayBlock::parse("16:00-17:00").expect("parses");
        assert_eq!(block.title, "");
        assert_eq!(block.render(), "- 16:00-17:00");
    }

    #[test]
    fn planned_merges_repeats_and_one_offs_in_start_order() {
        use crate::day::Day;

        let recurring = Recurring::parse(SAMPLE).expect("parses");
        // 2026-08-05 is a Wednesday, so both repeating blocks apply.
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 12:00-12:30 Lunch\n";
        let day = Day::parse(date(5), text).expect("parses");

        let occurrences = planned(&day, &recurring);
        let titles: Vec<&str> = occurrences
            .iter()
            .map(|occurrence| occurrence.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Deep work", "Lunch", "Weekly review"]);
        assert_eq!(
            occurrences[1].block, None,
            "a one-off has no repeating source"
        );
    }

    /// The handle every surface hands back to `replace_block` and
    /// `remove_block`, so it is a position among *this day's* one-offs and
    /// nothing else. Carried on the occurrence rather than counted after the
    /// merge, which is what let a range-wide count drift past day one.
    #[test]
    fn one_offs_are_numbered_per_day_across_a_range() {
        use crate::day::Day;

        let directory = tempfile::tempdir().expect("temp dir");
        let store = crate::store::Store::new(directory.path());
        for day in [5, 6] {
            store
                .update_day(date(day), |target: &mut Day| {
                    target.add_block(DayBlock::parse("12:00-12:30 Lunch").expect("parses"));
                    target.add_block(DayBlock::parse("18:00-18:30 Gym").expect("parses"));
                })
                .expect("writes");
        }

        let range = crate::report::DateRange::new(date(5), date(6)).expect("valid range");
        let numbered: Vec<(String, Option<usize>)> = planned_range(&store, range)
            .expect("expands")
            .into_iter()
            .map(|occurrence| (occurrence.title, occurrence.one_off_index))
            .collect();

        assert_eq!(
            numbered,
            vec![
                ("Lunch".to_owned(), Some(0)),
                ("Gym".to_owned(), Some(1)),
                ("Lunch".to_owned(), Some(0)),
                ("Gym".to_owned(), Some(1)),
            ],
            "the second day restarts at 0"
        );
    }

    #[test]
    fn a_repeat_carries_no_one_off_index() {
        use crate::day::Day;

        let recurring = Recurring::parse(SAMPLE).expect("parses");
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 12:00-12:30 Lunch\n";
        let day = Day::parse(date(5), text).expect("parses");

        let numbered: Vec<Option<usize>> = planned(&day, &recurring)
            .iter()
            .map(|occurrence| occurrence.one_off_index)
            .collect();
        assert_eq!(numbered, vec![None, Some(0), None]);
    }

    #[test]
    fn planned_drops_skipped_repeats_but_keeps_one_offs() {
        use crate::day::Day;

        let recurring = Recurring::parse(SAMPLE).expect("parses");
        let text = "---\ndate: 2026-08-05\n---\n\n## Schedule\n\n- 12:00-12:30 Lunch\n\n## Skipped\n\n- `deep-work`\n- `review`\n";
        let day = Day::parse(date(5), text).expect("parses");

        let occurrences = planned(&day, &recurring);
        assert_eq!(occurrences.len(), 1);
        assert_eq!(occurrences[0].title, "Lunch");
    }

    #[test]
    fn a_range_expands_every_day_in_order() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = crate::store::Store::new(directory.path());
        store
            .update_recurring(|recurring| {
                recurring.replace_all(Recurring::parse(SAMPLE).expect("parses").blocks().to_vec());
            })
            .expect("writes");

        // 2026-08-05 is a Wednesday, 2026-08-08 a Saturday.
        let range = crate::report::DateRange::new(date(5), date(8)).expect("valid range");
        let occurrences = planned_range(&store, range).expect("expands");

        let dates: Vec<String> = occurrences
            .iter()
            .map(|occurrence| occurrence.date.to_string())
            .collect();
        assert_eq!(
            dates,
            vec!["2026-08-05", "2026-08-05", "2026-08-06", "2026-08-07"]
        );
    }

    #[test]
    fn replace_all_swaps_the_whole_list() {
        let mut recurring = Recurring::parse(SAMPLE).expect("parses");
        assert_eq!(recurring.blocks().len(), 2);

        recurring.replace_all(vec![RecurringBlock {
            id: id("gym"),
            days: DaySet::from_weekdays([Weekday::Mon]),
            start: at(18, 0),
            end: at(19, 0),
            project: None,
            title: "Training".to_owned(),
            remind_before: None,
        }]);

        assert_eq!(recurring.blocks().len(), 1);
        assert_eq!(recurring.blocks()[0].id, id("gym"));
    }

    #[test]
    fn a_default_file_renders_a_template() {
        assert_eq!(
            Recurring::default().render(),
            "---\n---\n\n# Recurring schedule\n"
        );
    }

    #[test]
    fn rejects_a_block_line_missing_its_id() {
        assert!(matches!(
            RecurringBlock::parse("mon 09:00-10:00 No id"),
            Err(ParseErrorKind::MissingBlockId { .. })
        ));
        assert!(matches!(
            RecurringBlock::parse("`unclosed mon 09:00-10:00"),
            Err(ParseErrorKind::MissingBlockId { .. })
        ));
        assert!(matches!(
            RecurringBlock::parse("`Bad Id` mon 09:00-10:00"),
            Err(ParseErrorKind::MissingBlockId { .. })
        ));
    }
}
