//! `data/state/push.md` — the VAPID keypair and the browsers subscribed to it.
//!
//! This file holds a private key, which is why `state/` is gitignored and the
//! file is written with owner-only permissions. Everything else in the tree is
//! meant to be read, copied and committed; this one is not.

use serde::{Deserialize, Serialize};

use crate::document::Document;

/// One browser's push subscription, as handed over by the Push API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    /// The client's public key, base64url.
    pub p256dh: String,
    /// The client's auth secret, base64url.
    pub auth: String,
}

/// The push state: one server keypair, many subscribed browsers.
#[derive(Debug, Clone)]
pub struct PushState {
    /// VAPID private key, base64url of the raw 32-byte scalar.
    pub private_key: Option<String>,
    pub subscriptions: Vec<Subscription>,
    document: Document,
}

impl Default for PushState {
    fn default() -> Self {
        let mut document = Document::new();
        document.set_preamble(vec![
            String::new(),
            "# Push".to_owned(),
            String::new(),
            "Contains a private key. Do not commit or share this file.".to_owned(),
            String::new(),
        ]);
        Self {
            private_key: None,
            subscriptions: Vec::new(),
            document,
        }
    }
}

impl PushState {
    pub fn parse(text: &str) -> Result<Self, yaml_serde::Error> {
        let document = Document::parse(text)?;
        Ok(Self {
            private_key: document.front_key("vapid_private"),
            subscriptions: document.front_key("subscriptions").unwrap_or_default(),
            document,
        })
    }

    pub fn render(&self) -> String {
        let mut document = self.document.clone();

        match &self.private_key {
            Some(key) => document.set_front_key("vapid_private", key),
            None => document.remove_front_key("vapid_private"),
        }
        if self.subscriptions.is_empty() {
            document.remove_front_key("subscriptions");
        } else {
            document.set_front_key("subscriptions", &self.subscriptions);
        }

        document.render()
    }

    /// Adds a subscription, replacing any earlier one for the same endpoint.
    ///
    /// Browsers re-issue a subscription for the same endpoint after a key
    /// rotation, so upserting is what keeps one device from accumulating rows.
    pub fn subscribe(&mut self, subscription: Subscription) {
        match self
            .subscriptions
            .iter_mut()
            .find(|existing| existing.endpoint == subscription.endpoint)
        {
            Some(existing) => *existing = subscription,
            None => self.subscriptions.push(subscription),
        }
    }

    /// Drops a subscription. Returns whether it was there.
    ///
    /// Called when a push endpoint reports the subscription gone, which is how
    /// an uninstalled app stops being retried forever.
    pub fn unsubscribe(&mut self, endpoint: &str) -> bool {
        let before = self.subscriptions.len();
        self.subscriptions
            .retain(|subscription| subscription.endpoint != endpoint);
        self.subscriptions.len() != before
    }

    pub fn is_configured(&self) -> bool {
        self.private_key.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subscription(endpoint: &str) -> Subscription {
        Subscription {
            endpoint: endpoint.to_owned(),
            p256dh: "public-key".to_owned(),
            auth: "auth-secret".to_owned(),
        }
    }

    #[test]
    fn an_empty_state_has_no_key_and_no_subscribers() {
        let state = PushState::default();
        assert!(!state.is_configured());
        assert!(state.subscriptions.is_empty());
    }

    #[test]
    fn round_trips_through_render_and_parse() {
        let mut state = PushState {
            private_key: Some("a-private-key".to_owned()),
            ..PushState::default()
        };
        state.subscribe(subscription("https://push.example/1"));
        state.subscribe(subscription("https://push.example/2"));

        let reparsed = PushState::parse(&state.render()).expect("parses");

        assert_eq!(reparsed.private_key.as_deref(), Some("a-private-key"));
        assert_eq!(reparsed.subscriptions.len(), 2);
        assert_eq!(reparsed.subscriptions[0].endpoint, "https://push.example/1");
        assert!(reparsed.is_configured());
    }

    #[test]
    fn the_file_warns_that_it_holds_a_secret() {
        let state = PushState {
            private_key: Some("secret".to_owned()),
            ..PushState::default()
        };
        assert!(
            state.render().contains("Do not commit"),
            "{}",
            state.render()
        );
    }

    #[test]
    fn resubscribing_the_same_endpoint_replaces_rather_than_duplicates() {
        let mut state = PushState::default();
        state.subscribe(subscription("https://push.example/1"));
        state.subscribe(Subscription {
            p256dh: "rotated".to_owned(),
            ..subscription("https://push.example/1")
        });

        assert_eq!(state.subscriptions.len(), 1);
        assert_eq!(state.subscriptions[0].p256dh, "rotated");
    }

    #[test]
    fn unsubscribing_is_idempotent() {
        let mut state = PushState::default();
        state.subscribe(subscription("https://push.example/1"));

        assert!(state.unsubscribe("https://push.example/1"));
        assert!(!state.unsubscribe("https://push.example/1"));
        assert!(state.subscriptions.is_empty());
    }

    #[test]
    fn an_empty_subscription_list_leaves_no_stray_key() {
        let state = PushState {
            private_key: Some("secret".to_owned()),
            ..PushState::default()
        };
        assert!(
            !state.render().contains("subscriptions"),
            "{}",
            state.render()
        );
    }

    #[test]
    fn unknown_frontmatter_survives() {
        let text = "---\nvapid_private: secret\nnote: written by hand\n---\n";
        let state = PushState::parse(text).expect("parses");
        assert!(state.render().contains("note: written by hand"));
    }
}
