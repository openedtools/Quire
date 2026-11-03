-- Markdown export for Obsidian (spec F-MD). One settings row, and one row per exported file.
CREATE TABLE IF NOT EXISTS markdown_export_settings (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    enabled INTEGER NOT NULL DEFAULT 0,
    folder TEXT,
    confirmed_at TEXT,
    last_export_at TEXT
);

-- No foreign key to meetings: when a meeting is deleted its row stays until the exporter has
-- removed the file (only if unchanged, F-MD-6), then the exporter removes the row.
CREATE TABLE IF NOT EXISTS markdown_exports (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    rel_path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    exported_at TEXT NOT NULL
);
