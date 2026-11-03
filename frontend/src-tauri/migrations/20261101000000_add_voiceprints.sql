-- Voice memory: each automatically detected speaker keeps its average voice, so that in a later
-- meeting an unnamed speaker can be compared with the people you have already named.
--
-- voiceprint:       unit-length average embedding, little-endian f32 values (256 for the current model)
-- voiceprint_lines: how many transcript lines it was averaged from
--
-- A person's voice is simply the voiceprints of the speaker slots linked to them, so naming,
-- merging, unnaming and deleting meetings keep it consistent with no separate bookkeeping.
-- Stored only on this computer.
ALTER TABLE meeting_speakers ADD COLUMN voiceprint BLOB;
ALTER TABLE meeting_speakers ADD COLUMN voiceprint_lines INTEGER;
