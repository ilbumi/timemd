//! The todo list: `data/todos.md`.
//!
//! One global file rather than a section per project, because a todo outlives
//! the project it belongs to and half of them belong to no project at all. The
//! line grammar is [Obsidian Tasks' emoji format][tasks], so the same file is
//! editable by hand, by an agent, and by Obsidian.
//!
//! Two rules make that work in both directions. **Reads accept the signifiers in
//! any order**, which is what a hand-edited file looks like. **Writes emit one
//! canonical order**, which is what makes a second write a no-op — and an
//! idempotent write is the whole reason the tree can be edited from two places
//! at once.
//!
//! Recurrence is parsed and preserved, not executed: `🔁 every day when done`
//! survives a round trip untouched, and ticking a recurring todo does not spawn
//! the next one. Obsidian already does that, and a rule engine is its own
//! project.
//!
//! [tasks]: https://publish.obsidian.md/tasks/Reference/Task+Formats/Tasks+Emoji+Format

use std::fmt;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveTime};

use crate::document::Document;
use crate::error::{Error, ParseError, ParseErrorKind, Result};
use crate::grammar;
use crate::ids::{ProjectSlug, TodoId};

const SECTION_TODOS: &str = "Todos";

const PRIORITY_HIGHEST: char = '🔺';
const PRIORITY_HIGH: char = '⏫';
const PRIORITY_MEDIUM: char = '🔼';
const PRIORITY_LOW: char = '🔽';
const PRIORITY_LOWEST: char = '⏬';
const CREATED: char = '➕';
const START: char = '🛫';
const SCHEDULED: char = '⏳';
const DUE: char = '📅';
const DONE: char = '✅';
const CANCELLED: char = '❌';
const RECURRENCE: char = '🔁';
const ID: char = '🆔';
const DEPENDS_ON: char = '⛔';
const ON_COMPLETION: char = '🏁';

/// Every character that opens a field, and therefore closes the one before it.
///
/// One list rather than a match arm per parser: the description is "everything
/// up to the first of these", so a signifier this set forgets would silently be
/// swallowed into the description and lost on the next write.
const SIGNIFIERS: [char; 15] = [
    PRIORITY_HIGHEST,
    PRIORITY_HIGH,
    PRIORITY_MEDIUM,
    PRIORITY_LOW,
    PRIORITY_LOWEST,
    CREATED,
    START,
    SCHEDULED,
    DUE,
    DONE,
    CANCELLED,
    RECURRENCE,
    ID,
    DEPENDS_ON,
    ON_COMPLETION,
];

/// The emoji-presentation selector, which some editors append to a signifier.
/// Read and dropped; never written.
const VARIATION_SELECTOR: char = '\u{fe0f}';

/// A todo's checkbox.
///
/// `Other` exists because the checkbox charset is not ours: Obsidian lets a
/// user define any single character as a status, and a line carrying one must
/// survive a write rather than being reported as broken. An unknown character
/// counts as not-yet-done, which is the reading that cannot lose work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TodoStatus {
    #[default]
    Open,
    Done,
    Cancelled,
    Other(char),
}

impl TodoStatus {
    /// The character between the brackets.
    pub fn symbol(self) -> char {
        match self {
            Self::Open => ' ',
            Self::Done => 'x',
            Self::Cancelled => '-',
            Self::Other(symbol) => symbol,
        }
    }

    /// Whether this todo still wants doing. Cancelled counts as settled.
    pub fn is_open(self) -> bool {
        !matches!(self, Self::Done | Self::Cancelled)
    }

    fn from_symbol(symbol: char) -> Self {
        match symbol {
            ' ' => Self::Open,
            'x' | 'X' => Self::Done,
            '-' => Self::Cancelled,
            other => Self::Other(other),
        }
    }
}

impl fmt::Display for TodoStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => formatter.write_str("open"),
            Self::Done => formatter.write_str("done"),
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::Other(symbol) => write!(formatter, "{symbol}"),
        }
    }
}

impl FromStr for TodoStatus {
    type Err = Error;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw {
            "open" | "todo" => Ok(Self::Open),
            "done" => Ok(Self::Done),
            "cancelled" => Ok(Self::Cancelled),
            other => {
                let mut characters = other.chars();
                match (characters.next(), characters.next()) {
                    (Some(symbol), None) => Ok(Self::from_symbol(symbol)),
                    _ => Err(Error::Invalid(format!(
                        "unknown status {other:?}; expected open, done, cancelled \
                         or a single checkbox character"
                    ))),
                }
            }
        }
    }
}

/// How urgent a todo is. `Normal` is the absence of a signifier, not a symbol of
/// its own, which is why it renders as nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    Highest,
    High,
    Medium,
    #[default]
    Normal,
    Low,
    Lowest,
}

impl Priority {
    pub fn symbol(self) -> Option<char> {
        match self {
            Self::Highest => Some(PRIORITY_HIGHEST),
            Self::High => Some(PRIORITY_HIGH),
            Self::Medium => Some(PRIORITY_MEDIUM),
            Self::Normal => None,
            Self::Low => Some(PRIORITY_LOW),
            Self::Lowest => Some(PRIORITY_LOWEST),
        }
    }

    fn from_symbol(symbol: char) -> Option<Self> {
        match symbol {
            PRIORITY_HIGHEST => Some(Self::Highest),
            PRIORITY_HIGH => Some(Self::High),
            PRIORITY_MEDIUM => Some(Self::Medium),
            PRIORITY_LOW => Some(Self::Low),
            PRIORITY_LOWEST => Some(Self::Lowest),
            _ => None,
        }
    }
}

impl fmt::Display for Priority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Highest => "highest",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Normal => "normal",
            Self::Low => "low",
            Self::Lowest => "lowest",
        })
    }
}

impl FromStr for Priority {
    type Err = Error;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw {
            "highest" => Ok(Self::Highest),
            "high" => Ok(Self::High),
            "medium" => Ok(Self::Medium),
            "normal" => Ok(Self::Normal),
            "low" => Ok(Self::Low),
            "lowest" => Ok(Self::Lowest),
            other => Err(Error::Invalid(format!(
                "unknown priority {other:?}; expected highest, high, medium, normal, low or lowest"
            ))),
        }
    }
}

/// What Obsidian does with a todo once it is ticked.
///
/// Parsed and written back; timemd itself does nothing with it. Dropping the
/// field instead would delete a user's instruction the first time we touched
/// their line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnCompletion {
    #[default]
    Keep,
    Delete,
}

impl fmt::Display for OnCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Keep => "keep",
            Self::Delete => "delete",
        })
    }
}

impl FromStr for OnCompletion {
    type Err = Error;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        match raw {
            "keep" => Ok(Self::Keep),
            "delete" => Ok(Self::Delete),
            other => Err(Error::Invalid(format!(
                "unknown on-completion {other:?}; expected keep or delete"
            ))),
        }
    }
}

/// A date, optionally narrowed to a time of day.
///
/// Obsidian's dates carry no time. The optional `HH:MM` is this app's one
/// deliberate extension, because "scheduled" here has to be able to mean a slot
/// on the day timeline and not just a day. A stamp with no time renders exactly
/// as Obsidian writes it, so a file that never uses one stays byte-identical.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub date: NaiveDate,
    pub time: Option<NaiveTime>,
}

impl Stamp {
    pub fn on(date: NaiveDate) -> Self {
        Self { date, time: None }
    }

    pub fn at(date: NaiveDate, time: NaiveTime) -> Self {
        Self {
            date,
            time: Some(time),
        }
    }
}

impl fmt::Display for Stamp {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&grammar::format_date(self.date))?;
        match self.time {
            Some(time) => write!(formatter, " {}", grammar::format_time(time)),
            None => Ok(()),
        }
    }
}

impl FromStr for Stamp {
    type Err = ParseErrorKind;

    fn from_str(raw: &str) -> std::result::Result<Self, Self::Err> {
        let raw = raw.trim();
        match raw.split_once(char::is_whitespace) {
            None => Ok(Self::on(grammar::date(raw)?)),
            Some((date, time)) => Ok(Self::at(
                grammar::date(date)?,
                grammar::time(time.trim()).map_err(|_| ParseErrorKind::InvalidDate {
                    found: raw.to_owned(),
                })?,
            )),
        }
    }
}

/// One entry in `## Todos`.
///
/// Everything is public except the description, which is private for the same
/// reason a milestone's title is: [`Todo::new`] is the write-side gate, and a
/// field assignment would walk straight past it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Todo {
    pub status: TodoStatus,
    pub project: Option<ProjectSlug>,
    description: String,
    pub priority: Priority,
    pub recurrence: Option<String>,
    /// `None` until something writes this todo, at which point it is minted.
    /// See [`Todos::assign_ids`].
    id: Option<TodoId>,
    pub depends_on: Vec<TodoId>,
    pub created: Option<Stamp>,
    pub start: Option<Stamp>,
    pub scheduled: Option<Stamp>,
    pub due: Option<Stamp>,
    pub cancelled: Option<Stamp>,
    pub done: Option<Stamp>,
    pub on_completion: Option<OnCompletion>,
}

impl Todo {
    /// The one way to build a todo, and so the one place the write-side rule
    /// lives.
    ///
    /// A description is refused when rendering it would produce a line that
    /// reads back as something else: blank or multi-line has nowhere to go, a
    /// signifier would open a field, and a leading `[[` would be eaten as the
    /// project link. Refusing here rather than mangling on the way out is what
    /// lets the round-trip property hold with no exceptions.
    pub fn new(
        status: TodoStatus,
        description: impl AsRef<str>,
    ) -> std::result::Result<Self, ParseErrorKind> {
        let description = description.as_ref().trim();
        let refuse = || ParseErrorKind::InvalidTodo {
            found: description.to_owned(),
        };

        if description.is_empty()
            || description.contains(['\n', '\r'])
            || description.contains(SIGNIFIERS)
            || description.starts_with("[[")
        {
            return Err(refuse());
        }

        Ok(Self {
            status,
            project: None,
            description: description.to_owned(),
            priority: Priority::default(),
            recurrence: None,
            id: None,
            depends_on: Vec::new(),
            created: None,
            start: None,
            scheduled: None,
            due: None,
            cancelled: None,
            done: None,
            on_completion: None,
        })
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// Replaces the description, through the gate.
    pub fn set_description(
        &mut self,
        description: impl AsRef<str>,
    ) -> std::result::Result<(), ParseErrorKind> {
        self.description = Self::new(self.status, description)?.description;
        Ok(())
    }

    pub fn id(&self) -> Option<&TodoId> {
        self.id.as_ref()
    }

    /// The `#tag`s in the description, in the order they appear.
    ///
    /// Derived rather than stored: a tag *is* description text in Obsidian, and
    /// lifting it into a field of its own would mean choosing where to put it
    /// back on the way out. Nothing to choose, nothing to get wrong.
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.description
            .split_whitespace()
            .filter_map(|word| word.strip_prefix('#'))
            .filter(|tag| !tag.is_empty())
    }

    /// Whether this todo still wants doing.
    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }

    /// Applies an edit, field by field.
    ///
    /// The description goes first because it is the only field that can be
    /// refused, and a refusal must not leave half of itself behind — the same
    /// ordering rule `Project::update_milestone` follows, for the same reason.
    ///
    /// On the todo rather than only on [`Todos`] so that a create can build the
    /// line it means to write before it is added, instead of adding a bare todo
    /// and then editing the list to find it again.
    pub fn apply(&mut self, edit: TodoEdit) -> Result<()> {
        if let Some(description) = &edit.description {
            self.set_description(description)?;
        }
        if let Some(status) = edit.status {
            self.status = status;
        }
        if let Some(project) = edit.project {
            self.project = project;
        }
        if let Some(priority) = edit.priority {
            self.priority = priority;
        }
        if let Some(recurrence) = edit.recurrence {
            self.recurrence = recurrence.filter(|rule| !rule.trim().is_empty());
        }
        if let Some(depends_on) = edit.depends_on {
            self.depends_on = depends_on;
        }
        if let Some(created) = edit.created {
            self.created = created;
        }
        if let Some(start) = edit.start {
            self.start = start;
        }
        if let Some(scheduled) = edit.scheduled {
            self.scheduled = scheduled;
        }
        if let Some(due) = edit.due {
            self.due = due;
        }
        if let Some(cancelled) = edit.cancelled {
            self.cancelled = cancelled;
        }
        if let Some(done) = edit.done {
            self.done = done;
        }
        if let Some(on_completion) = edit.on_completion {
            self.on_completion = on_completion;
        }
        Ok(())
    }

    /// Reads a todo line, the bullet already stripped.
    fn parse(content: &str) -> std::result::Result<Self, ParseErrorKind> {
        let refuse = || ParseErrorKind::InvalidTodo {
            found: content.to_owned(),
        };

        let rest = content.strip_prefix('[').ok_or_else(refuse)?;
        let mut characters = rest.chars();
        let symbol = characters.next().ok_or_else(refuse)?;
        let rest = characters.as_str().strip_prefix(']').ok_or_else(refuse)?;

        let (project, rest) = grammar::wikilink(rest.trim_start());
        let (description, fields) = split_at_first_signifier(rest);

        let mut todo = Self::new(TodoStatus::from_symbol(symbol), description)?;
        todo.project = project;
        todo.read_fields(fields, &refuse)?;
        Ok(todo)
    }

    /// Applies the signifier region, which may hold the fields in any order.
    fn read_fields(
        &mut self,
        fields: &str,
        refuse: &impl Fn() -> ParseErrorKind,
    ) -> std::result::Result<(), ParseErrorKind> {
        for (signifier, value) in Fields::over(fields) {
            match signifier {
                CREATED => once(&mut self.created, value.parse()?, refuse)?,
                START => once(&mut self.start, value.parse()?, refuse)?,
                SCHEDULED => once(&mut self.scheduled, value.parse()?, refuse)?,
                DUE => once(&mut self.due, value.parse()?, refuse)?,
                DONE => once(&mut self.done, value.parse()?, refuse)?,
                CANCELLED => once(&mut self.cancelled, value.parse()?, refuse)?,
                ID => once(&mut self.id, TodoId::new(value)?, refuse)?,
                ON_COMPLETION => once(
                    &mut self.on_completion,
                    value.parse().map_err(|_| refuse())?,
                    refuse,
                )?,
                RECURRENCE if !value.is_empty() => {
                    once(&mut self.recurrence, value.to_owned(), refuse)?;
                }
                DEPENDS_ON if !value.is_empty() => {
                    if !self.depends_on.is_empty() {
                        return Err(refuse());
                    }
                    self.depends_on = value
                        .split(',')
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(TodoId::new)
                        .collect::<std::result::Result<_, _>>()?;
                }
                // A priority is a signifier on its own; text after it is not
                // ours to guess at.
                symbol if value.is_empty() => match Priority::from_symbol(symbol) {
                    Some(priority) if self.priority == Priority::Normal => self.priority = priority,
                    _ => return Err(refuse()),
                },
                _ => return Err(refuse()),
            }
        }
        Ok(())
    }

    /// Writes the line, signifiers in one canonical order.
    ///
    /// The order is the reason a second write changes nothing: reads take the
    /// fields however they come, so without a single order on the way out, two
    /// clients would rewrite each other's lines forever.
    fn render(&self) -> String {
        let mut line = format!(
            "- [{}] {}{}",
            self.status.symbol(),
            grammar::format_wikilink(self.project.as_ref()),
            self.description,
        );

        let mut field = |signifier: char, value: &dyn fmt::Display| {
            line.push(' ');
            line.push(signifier);
            let value = value.to_string();
            if !value.is_empty() {
                line.push(' ');
                line.push_str(&value);
            }
        };

        if let Some(symbol) = self.priority.symbol() {
            field(symbol, &"");
        }
        if let Some(rule) = &self.recurrence {
            field(RECURRENCE, rule);
        }
        if let Some(id) = &self.id {
            field(ID, id);
        }
        if !self.depends_on.is_empty() {
            let ids = self
                .depends_on
                .iter()
                .map(TodoId::as_str)
                .collect::<Vec<_>>()
                .join(",");
            field(DEPENDS_ON, &ids);
        }
        for (signifier, stamp) in [
            (CREATED, self.created),
            (START, self.start),
            (SCHEDULED, self.scheduled),
            (DUE, self.due),
            (CANCELLED, self.cancelled),
            (DONE, self.done),
        ] {
            if let Some(stamp) = stamp {
                field(signifier, &stamp);
            }
        }
        if let Some(on_completion) = self.on_completion {
            field(ON_COMPLETION, &on_completion);
        }

        line
    }
}

/// What [`Todos::update`] may change about one todo. Every field omitted leaves
/// it exactly as it was; a field set to `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TodoEdit {
    pub status: Option<TodoStatus>,
    pub description: Option<String>,
    pub project: Option<Option<ProjectSlug>>,
    pub priority: Option<Priority>,
    pub recurrence: Option<Option<String>>,
    pub depends_on: Option<Vec<TodoId>>,
    pub created: Option<Option<Stamp>>,
    pub start: Option<Option<Stamp>>,
    pub scheduled: Option<Option<Stamp>>,
    pub due: Option<Option<Stamp>>,
    pub cancelled: Option<Option<Stamp>>,
    pub done: Option<Option<Stamp>>,
    pub on_completion: Option<Option<OnCompletion>>,
}

/// What a surface narrows the list by. Every field absent means every todo.
///
/// Here rather than at each surface because "which todos" is one question with
/// one answer, and the API, MCP, the CLI and the UI all ask it.
#[derive(Debug, Clone, Default)]
pub struct TodoQuery {
    pub project: Option<ProjectSlug>,
    pub status: Option<TodoStatus>,
    /// Only todos that still want doing. A weaker ask than `status: Open`,
    /// because an unknown checkbox counts as open too.
    pub only_open: bool,
    /// Only todos with a due date on or before this day. The time of day is
    /// ignored: "due before Friday" is a question about days.
    pub due_before: Option<Stamp>,
    /// Only todos scheduled for this day, the time of day again ignored.
    pub scheduled_on: Option<Stamp>,
}

/// The todo file, with its prose and frontmatter intact.
#[derive(Debug, Clone)]
pub struct Todos {
    /// Private so that the doors below really are the only way in — an id is
    /// the address, and a `Vec` anyone could push to is a second id waiting to
    /// happen.
    todos: Vec<Todo>,
    unparsed: Vec<String>,
    problems: Vec<ParseError>,
    document: Document,
}

impl Todos {
    /// An empty list, rendering the same header a new day or project file gets.
    ///
    /// The template matters because this file is read by a person: without it a
    /// tree's first `timemd todo add` writes a bare fence with a `## Todos`
    /// jammed against it.
    pub fn new() -> Self {
        let mut document = Document::new();
        document.set_preamble(vec![String::new(), "# Todos".to_owned(), String::new()]);
        Self {
            todos: Vec::new(),
            unparsed: Vec::new(),
            problems: Vec::new(),
            document,
        }
    }

    pub fn parse(text: &str) -> std::result::Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        let mut problems = Vec::new();
        let (todos, unparsed) =
            document.parse_list_section(SECTION_TODOS, Todo::parse, &mut problems);

        Ok(Self {
            todos,
            unparsed,
            problems,
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();
        document.write_list_section(
            SECTION_TODOS,
            &self.todos,
            &self.unparsed,
            &[],
            Todo::render,
        );
        document.render()
    }

    /// The todos, in file order.
    pub fn todos(&self) -> &[Todo] {
        &self.todos
    }

    /// Todo lines the app could not read, so a broken file is visible rather
    /// than silently half-loaded.
    pub fn problems(&self) -> &[ParseError] {
        &self.problems
    }

    /// Mints an id for every todo that has none, and reports how many.
    ///
    /// Called on the way to disk rather than on the way in, so a file merely
    /// read is never rewritten. A hand-written todo therefore stays exactly as
    /// typed until something edits that file — and once it has an id, every
    /// surface can address it and `⛔` can name it.
    pub fn assign_ids(&mut self) -> usize {
        let mut minted = 0;
        for index in 0..self.todos.len() {
            if self.todos[index].id.is_some() {
                continue;
            }
            let seed = self.todos[index].description.clone();
            self.todos[index].id = Some(self.mint(&seed));
            minted += 1;
        }
        minted
    }

    /// An id no todo in this list already carries.
    fn mint(&self, seed: &str) -> TodoId {
        TodoId::mint(seed, |candidate| {
            self.todos
                .iter()
                .any(|todo| todo.id.as_ref() == Some(candidate))
        })
    }

    /// The position of the todo carrying this id.
    ///
    /// The addressing scheme for every surface. A milestone is addressed by its
    /// title because a project has a handful of them in a deliberate order; a
    /// todo list has hundreds in no order, titles repeat across weeks, and an
    /// index is a position another writer invalidates between two calls.
    pub fn position(&self, id: &TodoId) -> Result<usize> {
        let mut found = self
            .todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| todo.id.as_ref() == Some(id));

        match (found.next(), found.count()) {
            (Some((index, _)), 0) => Ok(index),
            (None, _) => Err(Error::Invalid(format!("no todo with id {id:?}"))),
            (Some(_), rest) => Err(Error::Invalid(format!(
                "{} todos carry the id {id:?}; give one a different id first",
                rest + 1
            ))),
        }
    }

    pub fn get(&self, id: &TodoId) -> Result<&Todo> {
        let index = self.position(id)?;
        Ok(&self.todos[index])
    }

    /// Adds a todo, minting its id if it has none. Returns the id it landed
    /// with, which is the handle every surface answers with.
    pub fn add(&mut self, mut todo: Todo) -> Result<TodoId> {
        let id = match &todo.id {
            Some(id) if self.position(id).is_ok() => {
                return Err(Error::Invalid(format!(
                    "a todo with id {id:?} already exists"
                )));
            }
            Some(id) => id.clone(),
            None => self.mint(&todo.description),
        };
        todo.id = Some(id.clone());
        self.todos.push(todo);
        Ok(id)
    }

    /// Removes the todo carrying `id`, giving it back.
    pub fn remove(&mut self, id: &TodoId) -> Result<Todo> {
        let index = self.position(id)?;
        Ok(self.todos.remove(index))
    }

    /// Applies an edit to the todo carrying `id`.
    ///
    /// The description goes first because it is the only field that can be
    /// refused, and a refusal must not leave half of itself behind — the same
    /// ordering rule `Project::update_milestone` follows, for the same reason.
    pub fn update(&mut self, id: &TodoId, edit: TodoEdit) -> Result<()> {
        let index = self.position(id)?;
        self.todos[index].apply(edit)
    }

    /// The todos a query keeps, in file order.
    pub fn matching(&self, query: &TodoQuery) -> Vec<&Todo> {
        self.todos
            .iter()
            .filter(|todo| !query.only_open || todo.is_open())
            .filter(|todo| query.project.is_none() || todo.project == query.project)
            .filter(|todo| query.status.is_none_or(|wanted| todo.status == wanted))
            .filter(|todo| {
                query
                    .due_before
                    .is_none_or(|limit| todo.due.is_some_and(|due| due.date <= limit.date))
            })
            .filter(|todo| {
                query.scheduled_on.is_none_or(|day| {
                    todo.scheduled
                        .is_some_and(|scheduled| scheduled.date == day.date)
                })
            })
            .collect()
    }

    /// The todos scheduled for `date`, earliest time of day first.
    ///
    /// Here rather than at each surface because "scheduled for today" is one
    /// question with one answer, and the day view, the CLI and an agent all ask
    /// it.
    pub fn scheduled_on(&self, date: NaiveDate) -> Vec<&Todo> {
        let mut due: Vec<&Todo> = self.matching(&TodoQuery {
            scheduled_on: Some(Stamp::on(date)),
            ..TodoQuery::default()
        });
        // A todo with no time of day sorts before one with, so the day's
        // untimed work reads as a heading rather than as midnight.
        due.sort_by_key(|todo| todo.scheduled.and_then(|stamp| stamp.time));
        due
    }
}

/// Fills a field that must appear at most once.
///
/// A field given twice is refused rather than resolved by keeping one: the line
/// is preserved verbatim either way, and guessing which of the two dates the
/// user meant is how the other one gets deleted.
fn once<T>(
    slot: &mut Option<T>,
    parsed: T,
    refuse: &impl Fn() -> ParseErrorKind,
) -> std::result::Result<(), ParseErrorKind> {
    match slot {
        Some(_) => Err(refuse()),
        None => {
            *slot = Some(parsed);
            Ok(())
        }
    }
}

impl Default for Todos {
    fn default() -> Self {
        Self::new()
    }
}

/// Splits a line's content at the first signifier, giving the description and
/// the field region.
fn split_at_first_signifier(text: &str) -> (&str, &str) {
    match text.find(SIGNIFIERS) {
        Some(index) => (text[..index].trim_end(), &text[index..]),
        None => (text.trim_end(), ""),
    }
}

/// Walks a field region as `(signifier, value)` pairs, a value running to the
/// next signifier.
struct Fields<'a> {
    rest: &'a str,
}

impl<'a> Fields<'a> {
    fn over(text: &'a str) -> Self {
        Self { rest: text }
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = (char, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let signifier = self.rest.chars().next()?;
        let mut value = &self.rest[signifier.len_utf8()..];
        // Some editors write the emoji-presentation form. Read it, drop it.
        if let Some(stripped) = value.strip_prefix(VARIATION_SELECTOR) {
            value = stripped;
        }

        let end = value.find(SIGNIFIERS).unwrap_or(value.len());
        self.rest = &value[end..];
        Some((signifier, value[..end].trim()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slug(name: &str) -> ProjectSlug {
        ProjectSlug::new(name).expect("valid slug")
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    fn todo(description: &str) -> Todo {
        Todo::new(TodoStatus::Open, description).expect("valid todo")
    }

    const SAMPLE: &str = "---\n---\n\n## Todos\n\n- [ ] [[timemd]] Draft the release notes ⏫ 🆔 dcf64c ➕ 2026-08-24 ⏳ 2026-08-30 14:00 📅 2026-08-31\n- [x] [[timemd]] Fix the ticker drift 🔺 🆔 0h17ye ⛔ dcf64c ✅ 2026-08-23\n";

    #[test]
    fn reads_every_field_off_one_line() {
        let todos = Todos::parse(SAMPLE).expect("parses");
        let first = &todos.todos()[0];

        assert_eq!(first.description(), "Draft the release notes");
        assert_eq!(first.project, Some(slug("timemd")));
        assert_eq!(first.priority, Priority::High);
        assert_eq!(first.id().map(TodoId::as_str), Some("dcf64c"));
        assert_eq!(first.created, Some(Stamp::on(date(2026, 8, 24))));
        assert_eq!(
            first.scheduled,
            Some(Stamp::at(date(2026, 8, 30), at(14, 0)))
        );
        assert_eq!(first.due, Some(Stamp::on(date(2026, 8, 31))));
        assert!(first.is_open());
        assert!(todos.problems().is_empty());
    }

    #[test]
    fn reads_dependencies_and_a_tick() {
        let todos = Todos::parse(SAMPLE).expect("parses");
        let second = &todos.todos()[1];

        assert_eq!(second.status, TodoStatus::Done);
        assert!(!second.is_open());
        assert_eq!(second.priority, Priority::Highest);
        assert_eq!(
            second
                .depends_on
                .iter()
                .map(TodoId::as_str)
                .collect::<Vec<_>>(),
            ["dcf64c"]
        );
        assert_eq!(second.done, Some(Stamp::on(date(2026, 8, 23))));
    }

    #[test]
    fn round_trips_a_canonical_file() {
        let todos = Todos::parse(SAMPLE).expect("parses");
        assert_eq!(todos.render(), SAMPLE);
    }

    /// The property the canonical write order buys: a hand-edited line with the
    /// signifiers in any order lands on the same todo, and is written back one
    /// way.
    #[test]
    fn accepts_the_signifiers_in_any_order() {
        let jumbled =
            "---\n---\n\n## Todos\n\n- [ ] Ship it 📅 2026-08-31 🆔 abc123 🔽 ⏳ 2026-08-30\n";
        let todos = Todos::parse(jumbled).expect("parses");
        let only = &todos.todos()[0];

        assert_eq!(only.priority, Priority::Low);
        assert_eq!(only.due, Some(Stamp::on(date(2026, 8, 31))));
        assert_eq!(only.scheduled, Some(Stamp::on(date(2026, 8, 30))));
        assert!(
            todos
                .render()
                .contains("- [ ] Ship it 🔽 🆔 abc123 ⏳ 2026-08-30 📅 2026-08-31"),
            "{}",
            todos.render()
        );
    }

    #[test]
    fn a_recurrence_rule_survives_untouched() {
        let source =
            "---\n---\n\n## Todos\n\n- [ ] Water the plants 🔁 every day when done ⏳ 2026-08-25\n";
        let todos = Todos::parse(source).expect("parses");

        assert_eq!(
            todos.todos()[0].recurrence.as_deref(),
            Some("every day when done")
        );
        assert_eq!(todos.render(), source);
    }

    #[test]
    fn an_unknown_checkbox_character_is_kept_and_reads_as_open() {
        let source = "---\n---\n\n## Todos\n\n- [/] Half done\n";
        let todos = Todos::parse(source).expect("parses");

        assert_eq!(todos.todos()[0].status, TodoStatus::Other('/'));
        assert!(todos.todos()[0].is_open());
        assert_eq!(todos.render(), source);
    }

    #[test]
    fn keeps_and_reports_a_line_it_cannot_read() {
        let source = "---\n---\n\n## Todos\n\n- [ ] Fine\n- [ ] Broken 📅 not-a-date\n";
        let todos = Todos::parse(source).expect("parses");

        assert_eq!(todos.todos().len(), 1);
        assert_eq!(todos.problems().len(), 1);
        assert!(
            todos.render().contains("- [ ] Broken 📅 not-a-date"),
            "{}",
            todos.render()
        );
    }

    /// The same date given twice is refused rather than resolved by keeping
    /// one: the line is preserved either way, and guessing is how the other
    /// one gets deleted.
    #[test]
    fn refuses_a_field_given_twice() {
        for candidate in [
            "[ ] Two dues 📅 2026-08-30 📅 2026-08-31",
            "[ ] Two priorities ⏫ 🔽",
            "[ ] Two ids 🆔 abc123 🆔 def456",
        ] {
            assert!(Todo::parse(candidate).is_err(), "{candidate:?}");
        }
    }

    #[test]
    fn refuses_a_description_it_could_not_read_back() {
        for candidate in [
            "",
            "   ",
            "two\nlines",
            "carriage\rreturn",
            "has a 📅 in it",
            "[[timemd]] leading link",
        ] {
            assert!(
                Todo::new(TodoStatus::Open, candidate).is_err(),
                "{candidate:?} should be refused"
            );
        }
        assert_eq!(todo("  padded  ").description(), "padded");
    }

    #[test]
    fn tags_are_read_out_of_the_description() {
        let tagged = todo("Draft the notes #writing #release");
        assert_eq!(tagged.tags().collect::<Vec<_>>(), ["writing", "release"]);
        assert_eq!(todo("no tags here").tags().count(), 0);
    }

    #[test]
    fn stamps_round_trip_with_and_without_a_time() {
        for raw in ["2026-08-30", "2026-08-30 14:00"] {
            let stamp: Stamp = raw.parse().expect("parses");
            assert_eq!(stamp.to_string(), raw);
        }
        for raw in ["2026-8-30", "30-08-2026", "2026-08-30 14:00:00", "soon", ""] {
            assert!(raw.parse::<Stamp>().is_err(), "{raw:?} should be refused");
        }
    }

    #[test]
    fn an_id_is_minted_on_the_way_to_disk_only() {
        let source = "---\n---\n\n## Todos\n\n- [ ] No id yet\n";
        let mut todos = Todos::parse(source).expect("parses");

        assert_eq!(todos.render(), source, "reading alone rewrites nothing");
        assert_eq!(todos.assign_ids(), 1);
        assert_eq!(todos.assign_ids(), 0, "a second pass mints nothing");

        let id = todos.todos()[0].id().expect("minted").clone();
        assert!(
            todos.render().contains(&format!("🆔 {id}")),
            "{}",
            todos.render()
        );
        assert_eq!(todos.position(&id).ok(), Some(0));
    }

    #[test]
    fn a_new_file_renders_a_template() {
        let mut todos = Todos::default();
        assert_eq!(todos.render(), "---\n---\n\n# Todos\n");

        todos.add(todo("Ship it")).expect("adds");
        assert!(
            todos
                .render()
                .starts_with("---\n---\n\n# Todos\n\n## Todos\n\n- [ ] Ship it"),
            "{}",
            todos.render()
        );
    }

    #[test]
    fn two_todos_sharing_a_description_get_different_ids() {
        let mut todos = Todos::default();
        let first = todos.add(todo("Same words")).expect("adds");
        let second = todos.add(todo("Same words")).expect("adds");
        assert_ne!(first, second);
    }

    #[test]
    fn adds_gets_updates_and_removes_by_id() {
        let mut todos = Todos::default();
        let id = todos.add(todo("Draft the notes")).expect("adds");

        todos
            .update(
                &id,
                TodoEdit {
                    status: Some(TodoStatus::Done),
                    description: Some("Draft the release notes".to_owned()),
                    due: Some(Some(Stamp::on(date(2026, 8, 31)))),
                    ..TodoEdit::default()
                },
            )
            .expect("updates");

        let updated = todos.get(&id).expect("present");
        assert_eq!(updated.description(), "Draft the release notes");
        assert_eq!(updated.status, TodoStatus::Done);
        assert_eq!(updated.due, Some(Stamp::on(date(2026, 8, 31))));

        todos
            .update(
                &id,
                TodoEdit {
                    due: Some(None),
                    ..TodoEdit::default()
                },
            )
            .expect("clears the due date");
        assert_eq!(todos.get(&id).expect("present").due, None);

        assert_eq!(
            todos.remove(&id).expect("removes").description(),
            "Draft the release notes"
        );
        assert!(todos.get(&id).is_err());
        assert!(todos.remove(&id).is_err());
    }

    /// A refusal must not leave half of itself behind, so the only field that
    /// can be refused is applied first.
    #[test]
    fn a_refused_description_leaves_the_tick_alone() {
        let mut todos = Todos::default();
        let id = todos.add(todo("Draft the notes")).expect("adds");

        assert!(
            todos
                .update(
                    &id,
                    TodoEdit {
                        status: Some(TodoStatus::Done),
                        description: Some(String::new()),
                        ..TodoEdit::default()
                    }
                )
                .is_err()
        );
        assert!(todos.get(&id).expect("present").is_open());
    }

    #[test]
    fn refuses_to_add_an_id_that_is_already_there() {
        let mut todos = Todos::default();
        let id = todos.add(todo("First")).expect("adds");

        let mut clash = todo("Second");
        clash.id = Some(id);
        assert!(todos.add(clash).is_err());
        assert_eq!(todos.todos().len(), 1);
    }

    /// Reads are lenient, so a hand-written duplicate id parses and lists fine.
    /// Writes are strict, so addressing one is refused rather than resolved by
    /// picking whichever came first.
    #[test]
    fn refuses_an_id_two_todos_share() {
        let source = "---\n---\n\n## Todos\n\n- [ ] One 🆔 abc123\n- [ ] Two 🆔 abc123\n";
        let todos = Todos::parse(source).expect("parses");

        assert_eq!(todos.todos().len(), 2);
        let id = TodoId::new("abc123").expect("valid");
        let error = todos.position(&id).expect_err("ambiguous");
        assert!(error.to_string().contains('2'), "{error}");
    }

    #[test]
    fn lists_what_is_scheduled_on_a_day_untimed_first() {
        let source = "---\n---\n\n## Todos\n\n- [ ] Afternoon ⏳ 2026-08-30 14:00\n- [ ] Whenever ⏳ 2026-08-30\n- [ ] Morning ⏳ 2026-08-30 09:00\n- [ ] Tomorrow ⏳ 2026-08-31\n";
        let todos = Todos::parse(source).expect("parses");

        let day: Vec<&str> = todos
            .scheduled_on(date(2026, 8, 30))
            .iter()
            .map(|todo| todo.description())
            .collect();
        assert_eq!(day, ["Whenever", "Morning", "Afternoon"]);
    }

    #[test]
    fn preserves_prose_and_unknown_sections_when_edited() {
        let source = "---\nagent_key: kept\n---\n\n# Todos\n\n## Todos\n\n- [ ] One 🆔 abc123\n\n## Someday\n\nA list I keep myself.\n";
        let mut todos = Todos::parse(source).expect("parses");
        todos.add(todo("Two")).expect("adds");

        let rendered = todos.render();
        assert!(rendered.contains("agent_key: kept"), "{rendered}");
        assert!(
            rendered.contains("## Someday\n\nA list I keep myself.\n"),
            "{rendered}"
        );
        assert!(rendered.contains("- [ ] Two"), "{rendered}");
    }

    #[test]
    fn an_emoji_presentation_selector_is_read_and_dropped() {
        let source = "---\n---\n\n## Todos\n\n- [ ] Ship it \u{23f3}\u{fe0f} 2026-08-30\n";
        let todos = Todos::parse(source).expect("parses");

        assert_eq!(
            todos.todos()[0].scheduled,
            Some(Stamp::on(date(2026, 8, 30)))
        );
        assert!(
            !todos.render().contains(VARIATION_SELECTOR),
            "writes the plain form"
        );
    }

    #[test]
    fn statuses_and_priorities_round_trip_through_text() {
        for status in [
            TodoStatus::Open,
            TodoStatus::Done,
            TodoStatus::Cancelled,
            TodoStatus::Other('/'),
        ] {
            assert_eq!(status.to_string().parse().ok(), Some(status));
        }
        assert!("half-way".parse::<TodoStatus>().is_err());

        for priority in [
            Priority::Highest,
            Priority::High,
            Priority::Medium,
            Priority::Normal,
            Priority::Low,
            Priority::Lowest,
        ] {
            assert_eq!(priority.to_string().parse().ok(), Some(priority));
        }
        assert!("urgent".parse::<Priority>().is_err());

        for on_completion in [OnCompletion::Keep, OnCompletion::Delete] {
            assert_eq!(on_completion.to_string().parse().ok(), Some(on_completion));
        }
        assert!("archive".parse::<OnCompletion>().is_err());
    }

    #[test]
    fn rejects_a_line_that_is_not_a_checkbox() {
        for candidate in ["[ ]", "[] nothing", "no box at all", "[x", ""] {
            assert!(Todo::parse(candidate).is_err(), "{candidate:?}");
        }
    }
}
