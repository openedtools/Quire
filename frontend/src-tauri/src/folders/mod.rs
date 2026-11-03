//! User-created folders for organizing meetings.
//!
//! A folder is just a label a meeting can point at (`meetings.folder_id`); deleting a
//! folder never deletes meetings. Unrelated to `meetings.folder_path`, the recording's
//! location on disk.

pub mod commands;
pub mod models;
