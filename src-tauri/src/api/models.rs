//! api-v2 response models. Only the fields the app uses; unknown fields are
//! ignored so additions on SoundCloud's side don't break decoding.

use serde::{Deserialize, Serialize};

/// The signed-in user (`GET /me`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Me {
    pub id: u64,
    pub username: String,
    #[serde(default)]
    pub permalink: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub full_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn me_decodes_and_ignores_unknown_fields() {
        let json = r#"{
            "avatar_url": "https://i1.sndcdn.com/avatars-000-large.jpg",
            "city": "Berlin",
            "followers_count": 12,
            "full_name": "",
            "id": 123456,
            "kind": "user",
            "permalink": "someone",
            "username": "Someone",
            "urn": "soundcloud:users:123456"
        }"#;
        let me: Me = serde_json::from_str(json).unwrap();
        assert_eq!(me.id, 123456);
        assert_eq!(me.username, "Someone");
        assert_eq!(me.permalink, "someone");
    }

    #[test]
    fn me_tolerates_missing_optional_fields() {
        let me: Me = serde_json::from_str(r#"{"id":1,"username":"u","avatar_url":null}"#).unwrap();
        assert_eq!(me.avatar_url, None);
        assert_eq!(me.permalink, "");
    }

    #[test]
    fn me_serializes_in_camel_case_for_the_frontend() {
        let me = Me {
            id: 1,
            username: "u".into(),
            permalink: "u".into(),
            avatar_url: None,
            full_name: None,
        };
        assert_eq!(
            serde_json::to_string(&me).unwrap(),
            r#"{"id":1,"username":"u","permalink":"u","avatarUrl":null,"fullName":null}"#
        );
    }
}
