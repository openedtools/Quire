-- Phase E: the shared vault, one way (spec F-SYNC-4, sync-format.md section 16). The only change
-- to existing tables is a stable vault ID per meeting, person and folder, filled on first export.
ALTER TABLE meetings ADD COLUMN sync_id TEXT;
ALTER TABLE people ADD COLUMN sync_id TEXT;
ALTER TABLE folders ADD COLUMN sync_id TEXT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_meetings_sync_id ON meetings(sync_id) WHERE sync_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_people_sync_id ON people(sync_id) WHERE sync_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_folders_sync_id ON folders(sync_id) WHERE sync_id IS NOT NULL;

-- What the app last wrote to the vault, per file it owns: never overwrite what changed there.
CREATE TABLE IF NOT EXISTS sync_state (
    sync_id TEXT NOT NULL,
    -- note, sidecar, person, folder
    kind TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    -- For notes and people: the canonical content (sync-format.md section 11), the rev and the
    -- updated time it was written with
    content_key TEXT,
    rev INTEGER,
    updated TEXT,
    exported_at TEXT NOT NULL,
    PRIMARY KEY (kind, rel_path)
);
CREATE INDEX IF NOT EXISTS idx_sync_state_id ON sync_state(sync_id);

-- Written just before each file, cleared once a run is verified: a run that fails or stops
-- part way still knows which files are its own, and with which rev (plan Phase E review).
CREATE TABLE IF NOT EXISTS sync_pending (
    sync_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    rel_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    content_key TEXT,
    rev INTEGER,
    updated TEXT,
    exported_at TEXT NOT NULL,
    PRIMARY KEY (kind, rel_path)
);

CREATE TABLE IF NOT EXISTS vault_settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    folder TEXT,
    -- `dev-` plus 8 hex characters, once per install (sync-format.md section 3)
    device_id TEXT,
    vault_id TEXT,
    disclosed_at TEXT,
    last_export_at TEXT,
    -- Counts only, never titles or paths
    last_report TEXT
);
