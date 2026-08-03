//! `data/state/ntfy.md` — where notifications go when a browser cannot take them.
//!
//! ntfy is the second notification channel. Web Push needs a service worker the
//! browser is willing to wake, which on a phone is a promise iOS does not
//! reliably keep; ntfy needs an app and a topic name.
//!
//! This lives under `state/` rather than in `settings.md` because a topic on a
//! public server is a bearer capability: anyone who knows the name can read
//! every notification it carries. A token is a secret outright. `settings.md` is
//! meant to be read, diffed and committed, and neither of these is.

use crate::document::Document;
use crate::error::{Error, Result};

/// The public ntfy server, used unless the file names another.
pub const DEFAULT_SERVER: &str = "https://ntfy.sh";

/// Where notifications are published, and what proves the right to publish.
#[derive(Debug, Clone)]
pub struct NtfyConfig {
    /// Base URL of the ntfy server.
    pub server: String,
    /// The topic to publish to. `None` means the channel is off.
    pub topic: Option<String>,
    /// Bearer token for a protected topic. `None` means the topic is public,
    /// and so readable by anyone who guesses its name.
    pub token: Option<String>,
    /// The app's external origin, which the server cannot work out for itself.
    /// `None` means notifications carry no link to follow.
    pub app_url: Option<String>,
    document: Document,
}

impl Default for NtfyConfig {
    fn default() -> Self {
        let mut document = Document::new();
        document.set_preamble(vec![
            String::new(),
            "# ntfy".to_owned(),
            String::new(),
            "Contains a credential. Do not commit or share this file.".to_owned(),
            String::new(),
        ]);
        Self {
            server: DEFAULT_SERVER.to_owned(),
            topic: None,
            token: None,
            app_url: None,
            document,
        }
    }
}

impl NtfyConfig {
    /// Reads the config, falling back for any value that is missing or blank.
    pub fn parse(text: &str) -> std::result::Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        Ok(Self {
            // A file that names the key but leaves it empty would otherwise
            // publish to no host at all — worse than publishing to the default.
            server: value(&document, "server").unwrap_or_else(|| DEFAULT_SERVER.to_owned()),
            topic: value(&document, "topic"),
            token: value(&document, "token"),
            app_url: value(&document, "app_url"),
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();
        document.set_front_key("server", &self.server);
        set_or_clear(&mut document, "topic", self.topic.as_ref());
        set_or_clear(&mut document, "token", self.token.as_ref());
        set_or_clear(&mut document, "app_url", self.app_url.as_ref());
        document.render()
    }

    /// Applies `patch`, leaving every field it does not name alone.
    ///
    /// The write-side half of the rule `parse` reads leniently. A topic is one
    /// URL path segment, so `a/b` would silently publish somewhere else and
    /// `my topic` somewhere nobody is subscribed; a server that is not an
    /// absolute HTTP URL cannot be turned into a request at all. Refusing
    /// rather than falling back is what tells the caller nothing happened.
    ///
    /// A field given as blank is a request to clear it, not a bad value: that is
    /// how a surface with no way to send `null` turns the channel off.
    pub fn apply(&mut self, patch: NtfyPatch) -> Result<()> {
        let server = match patch.server.as_deref().map(str::trim) {
            // Blank resets to the public server rather than clearing the field:
            // there is no useful state in which no server is configured.
            Some("") => Some(DEFAULT_SERVER.to_owned()),
            Some(server) => Some(absolute_url(server, "server")?),
            None => None,
        };
        let topic = clearable(patch.topic, valid_topic)?;
        let token = clearable(patch.token, |token| Ok(token.to_owned()))?;
        let app_url = clearable(patch.app_url, |url| absolute_url(url, "app_url"))?;

        if let Some(server) = server {
            self.server = server;
        }
        if let Some(topic) = topic {
            self.topic = topic;
        }
        if let Some(token) = token {
            self.token = token;
        }
        if let Some(app_url) = app_url {
            self.app_url = app_url;
        }
        Ok(())
    }

    /// Whether notifications have anywhere to go.
    pub fn is_configured(&self) -> bool {
        self.topic.is_some()
    }

    /// Where a publish is POSTed.
    ///
    /// The server root, because ntfy accepts a JSON body naming its own topic
    /// only there. `POST /{topic}` treats the JSON as the literal message text.
    pub fn publish_url(&self) -> String {
        format!("{}/", self.server.trim_end_matches('/'))
    }

    /// What a phone subscribes to, or `None` when the channel is off.
    pub fn topic_url(&self) -> Option<String> {
        let topic = self.topic.as_ref()?;
        Some(format!("{}/{topic}", self.server.trim_end_matches('/')))
    }

    /// Turns a notification's path into something a phone can open, or `None`
    /// when nobody has said where the app answers.
    pub fn click_for(&self, path: &str) -> Option<String> {
        let base = self.app_url.as_ref()?.trim_end_matches('/');
        Some(match path.strip_prefix('/') {
            Some(rest) => format!("{base}/{rest}"),
            None => format!("{base}/{path}"),
        })
    }
}

/// What may be changed. Every field omitted leaves the setting as it was.
///
/// `server` is singly optional because it has no cleared state — a blank one
/// falls back to the public server. The rest are doubly optional: `None` leaves
/// the field, `Some(None)` clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NtfyPatch {
    pub server: Option<String>,
    pub topic: Option<Option<String>>,
    pub token: Option<Option<String>>,
    pub app_url: Option<Option<String>>,
}

impl NtfyPatch {
    /// True when there is nothing to write, so a caller can answer a pure read
    /// without creating a file the tree did not have.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Reads a frontmatter string, treating blank as absent.
fn value(document: &Document, key: &str) -> Option<String> {
    document
        .front_key::<String>(key)
        .map(|raw| raw.trim().to_owned())
        .filter(|raw| !raw.is_empty())
}

fn set_or_clear(document: &mut Document, key: &str, value: Option<&String>) {
    match value {
        Some(value) => document.set_front_key(key, value),
        None => document.remove_front_key(key),
    }
}

/// Resolves one doubly-optional field, where blank means clear.
fn clearable(
    given: Option<Option<String>>,
    check: impl FnOnce(&str) -> Result<String>,
) -> Result<Option<Option<String>>> {
    match given {
        None => Ok(None),
        Some(None) => Ok(Some(None)),
        Some(Some(raw)) => match raw.trim() {
            "" => Ok(Some(None)),
            trimmed => Ok(Some(Some(check(trimmed)?))),
        },
    }
}

fn valid_topic(topic: &str) -> Result<String> {
    let unusable = topic
        .chars()
        .any(|character| character.is_whitespace() || character.is_control() || character == '/');
    if unusable {
        return Err(Error::Invalid(format!(
            "'{topic}' is not a topic: it is one path segment, so it cannot hold a slash or a space"
        )));
    }
    Ok(topic.to_owned())
}

fn absolute_url(url: &str, name: &str) -> Result<String> {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(Error::Invalid(format!(
            "{name} must start with http:// or https://, and '{url}' does not"
        )));
    }
    Ok(url.to_owned())
}

/// Compares the config itself, not the carried-through frontmatter.
impl PartialEq for NtfyConfig {
    fn eq(&self, other: &Self) -> bool {
        self.server == other.server
            && self.topic == other.topic
            && self.token == other.token
            && self.app_url == other.app_url
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_topic(topic: &str) -> NtfyConfig {
        let mut config = NtfyConfig::default();
        config
            .apply(NtfyPatch {
                topic: Some(Some(topic.to_owned())),
                ..NtfyPatch::default()
            })
            .expect("a plain topic is usable");
        config
    }

    /// An untouched tree must not invent a destination, and the channel must be
    /// off until somebody turns it on.
    #[test]
    fn an_empty_config_has_no_topic_and_the_public_server() {
        let config = NtfyConfig::default();

        assert_eq!(config.server, "https://ntfy.sh");
        assert_eq!(config.topic, None);
        assert_eq!(config.token, None);
        assert_eq!(config.app_url, None);
        assert!(!config.is_configured());
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let mut config = NtfyConfig::default();
        config
            .apply(NtfyPatch {
                server: Some("https://ntfy.example".to_owned()),
                topic: Some(Some("timemd-a7f3".to_owned())),
                token: Some(Some("tk_secret".to_owned())),
                app_url: Some(Some("https://box.ts.net".to_owned())),
            })
            .expect("all four are usable");

        let reparsed = NtfyConfig::parse(&config.render()).expect("parses");

        assert_eq!(reparsed, config);
        assert!(reparsed.is_configured());
    }

    /// The file holds a credential, and the only warning a user gets is the one
    /// they see on opening it.
    #[test]
    fn the_file_warns_that_it_holds_a_credential() {
        let rendered = NtfyConfig::default().render();

        assert!(
            rendered.contains("Do not commit or share this file"),
            "{rendered}"
        );
    }

    /// `server:` with nothing after it would otherwise publish to no host at all.
    #[test]
    fn a_blank_server_falls_back_rather_than_publishing_nowhere() {
        let config = NtfyConfig::parse("---\nserver: '   '\ntopic: alpha\n---\n").expect("parses");

        assert_eq!(config.server, DEFAULT_SERVER);
        assert_eq!(config.topic.as_deref(), Some("alpha"));
    }

    /// A `topic: null` line reads as a topic to someone skimming the file.
    #[test]
    fn an_absent_topic_leaves_no_stray_key() {
        let rendered = NtfyConfig::default().render();

        assert!(!rendered.contains("topic"), "{rendered}");
        assert!(!rendered.contains("token"), "{rendered}");
        assert!(!rendered.contains("app_url"), "{rendered}");
    }

    #[test]
    fn unknown_frontmatter_survives() {
        let config = NtfyConfig::parse("---\ntopic: alpha\npriority: 5\n---\n").expect("parses");

        assert!(config.render().contains("priority: 5"));
    }

    #[test]
    fn a_patch_only_touches_the_fields_it_names() {
        let mut config = with_topic("alpha");
        config
            .apply(NtfyPatch {
                app_url: Some(Some("https://box.ts.net".to_owned())),
                ..NtfyPatch::default()
            })
            .expect("a URL is usable");

        assert_eq!(config.topic.as_deref(), Some("alpha"));
        assert_eq!(config.server, DEFAULT_SERVER);
    }

    /// Turning the channel off must not mean re-typing the server, and dropping
    /// a token must not silently drop the topic with it.
    #[test]
    fn clearing_the_topic_turns_the_channel_off_and_clearing_the_token_keeps_the_topic() {
        let mut config = with_topic("alpha");
        config
            .apply(NtfyPatch {
                server: Some("https://ntfy.example".to_owned()),
                token: Some(Some("tk_secret".to_owned())),
                ..NtfyPatch::default()
            })
            .expect("both are usable");

        config
            .apply(NtfyPatch {
                token: Some(None),
                ..NtfyPatch::default()
            })
            .expect("clearing is usable");
        assert_eq!(config.token, None);
        assert_eq!(config.topic.as_deref(), Some("alpha"));

        config
            .apply(NtfyPatch {
                topic: Some(None),
                ..NtfyPatch::default()
            })
            .expect("clearing is usable");
        assert!(!config.is_configured());
        assert_eq!(
            config.server, "https://ntfy.example",
            "turning it off must not forget where it pointed"
        );
    }

    /// A surface with no way to send `null` still has to be able to turn the
    /// channel off.
    #[test]
    fn a_blank_topic_clears_rather_than_failing() {
        let mut config = with_topic("alpha");
        config
            .apply(NtfyPatch {
                topic: Some(Some("  ".to_owned())),
                ..NtfyPatch::default()
            })
            .expect("blank is a request to clear");

        assert!(!config.is_configured());
    }

    /// `a/b` silently publishes somewhere else; `my topic` publishes where
    /// nobody is subscribed. Both look like they worked.
    #[test]
    fn a_topic_with_a_slash_or_a_space_is_refused() {
        for bad in ["alpha/beta", "my topic", "alpha\nbeta"] {
            let mut config = NtfyConfig::default();
            let outcome = config.apply(NtfyPatch {
                topic: Some(Some(bad.to_owned())),
                ..NtfyPatch::default()
            });

            assert!(outcome.is_err(), "'{bad}' must be refused");
            assert!(!config.is_configured(), "'{bad}' must not be stored");
        }
    }

    #[test]
    fn a_server_that_is_not_http_is_refused() {
        let mut config = NtfyConfig::default();
        let outcome = config.apply(NtfyPatch {
            server: Some("ntfy.sh".to_owned()),
            ..NtfyPatch::default()
        });

        assert!(outcome.is_err(), "a bare host cannot become a request");
        assert_eq!(config.server, DEFAULT_SERVER);
    }

    /// A click a phone cannot open is worse than no click at all.
    #[test]
    fn an_app_url_that_is_not_http_is_refused() {
        let mut config = NtfyConfig::default();
        let outcome = config.apply(NtfyPatch {
            app_url: Some(Some("box.ts.net".to_owned())),
            ..NtfyPatch::default()
        });

        assert!(outcome.is_err());
        assert_eq!(config.app_url, None);
    }

    #[test]
    fn a_click_joins_the_app_url_to_the_path_without_doubling_the_slash() {
        let mut config = NtfyConfig::default();
        config
            .apply(NtfyPatch {
                app_url: Some(Some("https://box.ts.net/".to_owned())),
                ..NtfyPatch::default()
            })
            .expect("a URL is usable");

        assert_eq!(
            config.click_for("/today").as_deref(),
            Some("https://box.ts.net/today")
        );
    }

    /// The default must not invent an origin: `http://0.0.0.0:8080/today` opens
    /// nothing, and a link that goes nowhere is worse than none.
    #[test]
    fn there_is_no_click_without_an_app_url() {
        assert_eq!(with_topic("alpha").click_for("/today"), None);
    }

    #[test]
    fn the_publish_url_ends_in_a_slash_and_the_subscribe_url_names_the_topic() {
        let mut config = with_topic("alpha");
        assert_eq!(config.publish_url(), "https://ntfy.sh/");
        assert_eq!(config.topic_url().as_deref(), Some("https://ntfy.sh/alpha"));

        // A server mounted under a subpath, with the trailing slash a user is
        // just as likely to paste as not.
        config
            .apply(NtfyPatch {
                server: Some("https://example.com/ntfy/".to_owned()),
                ..NtfyPatch::default()
            })
            .expect("a URL is usable");

        assert_eq!(config.publish_url(), "https://example.com/ntfy/");
        assert_eq!(
            config.topic_url().as_deref(),
            Some("https://example.com/ntfy/alpha")
        );
    }

    #[test]
    fn there_is_no_subscribe_url_while_the_channel_is_off() {
        assert_eq!(NtfyConfig::default().topic_url(), None);
    }

    #[test]
    fn an_empty_patch_is_empty_and_changes_nothing() {
        assert!(NtfyPatch::default().is_empty());
        assert!(
            !NtfyPatch {
                topic: Some(None),
                ..NtfyPatch::default()
            }
            .is_empty(),
            "turning the channel off is a write"
        );

        let mut config = with_topic("alpha");
        let before = config.clone();
        config.apply(NtfyPatch::default()).expect("changes nothing");
        assert_eq!(config, before);
    }
}
