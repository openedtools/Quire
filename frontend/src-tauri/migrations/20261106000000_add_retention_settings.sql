-- Retention settings (spec F-RET-1, F-RET-14). A new install keeps audio for 7 days; an install
-- that already has meetings starts at Keep, so upgrading deletes nothing (decision D7, D3a).
ALTER TABLE retention_state ADD COLUMN default_retention TEXT;
ALTER TABLE retention_state ADD COLUMN detect_on_save INTEGER NOT NULL DEFAULT 0;
INSERT OR IGNORE INTO retention_state (id) VALUES (1);
UPDATE retention_state
SET default_retention = CASE WHEN EXISTS (SELECT 1 FROM meetings) THEN 'keep' ELSE 'd7' END
WHERE id = 1 AND default_retention IS NULL;
