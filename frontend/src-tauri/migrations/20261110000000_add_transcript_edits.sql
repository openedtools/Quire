-- Line editing and the transcript check (plan Phase L, PL5): a changed line keeps its first
-- text here, so the change can be undone. NULL means the line was never changed.
ALTER TABLE transcripts ADD COLUMN original_text TEXT;
