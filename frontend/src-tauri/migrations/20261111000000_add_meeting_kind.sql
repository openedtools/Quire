-- Thought mode (plan Phase N, PN4): "thoughts" for a recording of one person's own thoughts,
-- NULL for an ordinary meeting.
ALTER TABLE meetings ADD COLUMN kind TEXT;
