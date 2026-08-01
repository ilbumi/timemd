use std::path::PathBuf;

/// Anything that can go wrong reading or writing the markdown tree.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("{path}: malformed frontmatter: {source}")]
    Frontmatter {
        path: PathBuf,
        #[source]
        source: yaml_serde::Error,
    },

    /// The caller supplied something the domain rejects — a malformed slug, an
    /// unparseable duration, a backwards date range.
    ///
    /// Distinct from [`Error::UnknownProject`] so that "you typed this wrong"
    /// and "that does not exist" do not render as each other.
    #[error("{0}")]
    Invalid(String),

    #[error("no project named {0:?}")]
    UnknownProject(String),

    #[error("a project named {0:?} already exists")]
    DuplicateProject(String),
}

impl From<ParseErrorKind> for Error {
    fn from(kind: ParseErrorKind) -> Self {
        Self::Invalid(kind.to_string())
    }
}

/// A grammar violation, carrying the line it was found on.
///
/// Reads are lenient — a line the app cannot parse is preserved verbatim rather
/// than rejected, because a single typo in a hand-edited file must never make
/// the day unreadable. These errors are collected and reported alongside the
/// data rather than replacing it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {kind}")]
pub struct ParseError {
    pub line: usize,
    pub kind: ParseErrorKind,
}

impl ParseError {
    pub fn new(line: usize, kind: ParseErrorKind) -> Self {
        Self { line, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseErrorKind {
    #[error("expected a list item starting with '- ', found {found:?}")]
    NotAListItem { found: String },

    #[error("expected a HH:MM-HH:MM time range, found {found:?}")]
    MissingTimeRange { found: String },

    #[error("invalid time {found:?}; expected HH:MM")]
    InvalidTime { found: String },

    #[error("invalid duration {found:?}; expected forms like 25m, 1h or 1h30m")]
    InvalidDuration { found: String },

    #[error("invalid project slug {found:?}; expected lowercase letters, digits and dashes")]
    InvalidSlug { found: String },

    #[error("invalid weekday {found:?}; expected mon..sun, a range like mon-fri, or daily")]
    InvalidWeekday { found: String },

    #[error("expected a backtick-quoted block id, found {found:?}")]
    MissingBlockId { found: String },
}

pub type Result<T> = std::result::Result<T, Error>;
