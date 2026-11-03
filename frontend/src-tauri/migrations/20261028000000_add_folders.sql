-- User-created folders for organizing meetings.
--
-- Note: meetings.folder_path (existing) is the on-disk recording folder. This is unrelated:
-- meetings.folder_id points at one of these organizational folders, NULL meaning "unfiled".
-- Deleting a folder never deletes its meetings; they fall back to unfiled.

CREATE TABLE IF NOT EXISTS folders (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- A folder is unique by name, ignoring case
CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_name ON folders(name COLLATE NOCASE);

ALTER TABLE meetings ADD COLUMN folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_meetings_folder_id ON meetings(folder_id);
