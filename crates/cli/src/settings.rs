//! `settings` — pomodoro lengths and the reminder default.

use timemd_core::{Minutes, Result, Settings, SettingsPatch, Store};

/// Prints the settings, having first applied any flag that was given.
///
/// With no flag given this only reads: asking how long a focus session is must
/// not rewrite a git-tracked file.
pub fn run(
    store: &Store,
    focus: Option<String>,
    short_break: Option<String>,
    long_break: Option<String>,
    remind_before: Option<String>,
) -> Result<String> {
    let length = |raw: Option<String>| raw.map(|raw| raw.parse::<Minutes>()).transpose();
    let patch = SettingsPatch {
        focus: length(focus)?,
        short_break: length(short_break)?,
        long_break: length(long_break)?,
        remind_before: length(remind_before)?,
    };

    if patch.is_empty() {
        return Ok(show(&store.read_settings()?));
    }

    store.update_settings(|settings| {
        settings.apply(patch)?;
        Ok(show(settings))
    })?
}

fn show(settings: &Settings) -> String {
    [
        // The timezone is what turns every bare wall-clock time in the tree
        // into an instant, so it is shown but changed by editing settings.md.
        format!("timezone         {}", settings.timezone),
        format!("focus            {}", settings.focus),
        format!("short-break      {}", settings.short_break),
        format!("long-break       {}", settings.long_break),
        format!("long-break-every {}", settings.long_break_every),
        format!("remind-before    {}", settings.remind_before),
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use crate::testing::{moment, store};
    use crate::{Command, run};

    fn settings(focus: Option<&str>) -> Command {
        Command::Settings {
            focus: focus.map(ToOwned::to_owned),
            short_break: None,
            long_break: None,
            remind_before: None,
        }
    }

    /// Asking how long a focus session is must not rewrite a git-tracked file.
    #[test]
    fn reading_settings_does_not_write_the_file() {
        let (_directory, store) = store();
        store
            .update_settings(|settings| settings.timezone = chrono_tz::UTC)
            .expect("writes");
        let path = store.settings_path();
        let before = std::fs::read(&path).expect("settings exist");

        let output = run(&store, settings(None), moment(9, 0)).expect("reads");
        assert!(output.contains("focus            25m"), "{output}");
        assert!(output.contains("timezone         UTC"), "{output}");

        assert_eq!(std::fs::read(&path).expect("settings exist"), before);
    }

    #[test]
    fn a_length_is_changed_and_the_others_stay_put() {
        let (_directory, store) = store();

        let output = run(&store, settings(Some("50m")), moment(9, 0)).expect("writes");
        assert!(output.contains("focus            50m"), "{output}");
        assert!(output.contains("short-break      5m"), "{output}");
    }

    #[test]
    fn an_unreadable_duration_is_rejected() {
        let (_directory, store) = store();
        assert!(run(&store, settings(Some("a while")), moment(9, 0)).is_err());
    }

    /// `Settings::parse` falls back when it reads a zero, so writing one would
    /// leave the file saying one thing and the timer doing another.
    #[test]
    fn a_zero_session_length_is_rejected() {
        let (_directory, store) = store();
        assert!(run(&store, settings(Some("0m")), moment(9, 0)).is_err());

        let output = run(&store, settings(None), moment(9, 0)).expect("reads");
        assert!(output.contains("focus            25m"), "{output}");
    }
}
