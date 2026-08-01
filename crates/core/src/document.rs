//! The markdown document model every stored file is built on.
//!
//! A document is YAML frontmatter plus a body split into `##` sections. Typed
//! layers (projects, days, schedules) read and replace only the sections they
//! understand; everything else — unknown sections, prose, unknown frontmatter
//! keys — is carried through a write untouched. That property is what lets an
//! agent add `## Retrospective` to a day file and still have it there tomorrow.

use serde::Serialize;
use serde::de::DeserializeOwned;
use yaml_serde::{Mapping, Value};

use crate::error::{ParseError, ParseErrorKind};
use crate::grammar;

const FENCE: &str = "---";

/// A `##` section: its title and its raw body lines, heading excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub lines: Vec<String>,
}

impl Section {
    /// Body lines with blanks dropped, paired with their 1-based line number in
    /// the section body. Typed parsers work from this.
    pub fn content(&self) -> impl Iterator<Item = (usize, &str)> {
        self.lines
            .iter()
            .enumerate()
            .map(|(index, line)| (index + 1, line.trim_end()))
            .filter(|(_, line)| !line.trim().is_empty())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    front: Mapping,
    has_frontmatter: bool,
    /// Lines before the first `##` heading, e.g. the `# 2026-08-01` title.
    preamble: Vec<String>,
    sections: Vec<Section>,
}

impl Document {
    /// An empty document that will render with a frontmatter block.
    pub fn new() -> Self {
        Self {
            front: Mapping::new(),
            has_frontmatter: true,
            preamble: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Splits raw file text into frontmatter and sections.
    ///
    /// Only malformed YAML fails; body text is never rejected.
    pub fn parse(text: &str) -> Result<Self, yaml_serde::Error> {
        let (front, has_frontmatter, body) = split_frontmatter(text)?;

        let mut preamble = Vec::new();
        let mut sections: Vec<Section> = Vec::new();

        for line in body.split('\n') {
            match line.strip_prefix("## ") {
                Some(title) => sections.push(Section {
                    title: title.trim().to_owned(),
                    lines: Vec::new(),
                }),
                None => match sections.last_mut() {
                    Some(section) => section.lines.push(line.to_owned()),
                    None => preamble.push(line.to_owned()),
                },
            }
        }

        Ok(Self {
            front,
            has_frontmatter,
            preamble,
            sections,
        })
    }

    /// Reassembles the file text.
    pub fn render(&self) -> String {
        let mut output = String::new();

        if self.has_frontmatter || !self.front.is_empty() {
            output.push_str(FENCE);
            output.push('\n');
            if !self.front.is_empty() {
                // A Mapping never fails to serialise, so the fallback is unreachable
                // in practice; returning empty beats panicking on a write path.
                output.push_str(&yaml_serde::to_string(&self.front).unwrap_or_default());
            }
            output.push_str(FENCE);
            output.push('\n');
        }

        let mut first = true;
        let mut line = |text: &str, output: &mut String| {
            if !first {
                output.push('\n');
            }
            first = false;
            output.push_str(text);
        };

        for text in &self.preamble {
            line(text, &mut output);
        }
        for section in &self.sections {
            line(&format!("## {}", section.title), &mut output);
            for text in &section.lines {
                line(text, &mut output);
            }
        }

        output
    }

    pub fn section(&self, title: &str) -> Option<&Section> {
        self.sections
            .iter()
            .find(|section| section.title.eq_ignore_ascii_case(title))
    }

    /// Replaces a section's content, or inserts the section if absent.
    ///
    /// A new section is placed after the last of `after` that exists, which
    /// keeps known sections in their canonical order even when the file was
    /// created with only some of them. Passing empty content removes the
    /// section entirely, so an empty day does not accumulate empty headings.
    pub fn upsert_section(&mut self, title: &str, content: Vec<String>, after: &[&str]) {
        if content.is_empty() {
            self.remove_section(title);
            return;
        }

        let mut lines = Vec::with_capacity(content.len() + 2);
        lines.push(String::new());
        lines.extend(content);
        lines.push(String::new());

        match self.position(title) {
            Some(index) => self.sections[index].lines = lines,
            None => {
                let insert_at = after
                    .iter()
                    .filter_map(|preceding| self.position(preceding))
                    .max()
                    .map_or(0, |index| index + 1);
                self.sections.insert(
                    insert_at,
                    Section {
                        title: title.to_owned(),
                        lines,
                    },
                );
            }
        }
    }

    fn remove_section(&mut self, title: &str) {
        if let Some(index) = self.position(title) {
            self.sections.remove(index);
        }
    }

    fn position(&self, title: &str) -> Option<usize> {
        self.sections
            .iter()
            .position(|section| section.title.eq_ignore_ascii_case(title))
    }

    /// Reads one frontmatter key. Absent and unreadable are both `None`, which
    /// suits the lenient-read rule: a garbled value falls back to a default
    /// rather than failing the whole file.
    pub fn front_key<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let value = self.front.get(Value::from(key))?;
        yaml_serde::from_value(value.clone()).ok()
    }

    /// Writes one frontmatter key, leaving every other key untouched.
    pub fn set_front_key<T: Serialize>(&mut self, key: &str, value: &T) {
        if let Ok(encoded) = yaml_serde::to_value(value) {
            self.front.insert(Value::from(key), encoded);
        }
    }

    pub fn remove_front_key(&mut self, key: &str) {
        self.front.remove(Value::from(key));
    }

    /// Reads a section of list items leniently.
    ///
    /// This is the one implementation of the crate's central rule: a line that
    /// does not parse is preserved verbatim and reported, never dropped. Every
    /// owned section goes through here so that no future section can forget —
    /// which is exactly how `## Skipped` once lost its malformed lines.
    ///
    /// Returns the parsed items and the lines that failed, in file order.
    pub fn parse_list_section<T>(
        &self,
        title: &str,
        parse: impl Fn(&str) -> std::result::Result<T, ParseErrorKind>,
        problems: &mut Vec<ParseError>,
    ) -> (Vec<T>, Vec<String>) {
        let mut items = Vec::new();
        let mut unparsed = Vec::new();

        let Some(section) = self.section(title) else {
            return (items, unparsed);
        };

        for (line_number, line) in section.content() {
            match grammar::list_item(line).and_then(&parse) {
                Ok(item) => items.push(item),
                Err(kind) => {
                    problems.push(ParseError::new(line_number, kind));
                    unparsed.push(line.to_owned());
                }
            }
        }

        (items, unparsed)
    }

    /// Writes a section of list items back, re-emitting anything unparsed.
    ///
    /// The counterpart to [`Document::parse_list_section`]: unparsed lines land
    /// at the end of the section, which is where a user looking for what went
    /// wrong will find them.
    pub fn write_list_section<T>(
        &mut self,
        title: &str,
        items: &[T],
        unparsed: &[String],
        after: &[&str],
        render: impl Fn(&T) -> String,
    ) {
        let mut lines: Vec<String> = items.iter().map(render).collect();
        lines.extend(unparsed.iter().cloned());
        self.upsert_section(title, lines, after);
    }

    /// Replaces the preamble, used when creating a file from a template.
    pub fn set_preamble(&mut self, lines: Vec<String>) {
        self.preamble = lines;
    }

    pub fn is_empty(&self) -> bool {
        self.front.is_empty()
            && self.sections.is_empty()
            && self.preamble.iter().all(|line| line.trim().is_empty())
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the parsed frontmatter, whether a frontmatter block was present, and
/// the remaining body.
fn split_frontmatter(text: &str) -> Result<(Mapping, bool, &str), yaml_serde::Error> {
    let Some(rest) = text.strip_prefix(FENCE) else {
        return Ok((Mapping::new(), false, text));
    };
    let Some(rest) = rest.strip_prefix('\n') else {
        return Ok((Mapping::new(), false, text));
    };

    // The closing fence is a line consisting solely of `---`.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == FENCE {
            let yaml = &rest[..offset];
            let body = &rest[offset + line.len()..];
            let front = if yaml.trim().is_empty() {
                Mapping::new()
            } else {
                yaml_serde::from_str(yaml)?
            };
            return Ok((front, true, body));
        }
        offset += line.len();
    }

    // An unterminated fence is prose, not frontmatter.
    Ok((Mapping::new(), false, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = "---\ndate: 2026-08-01\n---\n\n# 2026-08-01\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] file store\n\n## Notes\n\nFree-form prose.\n";

    #[test]
    fn round_trips_a_canonical_document_byte_for_byte() {
        let document = Document::parse(DAY).expect("parses");
        assert_eq!(document.render(), DAY);
    }

    #[test]
    fn reads_frontmatter_and_sections() {
        let document = Document::parse(DAY).expect("parses");
        assert_eq!(
            document.front_key::<String>("date"),
            Some("2026-08-01".to_owned())
        );
        assert_eq!(
            document
                .section("Sessions")
                .map(|section| section.content().count()),
            Some(1)
        );
        assert!(document.section("Notes").is_some());
        assert!(document.section("Nope").is_none());
    }

    #[test]
    fn preserves_unknown_sections_and_keys_across_a_write() {
        let source = "---\ndate: 2026-08-01\nmood: focused\n---\n\n## Sessions\n\n- 09:00-09:25 (25m) [[timemd]] old\n\n## Retrospective\n\nAgent-authored prose\nover several lines.\n";
        let mut document = Document::parse(source).expect("parses");
        document.upsert_section(
            "Sessions",
            vec!["- 10:00-10:25 (25m) [[timemd]] new".to_owned()],
            &[],
        );

        let rendered = document.render();
        assert!(rendered.contains("mood: focused"), "{rendered}");
        assert!(
            rendered.contains("## Retrospective\n\nAgent-authored prose\nover several lines.\n"),
            "{rendered}"
        );
        assert!(rendered.contains("- 10:00-10:25 (25m) [[timemd]] new"));
        assert!(!rendered.contains("old"));
    }

    #[test]
    fn inserts_new_sections_in_canonical_order() {
        let source = "---\ndate: 2026-08-01\n---\n\n## Sessions\n\n- 09:00-09:25 (25m) work\n";
        let mut document = Document::parse(source).expect("parses");
        document.upsert_section("Schedule", vec!["- 16:00-17:00 talk".to_owned()], &[]);
        document.upsert_section("Notes", vec!["hi".to_owned()], &["Sessions", "Schedule"]);

        let rendered = document.render();
        let schedule = rendered.find("## Schedule").expect("schedule present");
        let sessions = rendered.find("## Sessions").expect("sessions present");
        let notes = rendered.find("## Notes").expect("notes present");
        assert!(schedule < sessions, "{rendered}");
        assert!(sessions < notes, "{rendered}");
    }

    #[test]
    fn empty_content_removes_the_section() {
        let mut document = Document::parse(DAY).expect("parses");
        document.upsert_section("Sessions", Vec::new(), &[]);
        let rendered = document.render();
        assert!(!rendered.contains("## Sessions"), "{rendered}");
        assert!(rendered.contains("## Notes"), "{rendered}");
    }

    #[test]
    fn handles_a_document_without_frontmatter() {
        let source = "# Just prose\n\n## Notes\n\nhello\n";
        let document = Document::parse(source).expect("parses");
        assert_eq!(document.render(), source);
    }

    #[test]
    fn treats_an_unterminated_fence_as_prose() {
        let source = "---\ndate: 2026-08-01\n\n# no closing fence\n";
        let document = Document::parse(source).expect("parses");
        assert_eq!(document.render(), source);
        assert_eq!(document.front_key::<String>("date"), None);
    }

    #[test]
    fn preserves_an_empty_frontmatter_block() {
        let source = "---\n---\n\n## Notes\n\nhi\n";
        let document = Document::parse(source).expect("parses");
        assert_eq!(document.render(), source);
    }

    #[test]
    fn setting_a_key_leaves_the_others_alone() {
        let mut document = Document::parse(DAY).expect("parses");
        document.set_front_key("timezone", &"Europe/Berlin");
        let rendered = document.render();
        assert!(rendered.contains("date: 2026-08-01"), "{rendered}");
        assert!(rendered.contains("timezone: Europe/Berlin"), "{rendered}");
    }

    #[test]
    fn rejects_malformed_yaml_frontmatter() {
        assert!(Document::parse("---\nkey: [unclosed\n---\n\nbody\n").is_err());
    }

    #[test]
    fn section_content_skips_blank_lines_and_numbers_from_one() {
        let document = Document::parse("## Sessions\n\n- a\n\n- b\n").expect("parses");
        let section = document.section("Sessions").expect("present");
        let collected: Vec<_> = section.content().collect();
        assert_eq!(collected, vec![(2, "- a"), (4, "- b")]);
    }
}
