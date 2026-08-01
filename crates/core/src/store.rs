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

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::NaiveDate;

use crate::active::{ActiveSession, IDLE};
use crate::day::Day;
use crate::error::{Error, Result};
use crate::ids::ProjectSlug;
use crate::project::Project;
use crate::settings::Settings;

const PROJECTS_DIR: &str = "projects";
const DAYS_DIR: &str = "days";
const STATE_DIR: &str = "state";
const SETTINGS_FILE: &str = "settings.md";
const ACTIVE_FILE: &str = "active.md";

/// Reads and writes the markdown tree rooted at `root`.
#[derive(Debug)]
pub struct Store {
    root: PathBuf,
    /// Serialises read-modify-write cycles so the timer task and an HTTP request
    /// cannot interleave on the same file. Held by the `update_*` methods for
    /// the whole cycle, which is why those exist instead of a bare `write`.
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

    /// Creates a project file, refusing to overwrite an existing one.
    pub fn create_project(&self, slug: ProjectSlug, name: &str, today: NaiveDate) -> Result<Project> {
        let _guard = self.lock();
        let path = self.project_path(&slug);
        if path.exists() {
            return Err(Error::DuplicateProject(slug.to_string()));
        }
        let project = Project::new(slug, name, today);
        write_atomic(&path, &project.render())?;
        Ok(project)
    }

    /// Applies an edit to an existing project, holding the write lock across the
    /// whole read-modify-write cycle.
    pub fn update_project<T>(
        &self,
        slug: &ProjectSlug,
        edit: impl FnOnce(&mut Project) -> T,
    ) -> Result<T> {
        let _guard = self.lock();
        let path = self.project_path(slug);
        let text = read_to_string(&path)?.ok_or_else(|| Error::UnknownProject(slug.to_string()))?;
        let mut project = Project::parse(slug.clone(), &text).map_err(|source| Error::Frontmatter {
            path: path.clone(),
            source,
        })?;
        let outcome = edit(&mut project);
        write_atomic(&path, &project.render())?;
        Ok(outcome)
    }

    /// Removes a project file. Returns whether it existed.
    ///
    /// Logged sessions that reference the slug are left alone: the day files are
    /// a historical record, and silently rewriting history to tidy up a deletion
    /// would be the wrong trade.
    pub fn delete_project(&self, slug: &ProjectSlug) -> Result<bool> {
        let _guard = self.lock();
        let path = self.project_path(slug);
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(source) => Err(Error::Io { path, source }),
        }
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

    /// Applies an edit to a day, creating the file if needed and holding the
    /// write lock across the whole cycle.
    pub fn update_day<T>(&self, date: NaiveDate, edit: impl FnOnce(&mut Day) -> T) -> Result<T> {
        let _guard = self.lock();
        let mut day = self.read_day(date)?;
        let outcome = edit(&mut day);
        write_atomic(&self.day_path(date), &day.render())?;
        Ok(outcome)
    }

    /// Days in `from..=to` that have a file, oldest first.
    pub fn recorded_days(&self, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        from.iter_days()
            .take_while(|date| *date <= to)
            .filter(|date| self.day_path(*date).exists())
            .collect()
    }

    // ---- settings and timer state -----------------------------------------

    pub fn settings_path(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }

    pub fn read_settings(&self) -> Result<Settings> {
        let path = self.settings_path();
        match read_to_string(&path)? {
            Some(text) => Settings::parse(&text).map_err(|source| Error::Frontmatter { path, source }),
            None => Ok(Settings::default()),
        }
    }

    pub fn update_settings<T>(&self, edit: impl FnOnce(&mut Settings) -> T) -> Result<T> {
        let _guard = self.lock();
        let mut settings = self.read_settings()?;
        let outcome = edit(&mut settings);
        write_atomic(&self.settings_path(), &settings.render())?;
        Ok(outcome)
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
        let _guard = self.lock();
        let text = session.map_or_else(|| IDLE.to_owned(), ActiveSession::render);
        write_atomic(&self.active_path(), &text)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        // A panic elsewhere cannot corrupt the invariant this lock protects —
        // it only orders file writes — so recovering beats propagating.
        self.write_lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
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

/// Writes via a temporary file in the same directory, then renames.
///
/// The rename is atomic, so a reader — an agent, an editor, another request —
/// sees either the old file or the new one, never a torn half-write.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
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
        assert_eq!(store.read_settings().expect("reads").focus, Minutes::new(25));
    }

    #[test]
    fn creates_reads_and_lists_projects() {
        let (_directory, store) = store();
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");
        store.create_project(slug("admin"), "Admin", date()).expect("creates");

        let projects = store.list_projects().expect("lists");
        assert_eq!(
            projects.iter().map(|p| p.slug().as_str()).collect::<Vec<_>>(),
            vec!["admin", "timemd"]
        );

        let project = store.read_project(&slug("admin")).expect("reads").expect("present");
        assert_eq!(project.name, "Admin");
        assert!(store.read_project(&slug("missing")).expect("reads").is_none());
    }

    #[test]
    fn refuses_to_overwrite_an_existing_project() {
        let (_directory, store) = store();
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");
        assert!(matches!(
            store.create_project(slug("timemd"), "again", date()),
            Err(Error::DuplicateProject(_))
        ));
    }

    #[test]
    fn updates_a_project_in_place() {
        let (_directory, store) = store();
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");
        store
            .update_project(&slug("timemd"), |project| {
                project.name = "Time MD".to_owned();
            })
            .expect("updates");

        let project = store.read_project(&slug("timemd")).expect("reads").expect("present");
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
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");
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
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");

        let listed = store.list_projects().expect("lists");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].slug().as_str(), "timemd");
    }

    #[test]
    fn writes_days_under_a_year_directory() {
        let (directory, store) = store();
        store
            .update_day(date(), |day| {
                day.add_session(Session::new(at(9, 0), at(9, 25), Some(slug("timemd")), "work"));
            })
            .expect("updates");

        let path = directory.path().join("days/2026/2026-08-01.md");
        assert!(path.exists(), "expected {path:?}");
        let text = fs::read_to_string(&path).expect("reads");
        assert!(text.contains("- 09:00-09:25 (25m) [[timemd]] work"), "{text}");
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
        assert_eq!(store.read_day(date()).expect("reads").total(), Minutes::new(75));
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
    fn lists_only_days_that_have_files() {
        let (_directory, store) = store();
        let later = NaiveDate::from_ymd_opt(2026, 8, 5).expect("valid date");
        for target in [date(), later] {
            store
                .update_day(target, |day| {
                    day.add_session(Session::new(at(9, 0), at(9, 25), None, "work"));
                })
                .expect("updates");
        }

        let recorded = store.recorded_days(date(), NaiveDate::from_ymd_opt(2026, 8, 31).expect("valid"));
        assert_eq!(recorded, vec![date(), later]);
        assert!(store.recorded_days(date(), date().pred_opt().expect("valid")).is_empty());
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
        assert_eq!(store.read_settings().expect("reads").focus, Minutes::new(50));
    }

    #[test]
    fn leaves_no_temporary_files_behind() {
        let (directory, store) = store();
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");
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
        assert!(text.contains("## Retrospective\n\nWritten by an agent.\n"), "{text}");
        assert!(text.contains("- 08:00-08:30 (30m) [[timemd]] by hand"), "{text}");
        assert!(text.contains("- 09:00-09:25 (25m) by the app"), "{text}");
    }

    #[test]
    fn writes_are_visible_to_a_second_store_on_the_same_root() {
        let (directory, store) = store();
        store.create_project(slug("timemd"), "timemd", date()).expect("creates");

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
