//! The filesystem layer.
//!
//! There is no cache and no file watcher: every read goes to disk. At
//! single-user scale a year of day files is a few hundred small reads, and the
//! payoff is that an agent's edit is visible on the very next request with zero
//! invalidation logic to get wrong.
//!
//! I/O here is synchronous so that the CLI and the MCP server can use it without
//! an async runtime. The server calls it directly from handlers; the operations
//! are sub-millisecond on files this size.

use std::convert::Infallible;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

use crate::active::{ActiveSession, IDLE};
use crate::day::Day;
use crate::error::{Error, Result};
use crate::ids::ProjectSlug;
use crate::ntfy::NtfyConfig;
use crate::project::Project;
use crate::push::PushState;
use crate::reminders::SentLog;
use crate::schedule::Recurring;
use crate::settings::Settings;

/// `crate::Result` fixes the error to [`Error`]; an edit refuses with its own.
type StdResult<T, E> = std::result::Result<T, E>;

/// Collapses the outcome of an edit that could not have refused.
///
/// Each infallible `update_*` is its fallible twin with [`Infallible`] as the
/// error, so there is one body per file rather than two that can drift — and
/// only one place where `write_atomic` is called.
fn settled<T>(outcome: Result<StdResult<T, Infallible>>) -> Result<T> {
    outcome.map(|value| match value {
        Ok(value) => value,
        Err(never) => match never {},
    })
}

const PROJECTS_DIR: &str = "projects";
const DAYS_DIR: &str = "days";
const STATE_DIR: &str = "state";
const SCHEDULE_DIR: &str = "schedule";
const RECURRING_FILE: &str = "recurring.md";
const SETTINGS_FILE: &str = "settings.md";
const ACTIVE_FILE: &str = "active.md";
const REMINDERS_FILE: &str = "reminders.md";
const PUSH_FILE: &str = "push.md";
const NTFY_FILE: &str = "ntfy.md";

/// Reads and writes the markdown tree rooted at `root`.
#[derive(Debug)]
pub struct Store {
    root: PathBuf,
    /// Serialises writes so the timer task and an HTTP request cannot interleave.
    write_lock: Mutex<()>,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            write_lock: Mutex::new(()),
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// An instant as the wall-clock time the file grammar stores.
    ///
    /// The single point where an offset becomes a bare `HH:MM`. The server, the
    /// CLI and the MCP server all go through here rather than each reading the
    /// timezone themselves.
    pub fn wall_clock(&self, instant: DateTime<Utc>) -> Result<NaiveDateTime> {
        Ok(self.read_settings()?.wall_clock(instant))
    }

    // ---- projects ----------------------------------------------------------

    pub fn project_path(&self, slug: &ProjectSlug) -> PathBuf {
        self.root.join(PROJECTS_DIR).join(format!("{slug}.md"))
    }

    /// Every project, ordered by slug.
    ///
    /// Files whose name is not a valid slug are skipped rather than failing the
    /// listing — the directory belongs to the user as much as to the app.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let directory = self.root.join(PROJECTS_DIR);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::Io {
                    path: directory,
                    source,
                });
            }
        };

        let mut projects = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|source| Error::Io {
                    path: directory.clone(),
                    source,
                })?
                .path();
            if path.extension().is_none_or(|extension| extension != "md") {
                continue;
            }
            let Some(slug) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| ProjectSlug::new(stem).ok())
            else {
                continue;
            };
            projects.push(self.load_project(&path, slug)?);
        }

        projects.sort_by(|left, right| left.slug().cmp(right.slug()));
        Ok(projects)
    }

    pub fn read_project(&self, slug: &ProjectSlug) -> Result<Option<Project>> {
        let path = self.project_path(slug);
        match read_to_string(&path)? {
            Some(text) => Project::parse(slug.clone(), &text)
                .map(Some)
                .map_err(|source| Error::Frontmatter { path, source }),
            None => Ok(None),
        }
    }

    /// Writes a new project file, refusing to overwrite an existing one.
    pub fn create_project(&self, project: &Project) -> Result<()> {
        self.transaction(|tx| tx.create_project(project))
    }

    /// Applies an edit to an existing project.
    pub fn update_project<T>(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> T,
    ) -> Result<T> {
        self.transaction(|tx| tx.update_project(slug, edit))
    }

    /// Applies an edit that may refuse, writing only if it did not.
    pub fn try_update_project<T, E>(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        self.transaction(|tx| tx.try_update_project(slug, edit))
    }

    /// Removes a project file. Returns whether it existed.
    pub fn delete_project(&self, slug: &ProjectSlug) -> Result<bool> {
        self.transaction(|tx| tx.delete_project(slug))
    }

    fn load_project(&self, path: &Path, slug: ProjectSlug) -> Result<Project> {
        let text = read_to_string(path)?.unwrap_or_default();
        Project::parse(slug, &text).map_err(|source| Error::Frontmatter {
            path: path.to_path_buf(),
            source,
        })
    }

    // ---- days --------------------------------------------------------------

    pub fn day_path(&self, date: NaiveDate) -> PathBuf {
        self.root
            .join(DAYS_DIR)
            .join(date.format("%Y").to_string())
            .join(format!("{date}.md"))
    }

    /// Reads a day, returning an empty one when the file does not exist yet.
    pub fn read_day(&self, date: NaiveDate) -> Result<Day> {
        let path = self.day_path(date);
        match read_to_string(&path)? {
            Some(text) => {
                Day::parse(date, &text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(Day::new(date)),
        }
    }

    /// Applies an edit to a day, creating the file if needed.
    pub fn update_day<T>(&self, date: NaiveDate, edit: impl FnOnce(&mut Day) -> T) -> Result<T> {
        self.transaction(|tx| tx.update_day(date, edit))
    }

    /// Applies an edit that may refuse, writing only if it did not.
    pub fn try_update_day<T, E>(
        &self,
        date: NaiveDate,
        edit: impl FnOnce(&mut Day) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        self.transaction(|tx| tx.try_update_day(date, edit))
    }

    // ---- schedule ----------------------------------------------------------

    pub fn recurring_path(&self) -> PathBuf {
        self.root.join(SCHEDULE_DIR).join(RECURRING_FILE)
    }

    pub fn read_recurring(&self) -> Result<Recurring> {
        let path = self.recurring_path();
        match read_to_string(&path)? {
            Some(text) => {
                Recurring::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(Recurring::default()),
        }
    }

    pub fn update_recurring<T>(&self, edit: impl FnOnce(&mut Recurring) -> T) -> Result<T> {
        self.transaction(|tx| tx.update_recurring(edit))
    }

    // ---- settings and timer state -----------------------------------------

    pub fn settings_path(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }

    pub fn read_settings(&self) -> Result<Settings> {
        let path = self.settings_path();
        match read_to_string(&path)? {
            Some(text) => {
                Settings::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(Settings::default()),
        }
    }

    pub fn update_settings<T>(&self, edit: impl FnOnce(&mut Settings) -> T) -> Result<T> {
        self.transaction(|tx| tx.update_settings(edit))
    }

    /// Applies an edit that may refuse, writing only if it did not.
    pub fn try_update_settings<T, E>(
        &self,
        edit: impl FnOnce(&mut Settings) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        self.transaction(|tx| tx.try_update_settings(edit))
    }

    pub fn active_path(&self) -> PathBuf {
        self.root.join(STATE_DIR).join(ACTIVE_FILE)
    }

    pub fn read_active(&self) -> Result<Option<ActiveSession>> {
        let path = self.active_path();
        match read_to_string(&path)? {
            Some(text) => {
                ActiveSession::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(None),
        }
    }

    /// Replaces the running timer, or clears it with `None`.
    pub fn set_active(&self, session: Option<&ActiveSession>) -> Result<()> {
        self.transaction(|tx| tx.set_active(session))
    }

    pub fn push_path(&self) -> PathBuf {
        self.root.join(STATE_DIR).join(PUSH_FILE)
    }

    pub fn read_push(&self) -> Result<PushState> {
        let path = self.push_path();
        match read_to_string(&path)? {
            Some(text) => {
                PushState::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(PushState::default()),
        }
    }

    pub fn update_push<T>(&self, edit: impl FnOnce(&mut PushState) -> T) -> Result<T> {
        self.transaction(|tx| {
            let mut state = tx.store.read_push()?;
            let outcome = edit(&mut state);
            write_secret(&tx.store.push_path(), &state.render())?;
            Ok(outcome)
        })
    }

    pub fn ntfy_path(&self) -> PathBuf {
        self.root.join(STATE_DIR).join(NTFY_FILE)
    }

    pub fn read_ntfy(&self) -> Result<NtfyConfig> {
        let path = self.ntfy_path();
        match read_to_string(&path)? {
            Some(text) => {
                NtfyConfig::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(NtfyConfig::default()),
        }
    }

    pub fn update_ntfy<T>(&self, edit: impl FnOnce(&mut NtfyConfig) -> T) -> Result<T> {
        self.transaction(|tx| tx.update_ntfy(edit))
    }

    /// Applies an edit that may refuse, writing only if it did not.
    pub fn try_update_ntfy<T, E>(
        &self,
        edit: impl FnOnce(&mut NtfyConfig) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        self.transaction(|tx| tx.try_update_ntfy(edit))
    }

    pub fn reminders_path(&self) -> PathBuf {
        self.root.join(STATE_DIR).join(REMINDERS_FILE)
    }

    pub fn read_sent_reminders(&self) -> Result<SentLog> {
        let path = self.reminders_path();
        match read_to_string(&path)? {
            Some(text) => {
                SentLog::parse(&text).map_err(|source| Error::Frontmatter { path, source })
            }
            None => Ok(SentLog::default()),
        }
    }

    /// Records reminders as sent, holding the lock across read and write so two
    /// ticks cannot both decide the same reminder is unsent.
    pub fn update_sent_reminders<T>(
        &self,
        now: chrono::NaiveDateTime,
        edit: impl FnOnce(&mut SentLog) -> T,
    ) -> Result<T> {
        self.transaction(|tx| {
            let mut log = tx.store.read_sent_reminders()?;
            let outcome = edit(&mut log);
            write_atomic(&tx.store.reminders_path(), &log.render(now))?;
            Ok(outcome)
        })
    }

    /// Runs a sequence of writes atomically with respect to any other writer.
    ///
    /// The timer needs this: logging a finished session and clearing the running
    /// one are two files, and without a boundary around both, the background tick
    /// and a `stop` request can interleave and log the same session twice.
    ///
    /// Write operations live on [`Tx`] rather than on `Store`, so taking the lock
    /// twice and deadlocking is not expressible — from inside a transaction there
    /// is no locking method in scope to call.
    pub fn transaction<T>(&self, work: impl FnOnce(&Tx<'_>) -> T) -> T {
        // A panic elsewhere cannot corrupt the invariant this lock protects —
        // it only orders file writes — so recovering beats propagating.
        let _guard = self
            .write_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&Tx { store: self })
    }
}

/// The write half of [`Store`], reachable only inside [`Store::transaction`].
///
/// Reads are not gated by the lock anywhere: an atomic rename means a reader
/// always sees a whole file, so only writers need ordering.
#[derive(Debug)]
pub struct Tx<'store> {
    store: &'store Store,
}

impl Tx<'_> {
    pub fn read_active(&self) -> Result<Option<ActiveSession>> {
        self.store.read_active()
    }

    pub fn read_settings(&self) -> Result<Settings> {
        self.store.read_settings()
    }

    pub fn read_project(&self, slug: &ProjectSlug) -> Result<Option<Project>> {
        self.store.read_project(slug)
    }

    /// Takes a fully-built project rather than its parts: the caller decides what
    /// a new project looks like, and the file is written exactly once so its keys
    /// land in a predictable order.
    pub fn create_project(&self, project: &Project) -> Result<()> {
        if self.store.project_path(project.slug()).exists() {
            return Err(Error::DuplicateProject(project.slug().to_string()));
        }
        self.write_project(project)
    }

    /// Writes a project whether or not it already exists.
    ///
    /// The upsert primitive: paired with [`Tx::read_project`] inside one
    /// transaction it gives "create or update" without a gap between the check
    /// and the write.
    pub fn write_project(&self, project: &Project) -> Result<()> {
        write_atomic(&self.store.project_path(project.slug()), &project.render())
    }

    pub fn update_project<T>(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> T,
    ) -> Result<T> {
        settled(self.try_update_project(slug, |project| Ok(edit(project))))
    }

    /// Applies an edit that may refuse, writing only if it did not.
    ///
    /// The door for anything fallible. `update_project` cannot tell a refusal
    /// from a value — `T` is opaque to it — so a closure that mutated and *then*
    /// returned `Err` had its half-edit written anyway, and its caller was told
    /// the edit had failed. Two of them did exactly that.
    ///
    /// Generic over `E` because the surfaces do not share an error type: the
    /// server and the shell refuse with [`Error`], the MCP tools with their own.
    pub fn try_update_project<T, E>(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        let path = self.store.project_path(slug);
        let text = read_to_string(&path)?.ok_or_else(|| Error::UnknownProject(slug.to_string()))?;
        let mut project =
            Project::parse(slug.clone(), &text).map_err(|source| Error::Frontmatter {
                path: path.clone(),
                source,
            })?;
        let outcome = edit(&mut project);
        if outcome.is_ok() {
            write_atomic(&path, &project.render())?;
        }
        Ok(outcome)
    }

    /// Logged sessions that reference the slug are left alone: the day files are
    /// a historical record, and silently rewriting history to tidy up a deletion
    /// would be the wrong trade.
    pub fn delete_project(&self, slug: &ProjectSlug) -> Result<bool> {
        let path = self.store.project_path(slug);
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(Error::Io { path, source }),
        }
    }

    pub fn update_day<T>(&self, date: NaiveDate, edit: impl FnOnce(&mut Day) -> T) -> Result<T> {
        settled(self.try_update_day(date, |day| Ok(edit(day))))
    }

    /// Applies an edit that may refuse, writing only if it did not. See
    /// [`Tx::try_update_project`] for why this exists.
    pub fn try_update_day<T, E>(
        &self,
        date: NaiveDate,
        edit: impl FnOnce(&mut Day) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        let mut day = self.store.read_day(date)?;
        let outcome = edit(&mut day);
        if outcome.is_ok() {
            write_atomic(&self.store.day_path(date), &day.render())?;
        }
        Ok(outcome)
    }

    pub fn update_settings<T>(&self, edit: impl FnOnce(&mut Settings) -> T) -> Result<T> {
        settled(self.try_update_settings(|settings| Ok(edit(settings))))
    }

    /// Applies an edit that may refuse, writing only if it did not. See
    /// [`Tx::try_update_project`] for why this exists.
    pub fn try_update_settings<T, E>(
        &self,
        edit: impl FnOnce(&mut Settings) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        let mut settings = self.store.read_settings()?;
        let outcome = edit(&mut settings);
        if outcome.is_ok() {
            write_atomic(&self.store.settings_path(), &settings.render())?;
        }
        Ok(outcome)
    }

    pub fn update_ntfy<T>(&self, edit: impl FnOnce(&mut NtfyConfig) -> T) -> Result<T> {
        settled(self.try_update_ntfy(|config| Ok(edit(config))))
    }

    /// Applies an edit that may refuse, writing only if it did not. See
    /// [`Tx::try_update_project`] for why this exists.
    pub fn try_update_ntfy<T, E>(
        &self,
        edit: impl FnOnce(&mut NtfyConfig) -> StdResult<T, E>,
    ) -> Result<StdResult<T, E>> {
        let mut config = self.store.read_ntfy()?;
        let outcome = edit(&mut config);
        if outcome.is_ok() {
            write_secret(&self.store.ntfy_path(), &config.render())?;
        }
        Ok(outcome)
    }

    pub fn set_active(&self, session: Option<&ActiveSession>) -> Result<()> {
        let text = session.map_or_else(|| IDLE.to_owned(), ActiveSession::render);
        write_atomic(&self.store.active_path(), &text)
    }

    pub fn update_recurring<T>(&self, edit: impl FnOnce(&mut Recurring) -> T) -> Result<T> {
        let mut recurring = self.store.read_recurring()?;
        let outcome = edit(&mut recurring);
        write_atomic(&self.store.recurring_path(), &recurring.render())?;
        Ok(outcome)
    }
}

/// Writes a file that only its owner may read.
///
/// `state/push.md` and `state/ntfy.md` both carry credentials, so neither
/// inherits the permissions the rest of the tree is happy with. One helper
/// rather than the pair written out per file: a write that forgot the second
/// half would leave a secret world-readable and pass every other test.
fn write_secret(path: &Path, contents: &str) -> Result<()> {
    write_atomic(path, contents)?;
    restrict_to_owner(path)
}

/// Restricts a file to owner read/write.
///
/// A no-op off Unix, where the concept does not map cleanly; the deployment this
/// is built for is a Unix box on a tailnet.
fn restrict_to_owner(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|source| {
            Error::Io {
                path: path.to_path_buf(),
                source,
            }
        })?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

/// Reads a file, mapping "not found" to `None`.
fn read_to_string(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Io {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Writes `contents` through a temporary file in the same directory and a
/// rename, and does nothing at all when the file already says exactly that.
///
/// The rename is atomic, so a reader — an agent, an editor, another request —
/// sees either the old file or the new one, never a torn half-write.
///
/// The no-op check is here rather than at each caller because every write goes
/// through this one door. `update_day` and friends hand their closure the file
/// and then write whatever comes back, so an edit that looked, found no session
/// at that index and returned would otherwise re-render and rewrite a
/// git-tracked file it had just declined to change. Comparing costs one read on
/// a path that was about to do a create, a write, an fsync and a rename.
///
/// This covers refusal signalled by *value* only, because the bytes are what it
/// compares. An edit that mutates and then returns `Err` has changed them, and
/// belongs on `try_update_*`, which does not call this at all.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    if read_to_string(path)?.is_some_and(|current| current == contents) {
        return Ok(());
    }

    let directory = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(directory).map_err(|source| Error::Io {
        path: directory.to_path_buf(),
        source,
    })?;

    let file_name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = directory.join(format!(".{file_name}.tmp"));

    let io = |source| Error::Io {
        path: temporary.clone(),
        source,
    };
    let mut file = fs::File::create(&temporary).map_err(io)?;
    file.write_all(contents.as_bytes()).map_err(io)?;
    file.sync_all().map_err(io)?;
    drop(file);

    fs::rename(&temporary, path).map_err(|source| Error::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active::SessionKind;
    use crate::day::Session;
    use crate::minutes::Minutes;
    use chrono::{NaiveTime, Timelike};

    fn store() -> (tempfile::TempDir, Store) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        (directory, store)
    }

    fn date() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 1).expect("valid date")
    }

    fn slug(text: &str) -> ProjectSlug {
        ProjectSlug::new(text).expect("valid slug")
    }

    fn at(hours: u32, minutes: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(hours, minutes, 0).expect("valid time")
    }

    #[test]
    fn an_empty_root_reads_as_empty_rather_than_failing() {
        let (_directory, store) = store();
        assert!(store.list_projects().expect("lists").is_empty());
        assert!(store.read_day(date()).expect("reads").sessions().is_empty());
        assert_eq!(store.read_active().expect("reads"), None);
        assert_eq!(
            store.read_settings().expect("reads").focus,
            Minutes::new(25)
        );
    }

    #[test]
    fn creates_reads_and_lists_projects() {
        let (_directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");
        store
            .create_project(&Project::new(slug("admin"), "Admin", date()))
            .expect("creates");

        let projects = store.list_projects().expect("lists");
        assert_eq!(
            projects
                .iter()
                .map(|p| p.slug().as_str())
                .collect::<Vec<_>>(),
            vec!["admin", "timemd"]
        );

        let project = store
            .read_project(&slug("admin"))
            .expect("reads")
            .expect("present");
        assert_eq!(project.name, "Admin");
        assert!(
            store
                .read_project(&slug("missing"))
                .expect("reads")
                .is_none()
        );
    }

    #[test]
    fn refuses_to_overwrite_an_existing_project() {
        let (_directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");
        assert!(matches!(
            store.create_project(&Project::new(slug("timemd"), "again", date())),
            Err(Error::DuplicateProject(_))
        ));
    }

    #[test]
    fn updates_a_project_in_place() {
        let (_directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");
        store
            .update_project(&slug("timemd"), |project| {
                project.name = "Time MD".to_owned();
            })
            .expect("updates");

        let project = store
            .read_project(&slug("timemd"))
            .expect("reads")
            .expect("present");
        assert_eq!(project.name, "Time MD");
    }

    #[test]
    fn updating_a_missing_project_is_an_error() {
        let (_directory, store) = store();
        let outcome = store.update_project(&slug("ghost"), |project| project.name.clone());
        assert!(matches!(outcome, Err(Error::UnknownProject(_))));
    }

    #[test]
    fn deletes_projects_idempotently() {
        let (_directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");
        assert!(store.delete_project(&slug("timemd")).expect("deletes"));
        assert!(!store.delete_project(&slug("timemd")).expect("deletes"));
    }

    #[test]
    fn skips_directory_entries_that_are_not_project_files() {
        let (directory, store) = store();
        let projects = directory.path().join(PROJECTS_DIR);
        fs::create_dir_all(&projects).expect("creates dir");
        fs::write(projects.join("Not A Slug.md"), "---\n---\n").expect("writes");
        fs::write(projects.join("notes.txt"), "ignored").expect("writes");
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");

        let listed = store.list_projects().expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug().as_str(), "timemd");
    }

    #[test]
    fn writes_days_under_a_year_directory() {
        let (directory, store) = store();
        store
            .update_day(date(), |day| {
                day.add_session(Session::new(
                    at(9, 0),
                    at(9, 25),
                    Some(slug("timemd")),
                    "work",
                ));
            })
            .expect("updates");

        let path = directory.path().join("days/2026/2026-08-01.md");
        assert!(path.exists(), "expected {path:?}");
        let text = fs::read_to_string(&path).expect("reads");
        assert!(
            text.contains("- 09:00-09:25 (25m) [[timemd]] work"),
            "{text}"
        );
    }

    #[test]
    fn day_edits_accumulate_across_calls() {
        let (_directory, store) = store();
        for hour in [9, 10, 11] {
            store
                .update_day(date(), |day| {
                    day.add_session(Session::new(at(hour, 0), at(hour, 25), None, "work"));
                })
                .expect("updates");
        }
        assert_eq!(store.read_day(date()).expect("reads").sessions().len(), 3);
        assert_eq!(
            store.read_day(date()).expect("reads").total(),
            Minutes::new(75)
        );
    }

    #[test]
    fn an_update_returns_the_closure_result() {
        let (_directory, store) = store();
        let total = store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));
                day.total()
            })
            .expect("updates");
        assert_eq!(total, Minutes::new(25));
    }

    #[test]
    fn round_trips_the_running_timer() {
        let (_directory, store) = store();
        let running = ActiveSession::new(
            date().and_hms_opt(9, 0, 0).expect("valid"),
            SessionKind::Focus,
            Minutes::new(25),
            Some(slug("timemd")),
            "file store",
        );
        store.set_active(Some(&running)).expect("writes");
        assert_eq!(store.read_active().expect("reads"), Some(running));

        store.set_active(None).expect("clears");
        assert_eq!(store.read_active().expect("reads"), None);
    }

    #[test]
    fn round_trips_settings() {
        let (_directory, store) = store();
        store
            .update_settings(|settings| settings.focus = Minutes::new(50))
            .expect("updates");
        assert_eq!(
            store.read_settings().expect("reads").focus,
            Minutes::new(50)
        );
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let (directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");
        store.set_active(None).expect("writes");

        // Match on the file name only: the enclosing temp directory is itself
        // named `.tmpXXXX`, so testing the whole path would match everything.
        let leftovers: Vec<_> = walk(directory.path())
            .into_iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".tmp"))
            })
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn surfaces_malformed_frontmatter_with_the_offending_path() {
        let (directory, store) = store();
        let path = directory.path().join("days/2026/2026-08-01.md");
        fs::create_dir_all(path.parent().expect("has parent")).expect("creates dir");
        fs::write(&path, "---\nbroken: [unclosed\n---\n").expect("writes");

        match store.read_day(date()) {
            Err(Error::Frontmatter { path: reported, .. }) => assert_eq!(reported, path),
            other => panic!("expected a frontmatter error, got {other:?}"),
        }
    }

    #[test]
    fn an_agents_hand_edit_survives_an_app_write() {
        let (directory, store) = store();
        let path = directory.path().join("days/2026/2026-08-01.md");
        fs::create_dir_all(path.parent().expect("has parent")).expect("creates dir");
        fs::write(
            &path,
            "---\ndate: 2026-08-01\nmood: focused\n---\n\n## Sessions\n\n- 08:00-08:30 (30m) [[timemd]] by hand\n\n## Retrospective\n\nWritten by an agent.\n",
        )
        .expect("writes");

        store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(9, 0), at(9, 25), None, "by the app"));
            })
            .expect("updates");

        let text = fs::read_to_string(&path).expect("reads");
        assert!(text.contains("mood: focused"), "{text}");
        assert!(
            text.contains("## Retrospective\n\nWritten by an agent.\n"),
            "{text}"
        );
        assert!(
            text.contains("- 08:00-08:30 (30m) [[timemd]] by hand"),
            "{text}"
        );
        assert!(text.contains("- 09:00-09:25 (25m) by the app"), "{text}");
    }

    #[test]
    fn writes_are_visible_to_a_second_store_on_the_same_root() {
        let (directory, store) = store();
        store
            .create_project(&Project::new(slug("timemd"), "timemd", date()))
            .expect("creates");

        // Proves the no-cache design: a separate reader sees the write at once.
        let observer = Store::new(directory.path());
        assert_eq!(observer.list_projects().expect("lists").len(), 1);
    }

    #[test]
    fn day_paths_are_grouped_by_year() {
        let (_directory, store) = store();
        let path = store.day_path(date());
        assert!(path.ends_with("days/2026/2026-08-01.md"), "{path:?}");
    }

    #[test]
    fn a_session_logged_at_midnight_keeps_its_wall_clock_times() {
        let (_directory, store) = store();
        store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(23, 50), at(0, 20), None, "late"));
            })
            .expect("updates");

        let day = store.read_day(date()).expect("reads");
        assert_eq!(day.sessions()[0].start.hour(), 23);
        assert_eq!(day.total(), Minutes::new(30));
    }

    /// An edit that refused must leave the file alone. `update_*` writes back
    /// whatever the closure hands it, so without this a rejected edit would
    /// move the mtime of a git-tracked file it had just declined to change.
    #[test]
    fn an_edit_that_changes_nothing_does_not_touch_the_file() {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = Store::new(directory.path());
        store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));
            })
            .expect("writes");

        let path = store.day_path(date());
        let written = fs::metadata(&path).expect("exists").modified().ok();

        store
            .update_day(date(), |day| {
                // What every refusing edit does: looks, decides nothing
                // happened, and returns.
                assert!(day.remove_session(7).is_none());
            })
            .expect("no-ops");

        assert_eq!(
            fs::metadata(&path).expect("exists").modified().ok(),
            written,
            "the file must not have been rewritten"
        );
    }

    /// The other half of the rule above, and the one the byte-equality check
    /// cannot cover: an edit that mutated and *then* refused. `update_*` writes
    /// whatever the closure leaves behind, so a half-applied edit reached the
    /// file even though its caller was handed an error. `try_update_*` is the
    /// door for anything that can refuse.
    #[test]
    fn an_edit_that_refuses_does_not_write_what_it_had_already_changed() {
        let (_directory, store) = store();
        let slug = slug("thesis");
        store
            .create_project(&Project::new(slug.clone(), "Thesis", date()))
            .expect("creates");

        let outcome: std::result::Result<(), Error> = store
            .try_update_project(&slug, |project| {
                project.name = "Renamed".to_owned();
                Err(Error::Invalid("the edit refused".to_owned()))
            })
            .expect("the store call itself succeeds");

        assert!(outcome.is_err(), "the closure's error must travel out");
        assert_eq!(
            store
                .read_project(&slug)
                .expect("reads")
                .expect("exists")
                .name,
            "Thesis",
            "a refused edit must not be written"
        );
    }

    /// The success half: `try_update_*` still writes, and still hands back the
    /// closure's value.
    #[test]
    fn an_edit_that_succeeds_is_written_and_answers_with_its_value() {
        let (_directory, store) = store();
        let slug = slug("thesis");
        store
            .create_project(&Project::new(slug.clone(), "Thesis", date()))
            .expect("creates");

        let renamed = store
            .try_update_project(&slug, |project| {
                project.name = "Renamed".to_owned();
                Ok::<_, Error>(project.name.clone())
            })
            .expect("the store call")
            .expect("the edit");

        assert_eq!(renamed, "Renamed");
        assert_eq!(
            store
                .read_project(&slug)
                .expect("reads")
                .expect("exists")
                .name,
            "Renamed"
        );
    }

    /// A day and the settings carry the same guarantee — every list a fallible
    /// caller reaches for is behind one of the three.
    #[test]
    fn a_refused_day_or_settings_edit_writes_nothing_either() {
        let (_directory, store) = store();
        store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));
            })
            .expect("writes");

        let refused: std::result::Result<(), Error> = store
            .try_update_day(date(), |day| {
                day.add_session(Session::new(at(14, 0), at(15, 0), None, "extra"));
                Err(Error::Invalid("the edit refused".to_owned()))
            })
            .expect("the store call");
        assert!(refused.is_err());
        assert_eq!(store.read_day(date()).expect("reads").sessions().len(), 1);

        let refused: std::result::Result<(), Error> = store
            .try_update_settings(|settings| {
                settings.focus = Minutes::new(99);
                Err(Error::Invalid("the edit refused".to_owned()))
            })
            .expect("the store call");
        assert!(refused.is_err());
        assert_eq!(
            store.read_settings().expect("reads").focus,
            Settings::default().focus
        );
    }

    /// The whole reason the ntfy config lives under `state/` rather than in
    /// `settings.md`: a topic is a bearer capability and a token is a secret.
    #[test]
    fn the_ntfy_file_is_readable_only_by_its_owner() {
        let (_directory, store) = store();
        store
            .update_ntfy(|config| config.token = Some("tk_secret".to_owned()))
            .expect("writes");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(store.ntfy_path())
                .expect("the file exists")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "ntfy.md holds a credential");
        }
    }

    /// A refused edit must not leave a half-written credential file behind in a
    /// tree that had none.
    #[test]
    fn a_refused_ntfy_edit_writes_nothing() {
        let (_directory, store) = store();

        let refused: std::result::Result<(), Error> = store
            .try_update_ntfy(|config| {
                config.topic = Some("alpha".to_owned());
                Err(Error::Invalid("the edit refused".to_owned()))
            })
            .expect("the store call");

        assert!(refused.is_err());
        assert!(!store.ntfy_path().exists());
        assert!(!store.read_ntfy().expect("reads").is_configured());
    }

    fn walk(root: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = fs::read_dir(root) else {
            return found;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk(&path));
            } else {
                found.push(path);
            }
        }
        found
    }
}
