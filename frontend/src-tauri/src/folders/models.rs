use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// A user-created folder for organizing meetings.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// A folder plus how many meetings it holds.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct FolderSummary {
    pub id: String,
    pub name: String,
    pub meeting_count: i64,
}

/// One row of the sidebar meeting list. `folder_id` is None for unfiled meetings.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct MeetingListItem {
    pub id: String,
    pub title: String,
    pub folder_id: Option<String>,
    /// Being deleted, waiting for a recording file that is in use
    pub deleting: bool,
}
