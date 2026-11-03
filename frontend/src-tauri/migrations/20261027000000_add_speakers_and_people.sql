-- Speaker labels and a reusable people directory.
--
-- people:            global directory of named people (created the first time a name is typed)
-- meeting_speakers:  speaker slots for one meeting ("S1", "S2", ...). Each slot may be linked to a person.
--                    Transcript lines reference a slot through transcripts.speaker (the column added in
--                    20251110000001), so renaming a slot's person updates every line at once.

CREATE TABLE IF NOT EXISTS people (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- A person is unique by name, ignoring case ("maj smith" == "Maj Smith")
CREATE UNIQUE INDEX IF NOT EXISTS idx_people_name ON people(name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS meeting_speakers (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL,
    speaker_key TEXT NOT NULL,
    person_id TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (person_id) REFERENCES people(id) ON DELETE SET NULL,
    UNIQUE (meeting_id, speaker_key)
);

CREATE INDEX IF NOT EXISTS idx_meeting_speakers_meeting ON meeting_speakers(meeting_id);
CREATE INDEX IF NOT EXISTS idx_meeting_speakers_person ON meeting_speakers(person_id);

-- Fast "all lines by this speaker in this meeting" lookups
CREATE INDEX IF NOT EXISTS idx_transcripts_meeting_speaker ON transcripts(meeting_id, speaker);
