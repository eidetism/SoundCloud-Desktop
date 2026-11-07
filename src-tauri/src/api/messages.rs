//! Direct messages (`/users/{me}/conversations…`, per `docs/har/messages.har`).
//! api-v2 names a conversation by both participants (`"<a>:<b>"`, either
//! order, or `"<me>:system"`) and addresses it by the other side's id; the
//! frontend gets that id as `with`, the other participant as `peer`, and each
//! message marked `mine` or `system`.

use serde::{Deserialize, Serialize};

use super::models::{Page, User};

pub const SYSTEM: &str = "system";

/// A conversation as api-v2 sends it. `read` is the signed-in user's side:
/// sending a message also leaves it unread until `PUT read=true`.
#[derive(Debug, Deserialize)]
pub struct RawConversation {
    id: String,
    #[serde(default)]
    read: bool,
    #[serde(default)]
    last_message: Option<RawMessage>,
    #[serde(default)]
    users: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub struct RawMessage {
    #[serde(default)]
    content: String,
    #[serde(default)]
    sender: Option<User>,
    /// `user` or `system`.
    #[serde(default)]
    sender_type: String,
    sent_at: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conversation {
    /// The other side's user id, or `"system"`: the key in api-v2 paths.
    pub with: String,
    pub peer: Option<User>,
    pub system: bool,
    pub read: bool,
    pub last_message: Option<Message>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub body: String,
    pub sender: Option<User>,
    pub mine: bool,
    pub system: bool,
    pub sent_at: String,
}

impl RawMessage {
    pub fn view(self, me: u64) -> Message {
        let system = self.sender_type == SYSTEM;
        let mine = !system && self.sender.as_ref().is_some_and(|user| user.id == me);
        Message {
            body: self.content,
            sender: self.sender,
            mine,
            system,
            sent_at: self.sent_at,
        }
    }
}

impl RawConversation {
    /// `None` if the id has no side other than `me`.
    pub fn view(self, me: u64) -> Option<Conversation> {
        let mine = me.to_string();
        let with = self.id.split(':').find(|side| *side != mine)?.to_string();
        if with != SYSTEM && with.parse::<u64>().is_err() {
            return None;
        }
        Some(Conversation {
            system: with == SYSTEM,
            with,
            peer: self.users.into_iter().find(|user| user.id != me),
            read: self.read,
            last_message: self.last_message.map(|message| message.view(me)),
        })
    }
}

pub fn conversations(page: Page<RawConversation>, me: u64) -> Page<Conversation> {
    Page {
        collection: page
            .collection
            .into_iter()
            .filter_map(|conversation| conversation.view(me))
            .collect(),
        next_href: page.next_href,
    }
}

pub fn messages(page: Page<RawMessage>, me: u64) -> Page<Message> {
    Page {
        collection: page
            .collection
            .into_iter()
            .map(|message| message.view(me))
            .collect(),
        next_href: page.next_href,
    }
}

/// A conversation key from the frontend: a user id or `"system"`.
pub fn valid_with(with: &str) -> bool {
    with == SYSTEM || with.parse::<u64>().is_ok()
}

/// What `/resolve?url=` points at; only tracks matter for message previews.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Resolved {
    Track(Box<super::models::Track>),
    #[serde(other)]
    Other,
}

/// A short link (`https://on.soundcloud.com/<code>`). It answers a redirect
/// to the page it stands for.
pub fn short_link(url: &str) -> Option<reqwest::Url> {
    let url = reqwest::Url::parse(url).ok()?;
    let ok = url.scheme() == "https"
        && url.host_str() == Some("on.soundcloud.com")
        && url.port().is_none()
        && url.query().is_none()
        && url.path_segments().is_some_and(|mut segments| {
            segments.next().is_some_and(|code| {
                !code.is_empty() && code.chars().all(|c| c.is_ascii_alphanumeric())
            }) && segments.next().is_none()
        });
    ok.then_some(url)
}

/// A soundcloud.com track page (`https://soundcloud.com/<user>/<track>`).
/// Query and fragment are dropped.
pub fn track_page(url: &str) -> Option<reqwest::Url> {
    let mut url = reqwest::Url::parse(url).ok()?;
    let host = url.host_str()?;
    if url.scheme() != "https"
        || !matches!(
            host,
            "soundcloud.com" | "www.soundcloud.com" | "m.soundcloud.com"
        )
        || url.port().is_some()
    {
        return None;
    }
    let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    if segments.len() != 2 {
        return None;
    }
    url.set_query(None);
    url.set_fragment(None);
    Some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ME: u64 = 1_000_001;

    fn page<T: serde::de::DeserializeOwned>(json: &str) -> Page<T> {
        serde_json::from_str(json).expect("fixture decodes")
    }

    #[test]
    fn conversations_name_the_other_side() {
        let list = conversations(page(include_str!("fixtures/conversations.json")), ME);
        let with: Vec<&str> = list.collection.iter().map(|c| c.with.as_str()).collect();
        assert_eq!(with, ["1000002", "system", "1000003", "1000004"]);
        let first = &list.collection[0];
        assert_eq!(first.peer.as_ref().map(|u| u.id), Some(1_000_002));
        assert!(first.read && !first.system);
        assert!(first.last_message.as_ref().is_some_and(|m| m.mine));
        let system = &list.collection[1];
        assert!(system.system);
        assert_eq!(
            system.peer.as_ref().map(|u| u.username.as_str()),
            Some("SoundCloud")
        );
        let notice = system.last_message.as_ref().unwrap();
        assert!(notice.system && !notice.mine);
        assert!(list.next_href.is_none());
    }

    #[test]
    fn messages_are_newest_first_with_a_cursor() {
        let list = messages(page(include_str!("fixtures/messages.json")), ME);
        assert_eq!(list.collection.len(), 4);
        assert!(list.collection[0].sent_at > list.collection[3].sent_at);
        assert!(list.collection.iter().all(|m| !m.system));
        assert!(list.next_href.as_deref().unwrap().contains("offset=10"));
    }

    #[test]
    fn a_sent_message_comes_back_in_its_conversation() {
        let raw: RawConversation =
            serde_json::from_str(include_str!("fixtures/message_sent.json")).unwrap();
        let sent = raw.view(ME).unwrap();
        assert_eq!(sent.with, "1000002");
        assert!(!sent.read);
        assert!(sent.last_message.unwrap().mine);
    }

    #[test]
    fn odd_conversation_ids_are_dropped() {
        let raw = |id: &str| RawConversation {
            id: id.into(),
            read: true,
            last_message: None,
            users: vec![],
        };
        assert!(raw("1000001").view(ME).is_none());
        assert!(raw("1000001:x/../me").view(ME).is_none());
        assert_eq!(raw("7:1000001").view(ME).unwrap().with, "7");
    }

    #[test]
    fn conversation_keys() {
        assert!(valid_with("system") && valid_with("1000002"));
        assert!(!valid_with("") && !valid_with("1/../me") && !valid_with("-1"));
    }

    #[test]
    fn only_track_pages_are_previewed() {
        assert_eq!(
            track_page("https://soundcloud.com/someartist/some-track?si=1#t=0")
                .unwrap()
                .as_str(),
            "https://soundcloud.com/someartist/some-track"
        );
        for url in [
            "https://on.soundcloud.com/AbCdEf123",
            "https://soundcloud.com/someartist",
            "https://soundcloud.com/someartist/sets/list",
            "http://soundcloud.com/a/b",
            "https://soundcloud.com.evil.example/a/b",
            "https://soundcloud.com:444/a/b",
        ] {
            assert!(track_page(url).is_none(), "{url}");
        }
    }

    #[test]
    fn short_links() {
        assert!(short_link("https://on.soundcloud.com/uCREGxH1vnNfgNJNRt").is_some());
        for url in [
            "https://on.soundcloud.com/",
            "https://on.soundcloud.com/a/b",
            "https://on.soundcloud.com/a?x=1",
            "https://on.soundcloud.com/a-b",
            "http://on.soundcloud.com/abc",
            "https://soundcloud.com/abc",
        ] {
            assert!(short_link(url).is_none(), "{url}");
        }
        // Where a short link from the capture leads.
        let target = "https://soundcloud.com/someone/some-track?ref=sc-messaging&p=a&c=0&si=x&utm_source=sc-messaging";
        assert_eq!(
            track_page(target).unwrap().as_str(),
            "https://soundcloud.com/someone/some-track"
        );
    }

    #[test]
    fn resolve_answers_decode() {
        let track: Resolved =
            serde_json::from_str(r#"{"kind":"track","id":1,"title":"t","duration":1000}"#).unwrap();
        assert!(matches!(track, Resolved::Track(_)));
        let user: Resolved = serde_json::from_str(r#"{"kind":"user","id":1}"#).unwrap();
        assert!(matches!(user, Resolved::Other));
    }
}
