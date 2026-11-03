-- Phase B: audio retention, transcript-only mode and two-phase meeting delete (spec F-RET, F-TO).
ALTER TABLE meetings ADD COLUMN audio_retention TEXT;
ALTER TABLE meetings ADD COLUMN audio_expires_at TEXT;
ALTER TABLE meetings ADD COLUMN audio_state TEXT NOT NULL DEFAULT 'none';
ALTER TABLE meetings ADD COLUMN audio_deleted_at TEXT;
ALTER TABLE meetings ADD COLUMN finalized_at TEXT;
ALTER TABLE meetings ADD COLUMN mode TEXT NOT NULL DEFAULT 'audio';
ALTER TABLE meetings ADD COLUMN delete_requested_at TEXT;

-- Existing meetings keep their audio: nothing is scheduled for deletion by upgrading (AC-RET-7).
-- A meeting with a recording folder counts as having audio until a sweep finds none.
UPDATE meetings SET audio_retention = 'keep', finalized_at = created_at;
UPDATE meetings SET audio_state = 'present' WHERE folder_path IS NOT NULL AND folder_path <> '';

-- When the sweep last ran, so a clock set backwards can be noticed (F-RET-13).
CREATE TABLE IF NOT EXISTS retention_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    last_sweep_at TEXT,
    latest_clock_seen TEXT
);
