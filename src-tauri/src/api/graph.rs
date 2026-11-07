//! The site's GraphQL endpoint (`graph.soundcloud.com/graphql`), per the
//! track page HARs. Reading comments works from Rust; every mutation is
//! DataDome-protected and goes through the hidden site window. Documents are
//! the web app's own, verbatim, in `graphql/`.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const TRACK_COMMENTS: &str = include_str!("graphql/track_comments.graphql");
pub const REPOST_TRACK: &str = include_str!("graphql/repost_track.graphql");
pub const UNREPOST_TRACK: &str = include_str!("graphql/unrepost_track.graphql");
pub const LIKE_COMMENT: &str = include_str!("graphql/like_comment.graphql");
pub const UNLIKE_COMMENT: &str = include_str!("graphql/unlike_comment.graphql");
pub const CREATE_COMMENT: &str = include_str!("graphql/create_comment.graphql");
pub const DELETE_COMMENT: &str = include_str!("graphql/delete_comment.graphql");

/// Top-level comments per page, as many as the endpoint was seen to accept.
pub const COMMENTS_PER_PAGE: u32 = 50;
/// Replies loaded with each comment, as the site asks (50 is answered with
/// an error). The query for the rest of a thread is not in the captures.
pub const REPLIES_PER_COMMENT: u32 = 16;

const LIKE: &str = "sc:interactiontypevalue:like";

/// A request body: `{operationName, variables, query}`.
pub fn request(operation: &str, query: &str, variables: Value) -> Value {
    json!({ "operationName": operation, "variables": variables, "query": query })
}

pub fn comments_variables(track_urn: &str, creator_urn: &str, after: Option<&str>) -> Value {
    json!({
        "urn": track_urn,
        "creatorUrn": creator_urn,
        "options": {
            "token": null,
            "first": COMMENTS_PER_PAGE,
            "repliesFirst": REPLIES_PER_COMMENT,
            "sort": "NEWEST",
            "repliesSort": "ASCENDING",
            "after": after,
        },
        "anon": false,
    })
}

/// `data` of a GraphQL answer; its `errors`, if any, become the error.
pub fn data<T: DeserializeOwned>(answer: Value) -> Result<T, String> {
    #[derive(Deserialize)]
    struct Envelope {
        #[serde(default)]
        data: Option<Value>,
        #[serde(default)]
        errors: Vec<GraphError>,
    }
    #[derive(Deserialize)]
    struct GraphError {
        #[serde(default)]
        message: String,
    }
    let envelope: Envelope = serde_json::from_value(answer).map_err(|e| format!("graphql: {e}"))?;
    if let Some(error) = envelope.errors.first() {
        return Err(format!("graphql: {}", error.message));
    }
    let data = envelope
        .data
        .filter(|data| !data.is_null())
        .ok_or("graphql: no data")?;
    serde_json::from_value(data).map_err(|e| format!("graphql decode: {e}"))
}

/// A mutation answer field that is either the success type or an error type
/// carrying `errorMessage`.
pub fn expect_type(field: &Value, success: &str) -> Result<(), String> {
    match field.get("__typename").and_then(Value::as_str) {
        Some(name) if name == success => Ok(()),
        _ => Err(field
            .get("errorMessage")
            .and_then(Value::as_str)
            .unwrap_or("unexpected answer")
            .to_string()),
    }
}

/// A comment as the frontend gets it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub urn: String,
    pub body: String,
    pub created_at: String,
    /// Position in the track, ms.
    pub track_time_ms: u64,
    pub likes: u64,
    pub liked: bool,
    pub liked_by_creator: bool,
    pub user: CommentUser,
    pub replies: Vec<Comment>,
    pub replies_total: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentUser {
    pub urn: String,
    /// The numeric id from the urn, for opening the artist page.
    pub id: Option<u64>,
    pub username: String,
    pub permalink: String,
    pub avatar_url: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentPage {
    pub comments: Vec<Comment>,
    /// Pass back as `cursor` for the next page; `None` at the end.
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawComment {
    urn: String,
    body: String,
    created_at: String,
    #[serde(default)]
    track_time: u64,
    #[serde(default)]
    reactions: Option<RawReactions>,
    user: RawUser,
    #[serde(default)]
    replies: Option<RawReplies>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawReactions {
    #[serde(default)]
    creator_reaction: Option<Value>,
    #[serde(default)]
    user_reaction: Option<Value>,
    #[serde(default)]
    reaction_counts: Vec<RawCount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawCount {
    reaction_type_value_urn: String,
    count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawUser {
    urn: String,
    username: String,
    #[serde(default)]
    permalink: String,
    #[serde(default)]
    avatar_url: Option<String>,
    #[serde(default)]
    verified: bool,
}

#[derive(Deserialize)]
struct RawReplies {
    #[serde(default)]
    total: u64,
    #[serde(default)]
    comments: Vec<RawComment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPage {
    track_comments: RawTrackComments,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTrackComments {
    #[serde(default)]
    comments: Vec<RawComment>,
    #[serde(default)]
    page_info: Option<RawPageInfo>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPageInfo {
    #[serde(default)]
    end_cursor: Option<String>,
}

/// The numeric tail of a urn (`soundcloud:users:42` → 42).
pub fn urn_id(urn: &str) -> Option<u64> {
    urn.rsplit(':').next()?.parse().ok()
}

fn reacted(value: &Option<Value>) -> bool {
    value
        .as_ref()
        .is_some_and(|v| !v.is_null() && v != &Value::Bool(false))
}

impl From<RawComment> for Comment {
    fn from(raw: RawComment) -> Self {
        let reactions = raw.reactions.as_ref();
        let replies = raw.replies.unwrap_or(RawReplies {
            total: 0,
            comments: Vec::new(),
        });
        Comment {
            urn: raw.urn,
            body: raw.body,
            created_at: raw.created_at,
            track_time_ms: raw.track_time,
            likes: reactions
                .and_then(|r| {
                    r.reaction_counts
                        .iter()
                        .find(|c| c.reaction_type_value_urn == LIKE)
                })
                .map_or(0, |c| c.count),
            liked: reactions.is_some_and(|r| reacted(&r.user_reaction)),
            liked_by_creator: reactions.is_some_and(|r| reacted(&r.creator_reaction)),
            user: CommentUser {
                id: urn_id(&raw.user.urn),
                urn: raw.user.urn,
                username: raw.user.username,
                permalink: raw.user.permalink,
                avatar_url: raw.user.avatar_url,
                verified: raw.user.verified,
            },
            replies_total: replies.total,
            replies: replies.comments.into_iter().map(Comment::from).collect(),
        }
    }
}

/// `data` of `TrackPageCommentsList`.
pub fn comment_page(answer: Value) -> Result<CommentPage, String> {
    let page: RawPage = data(answer)?;
    let comments: Vec<Comment> = page
        .track_comments
        .comments
        .into_iter()
        .map(Comment::from)
        .collect();
    // The cursor stays set on the last page; an empty page ends the list.
    let cursor = if comments.is_empty() {
        None
    } else {
        page.track_comments
            .page_info
            .and_then(|info| info.end_cursor)
    };
    Ok(CommentPage { comments, cursor })
}

/// `data` of `CreateTrackComment`: the new comment.
pub fn created_comment(answer: Value) -> Result<Comment, String> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        create_track_comment: Payload,
    }
    #[derive(Deserialize)]
    struct Payload {
        comment: RawComment,
    }
    let data: Data = data(answer)?;
    Ok(data.create_track_comment.comment.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn comment_page_decodes_threads_and_reactions() {
        let page =
            comment_page(fixture(include_str!("fixtures/graph_track_comments.json"))).unwrap();
        assert_eq!(page.comments.len(), 3);
        assert_eq!(
            page.cursor.as_deref(),
            Some("soundcloud:comments:2610895281")
        );
        let first = &page.comments[0];
        assert_eq!(first.likes, 2);
        assert!(!first.liked);
        assert_eq!(first.track_time_ms, 52804);
        assert_eq!(first.user.id, Some(1675324283));
        let threaded = &page.comments[1];
        assert_eq!(threaded.replies_total, 17);
        assert_eq!(threaded.replies.len(), 1);
        assert!(threaded.replies[0].liked);
        assert!(threaded.replies[0].replies.is_empty());
    }

    #[test]
    fn an_empty_page_ends_the_list() {
        let answer =
            json!({"data": {"trackComments": {"comments": [], "pageInfo": {"endCursor": "x"}}}});
        assert_eq!(comment_page(answer).unwrap().cursor, None);
    }

    #[test]
    fn graphql_errors_win_over_data() {
        let answer = json!({"data": null, "errors": [{"message": "Not allowed"}]});
        assert_eq!(comment_page(answer).unwrap_err(), "graphql: Not allowed");
    }

    #[test]
    fn created_comment_decodes_without_reactions() {
        let comment =
            created_comment(fixture(include_str!("fixtures/graph_create_comment.json"))).unwrap();
        assert_eq!(comment.body, "a new comment");
        assert_eq!(comment.track_time_ms, 1852);
        assert_eq!(comment.likes, 0);
        assert_eq!(comment.user.id, Some(1670471345));
    }

    #[test]
    fn mutation_types_are_checked() {
        assert!(expect_type(&json!({"__typename": "Repost", "caption": ""}), "Repost").is_ok());
        let failed = json!({"__typename": "TrackRepostFailedError", "errorMessage": "nope"});
        assert_eq!(expect_type(&failed, "Repost").unwrap_err(), "nope");
    }

    #[test]
    fn documents_are_the_captured_ones() {
        for (document, operation) in [
            (TRACK_COMMENTS, "query TrackPageCommentsList("),
            (REPOST_TRACK, "mutation RepostTrack("),
            (UNREPOST_TRACK, "mutation UnrepostTrack("),
            (LIKE_COMMENT, "mutation LikeTrackComment("),
            (UNLIKE_COMMENT, "mutation UnlikeTrackComment("),
            (CREATE_COMMENT, "mutation CreateTrackComment("),
            (DELETE_COMMENT, "mutation DeleteTrackComment("),
        ] {
            assert!(document.starts_with(operation), "{operation}");
        }
    }
}
