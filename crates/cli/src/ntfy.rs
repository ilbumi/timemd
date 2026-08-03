//! `ntfy` — where notifications go when a browser cannot take them.

use timemd_core::{NtfyConfig, NtfyPatch, Result, Store};

/// Prints the config, having first applied any flag that was given.
///
/// With no flag given this only reads: asking where notifications go must not
/// create a credential file in a tree that had none.
///
/// A flag given empty clears the field — `--topic ''` turns the channel off.
/// `NtfyConfig::apply` owns that rule, so it reads the same on every surface.
pub fn run(
    store: &Store,
    server: Option<String>,
    topic: Option<String>,
    token: Option<String>,
    app_url: Option<String>,
) -> Result<String> {
    let patch = NtfyPatch {
        server,
        topic: topic.map(Some),
        token: token.map(Some),
        app_url: app_url.map(Some),
    };

    if patch.is_empty() {
        return Ok(show(&store.read_ntfy()?));
    }

    store.try_update_ntfy(|config| {
        config.apply(patch)?;
        Ok(show(config))
    })?
}

fn show(config: &NtfyConfig) -> String {
    let mut lines = vec![
        format!("server     {}", config.server),
        format!("topic      {}", config.topic.as_deref().unwrap_or("-")),
        // Whether, never what: printing a token puts it in a shell history.
        format!(
            "token      {}",
            if config.token.is_some() { "(set)" } else { "-" }
        ),
        format!("app-url    {}", config.app_url.as_deref().unwrap_or("-")),
    ];

    // The most useful line at setup: it is what you paste into the ntfy app.
    if let Some(url) = config.topic_url() {
        lines.push(format!("subscribe  {url}"));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use crate::testing::{moment, store};
    use crate::{Command, run};

    fn ntfy(topic: Option<&str>) -> Command {
        Command::Ntfy {
            server: None,
            topic: topic.map(ToOwned::to_owned),
            token: None,
            app_url: None,
        }
    }

    /// Asking where notifications go must not create a 0600 file in a tree that
    /// had none.
    #[test]
    fn reading_the_config_does_not_write_the_file() {
        let (_directory, store) = store();

        let output = run(&store, ntfy(None), moment(9, 0)).expect("reads");

        assert!(output.contains("server     https://ntfy.sh"), "{output}");
        assert!(output.contains("topic      -"), "{output}");
        assert!(!store.ntfy_path().exists());
    }

    #[test]
    fn a_topic_is_set_and_the_server_stays_put() {
        let (_directory, store) = store();

        let output = run(&store, ntfy(Some("timemd-a7f3")), moment(9, 0)).expect("writes");

        assert!(output.contains("topic      timemd-a7f3"), "{output}");
        assert!(
            output.contains("subscribe  https://ntfy.sh/timemd-a7f3"),
            "the line you paste into the app is the point: {output}"
        );
    }

    #[test]
    fn an_empty_topic_turns_the_channel_off() {
        let (_directory, store) = store();
        run(&store, ntfy(Some("timemd-a7f3")), moment(9, 0)).expect("writes");

        let output = run(&store, ntfy(Some("")), moment(9, 0)).expect("clears");

        assert!(output.contains("topic      -"), "{output}");
        assert!(!output.contains("subscribe"), "{output}");
    }

    /// A token printed once is a token in a shell history forever.
    #[test]
    fn the_token_is_never_printed() {
        let (_directory, store) = store();

        let output = run(
            &store,
            Command::Ntfy {
                server: None,
                topic: Some("timemd-a7f3".to_owned()),
                token: Some("tk_secret".to_owned()),
                app_url: None,
            },
            moment(9, 0),
        )
        .expect("writes");

        assert!(output.contains("token      (set)"), "{output}");
        assert!(!output.contains("tk_secret"), "{output}");
    }

    #[test]
    fn an_unusable_topic_is_rejected() {
        let (_directory, store) = store();

        assert!(run(&store, ntfy(Some("alpha/beta")), moment(9, 0)).is_err());
        assert!(!store.ntfy_path().exists());
    }
}
