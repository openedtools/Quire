-- My notes (spec F-NOTE): the person's own notes for a meeting, separate from the AI summary.
CREATE TABLE IF NOT EXISTS meeting_user_notes (
    meeting_id TEXT PRIMARY KEY NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    body_blocks TEXT NOT NULL,
    body_markdown TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
