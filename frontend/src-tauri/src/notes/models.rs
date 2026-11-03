use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// The person's notes for one meeting. `body_blocks` is the editor's JSON; `body_markdown`
/// is the same text as Markdown, used for the summary prompt and for Ask.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct UserNotes {
    pub meeting_id: String,
    pub body_blocks: String,
    pub body_markdown: String,
    pub updated_at: String,
}
