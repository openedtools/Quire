use crate::people::models::{MeetingSpeaker, Person, PersonDetail, PersonLine, PersonMeeting, PersonMeetingHit, PersonSummary, SpeakerSuggestion};
use chrono::Utc;
use sqlx::{Error as SqlxError, SqliteConnection, SqlitePool};
use uuid::Uuid;

const MAX_NAME_LEN: usize = 120;
const MAX_NOTES_LEN: usize = 20_000;
/// A meeting never has anywhere near this many speakers; larger numbers are corrupt or hostile input.
const MAX_DETECTED_SPEAKERS: usize = 1_000;
/// Upper bound on lines scanned for one person; keeps a text search over a very large history quick.
const MAX_LINES_SCANNED: i64 = 50_000;
/// Voices at least this alike are offered as a name suggestion. Measured: the same person across
/// separate recordings scored 0.66-0.97 (even through reverb and a phone-band codec); different
/// people mostly scored below 0.45, and only deliberately similar synthetic voices reached 0.57.
pub const MIN_SUGGESTION_SIMILARITY: f32 = 0.55;
/// At or above this a match is called "likely" rather than "possible".
pub const STRONG_SUGGESTION_SIMILARITY: f32 = 0.70;
const MAX_SUGGESTIONS_PER_SPEAKER: usize = 3;

/// An embedding as stored: little-endian f32 values.
fn encode_voice(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

fn decode_voice(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

/// Cosine similarity, or `None` when the embeddings cannot be compared (different models).
fn voice_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || a.len() != b.len() {
        return None;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norms = (a.iter().map(|x| x * x).sum::<f32>() * b.iter().map(|x| x * x).sum::<f32>()).sqrt();
    (norms > 0.0).then(|| dot / norms)
}

/// Weighted average of two voices (weights = number of lines each came from), unit length.
fn blend_voices(a: (&[f32], i64), b: (&[f32], i64)) -> Vec<f32> {
    if a.0.len() != b.0.len() {
        return if a.1 >= b.1 { a.0.to_vec() } else { b.0.to_vec() };
    }
    let (wa, wb) = (a.1.max(1) as f32, b.1.max(1) as f32);
    let mixed: Vec<f32> = a.0.iter().zip(b.0).map(|(x, y)| x * wa + y * wb).collect();
    let norm = mixed.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 { mixed.iter().map(|x| x / norm).collect() } else { mixed }
}

#[derive(Debug, thiserror::Error)]
pub enum PeopleError {
    /// A problem with what the caller asked for; the message is safe to show in the UI.
    #[error("{0}")]
    Invalid(String),
    #[error("Database error: {0}")]
    Db(#[from] SqlxError),
}

fn invalid<T>(msg: impl Into<String>) -> Result<T, PeopleError> {
    Err(PeopleError::Invalid(msg.into()))
}

/// Characters with no visible glyph that can be used to forge or mislead: control characters,
/// zero-width spaces/joiners, soft hyphens, byte-order marks, and the bidirectional overrides and
/// isolates that make text display in a different order than it is stored.
fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{00AD}' | '\u{061C}' | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
        )
}

/// One line of text: whitespace collapsed to single spaces and invisible characters removed, so a
/// name cannot hide behind zero-width characters or reorder itself with text-direction overrides.
pub(crate) fn clean_single_line(text: &str) -> String {
    text.split_whitespace()
        .map(|word| word.chars().filter(|c| !is_invisible(*c)).collect::<String>())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Free text: line breaks and tabs are kept, other invisible characters are dropped.
fn clean_notes(text: &str) -> String {
    text.chars().filter(|c| matches!(c, '\n' | '\t') || !is_invisible(*c)).collect()
}

/// Collapse runs of whitespace, drop invisible characters, and reject empty or oversized names.
fn normalize_name(name: &str) -> Result<String, PeopleError> {
    let cleaned = clean_single_line(name);
    if cleaned.is_empty() {
        return invalid("Name cannot be empty");
    }
    if cleaned.chars().count() > MAX_NAME_LEN {
        return invalid(format!("Name is too long (max {} characters)", MAX_NAME_LEN));
    }
    Ok(cleaned)
}

/// Speaker keys are "S1", "S2", ... with no leading zeros, so one speaker has one spelling.
fn speaker_number(key: &str) -> Option<u32> {
    let n: u32 = key.strip_prefix('S')?.parse().ok()?;
    (n > 0 && format!("S{}", n) == key).then_some(n)
}

fn check_speaker_key(key: &str) -> Result<(), PeopleError> {
    match speaker_number(key) {
        Some(_) => Ok(()),
        None => invalid(format!("Invalid speaker key: {}", key)),
    }
}

/// Escape LIKE wildcards so a typed "50%" or "a_b" matches literally.
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

pub struct PeopleRepository;

impl PeopleRepository {
    // ---------------------------------------------------------------- people

    /// All people with meeting counts, optionally filtered by a name fragment.
    pub async fn list_people(
        pool: &SqlitePool,
        query: Option<&str>,
    ) -> Result<Vec<PersonSummary>, PeopleError> {
        let pattern = query
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(|q| format!("%{}%", escape_like(q)));

        let people = sqlx::query_as::<_, PersonSummary>(
            r#"SELECT p.id, p.name,
                      COUNT(DISTINCT ms.meeting_id) AS meeting_count,
                      MAX(m.created_at) AS last_meeting_at
               FROM people p
               LEFT JOIN meeting_speakers ms ON ms.person_id = p.id
               LEFT JOIN meetings m ON m.id = ms.meeting_id
               WHERE ?1 IS NULL OR p.name LIKE ?1 ESCAPE '\'
               GROUP BY p.id, p.name
               ORDER BY p.name COLLATE NOCASE"#,
        )
        .bind(pattern)
        .fetch_all(pool)
        .await?;
        Ok(people)
    }

    async fn find_person_by_name(
        conn: &mut SqliteConnection,
        name: &str,
    ) -> Result<Option<Person>, SqlxError> {
        sqlx::query_as::<_, Person>("SELECT * FROM people WHERE name = ? COLLATE NOCASE")
            .bind(name)
            .fetch_optional(conn)
            .await
    }

    async fn find_person_by_id(
        conn: &mut SqliteConnection,
        id: &str,
    ) -> Result<Option<Person>, SqlxError> {
        sqlx::query_as::<_, Person>("SELECT * FROM people WHERE id = ?")
            .bind(id)
            .fetch_optional(conn)
            .await
    }

    /// Return the person with this name (case-insensitive), creating them if new.
    /// This is what makes a name typed once reappear in the dropdown from then on.
    pub async fn get_or_create_person(pool: &SqlitePool, name: &str) -> Result<Person, PeopleError> {
        let name = normalize_name(name)?;
        let mut conn = pool.acquire().await?;

        if let Some(existing) = Self::find_person_by_name(&mut conn, &name).await? {
            return Ok(existing);
        }

        let now = Utc::now().to_rfc3339();
        let person = Person {
            id: format!("person-{}", Uuid::new_v4()),
            name: name.clone(),
            created_at: now.clone(),
            updated_at: now,
            notes: None,
        };
        let inserted = sqlx::query(
            "INSERT INTO people (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&person.id)
        .bind(&person.name)
        .bind(&person.created_at)
        .bind(&person.updated_at)
        .execute(&mut *conn)
        .await;

        match inserted {
            Ok(_) => Ok(person),
            // Lost a race against the unique name index: hand back whoever won.
            Err(e) => match Self::find_person_by_name(&mut conn, &name).await? {
                Some(winner) => Ok(winner),
                None => Err(e.into()),
            },
        }
    }

    pub async fn rename_person(
        pool: &SqlitePool,
        person_id: &str,
        new_name: &str,
    ) -> Result<Person, PeopleError> {
        let name = normalize_name(new_name)?;
        let mut conn = pool.acquire().await?;

        if let Some(clash) = Self::find_person_by_name(&mut conn, &name).await? {
            if clash.id != person_id {
                return invalid(format!("A person named \"{}\" already exists", clash.name));
            }
        }

        let updated = sqlx::query("UPDATE people SET name = ?, updated_at = ? WHERE id = ?")
            .bind(&name)
            .bind(Utc::now().to_rfc3339())
            .bind(person_id)
            .execute(&mut *conn)
            .await?;
        if updated.rows_affected() == 0 {
            return invalid("Person not found");
        }

        match Self::find_person_by_id(&mut conn, person_id).await? {
            Some(person) => Ok(person),
            None => invalid("Person not found"),
        }
    }

    /// Remove a person. Their speaker slots stay in each meeting, back to "Speaker N".
    pub async fn delete_person(pool: &SqlitePool, person_id: &str) -> Result<bool, PeopleError> {
        let mut tx = pool.begin().await?;
        // Their speaker slots stay as "Speaker N", without the voice that identified them
        sqlx::query(
            "UPDATE meeting_speakers SET person_id = NULL, voiceprint = NULL, voiceprint_lines = NULL WHERE person_id = ?",
        )
        .bind(person_id)
        .execute(&mut *tx)
        .await?;
        let deleted = sqlx::query("DELETE FROM people WHERE id = ?")
            .bind(person_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(deleted.rows_affected() > 0)
    }

    pub async fn get_person(pool: &SqlitePool, person_id: &str) -> Result<Option<Person>, PeopleError> {
        let mut conn = pool.acquire().await?;
        Ok(Self::find_person_by_id(&mut conn, person_id).await?)
    }

    /// Save notes for a person. Blank notes clear the field.
    pub async fn update_person_notes(
        pool: &SqlitePool,
        person_id: &str,
        notes: Option<&str>,
    ) -> Result<Person, PeopleError> {
        let cleaned_notes = notes.map(clean_notes);
        let notes = cleaned_notes.as_deref().map(str::trim).filter(|n| !n.is_empty());
        if notes.map_or(false, |n| n.chars().count() > MAX_NOTES_LEN) {
            return invalid(format!("Notes are too long (max {} characters)", MAX_NOTES_LEN));
        }

        let mut conn = pool.acquire().await?;
        let updated = sqlx::query("UPDATE people SET notes = ?, updated_at = ? WHERE id = ?")
            .bind(notes)
            .bind(Utc::now().to_rfc3339())
            .bind(person_id)
            .execute(&mut *conn)
            .await?;
        if updated.rows_affected() == 0 {
            return invalid("Person not found");
        }
        match Self::find_person_by_id(&mut conn, person_id).await? {
            Some(person) => Ok(person),
            None => invalid("Person not found"),
        }
    }

    /// Every meeting this person spoke in (newest first) with how much they said.
    pub async fn list_person_meetings(
        pool: &SqlitePool,
        person_id: &str,
    ) -> Result<Vec<PersonMeeting>, PeopleError> {
        let meetings = sqlx::query_as::<_, PersonMeeting>(
            r#"SELECT m.id AS meeting_id, m.title, m.created_at, m.folder_id,
                      COUNT(t.id) AS line_count,
                      MIN(t.audio_start_time) AS first_line_start
               FROM meeting_speakers ms
               JOIN meetings m ON m.id = ms.meeting_id
               LEFT JOIN transcripts t ON t.meeting_id = ms.meeting_id AND t.speaker = ms.speaker_key
               WHERE ms.person_id = ?
               GROUP BY m.id, m.title, m.created_at, m.folder_id
               ORDER BY m.created_at DESC"#,
        )
        .bind(person_id)
        .fetch_all(pool)
        .await?;
        Ok(meetings)
    }

    pub async fn get_person_detail(
        pool: &SqlitePool,
        person_id: &str,
    ) -> Result<PersonDetail, PeopleError> {
        let Some(person) = Self::get_person(pool, person_id).await? else {
            return invalid("Person not found");
        };
        let meetings = Self::list_person_meetings(pool, person_id).await?;
        let (voice_meetings,): (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT meeting_id) FROM meeting_speakers WHERE person_id = ? AND voiceprint IS NOT NULL",
        )
        .bind(person_id)
        .fetch_one(pool)
        .await?;
        Ok(PersonDetail { person, meetings, voice_meetings })
    }

    /// What this person said across all meetings, newest meeting first, optionally filtered by text.
    ///
    /// Filtering happens here rather than in SQL because SQLite only folds ASCII case, which would
    /// miss "ПРИВІТ" for a query of "привіт".
    pub async fn list_person_lines(
        pool: &SqlitePool,
        person_id: &str,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<PersonLine>, PeopleError> {
        let lines = sqlx::query_as::<_, PersonLine>(
            r#"SELECT t.id AS transcript_id, t.meeting_id,
                      m.title AS meeting_title, m.created_at AS meeting_created_at,
                      t.transcript AS text, t.audio_start_time
               FROM transcripts t
               JOIN meeting_speakers ms ON ms.meeting_id = t.meeting_id AND ms.speaker_key = t.speaker
               JOIN meetings m ON m.id = t.meeting_id
               WHERE ms.person_id = ?
               ORDER BY m.created_at DESC, t.audio_start_time ASC
               LIMIT ?"#,
        )
        .bind(person_id)
        .bind(MAX_LINES_SCANNED)
        .fetch_all(pool)
        .await?;

        let needle = query.map(str::trim).filter(|q| !q.is_empty()).map(str::to_lowercase);
        Ok(lines
            .into_iter()
            .filter(|line| needle.as_ref().map_or(true, |n| line.text.to_lowercase().contains(n)))
            .take(limit)
            .collect())
    }

    /// Meetings where someone whose name contains `query` spoke. Powers "search by person".
    pub async fn search_meetings_by_person(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<PersonMeetingHit>, PeopleError> {
        let needle = query.trim().to_lowercase();
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        let people: Vec<(String, String)> = sqlx::query_as("SELECT id, name FROM people")
            .fetch_all(pool)
            .await?;

        let mut hits = Vec::new();
        for (id, name) in people.into_iter().filter(|(_, n)| n.to_lowercase().contains(&needle)).take(20) {
            let rows: Vec<(String, String)> = sqlx::query_as(
                r#"SELECT m.id, m.title
                   FROM meeting_speakers ms JOIN meetings m ON m.id = ms.meeting_id
                   WHERE ms.person_id = ?
                   GROUP BY m.id, m.title
                   ORDER BY m.created_at DESC"#,
            )
            .bind(&id)
            .fetch_all(pool)
            .await?;
            hits.extend(rows.into_iter().map(|(meeting_id, title)| PersonMeetingHit {
                meeting_id,
                title,
                person_name: name.clone(),
            }));
        }
        Ok(hits)
    }

    /// Fold a duplicate person into another ("Prof Smith" / "Professor Smith"): all their speaker slots
    /// move over, notes are combined, and the duplicate is removed. Returns the surviving person.
    pub async fn merge_people(
        pool: &SqlitePool,
        from_id: &str,
        into_id: &str,
    ) -> Result<Person, PeopleError> {
        if from_id == into_id {
            return invalid("Choose two different people to merge");
        }

        let mut tx = pool.begin().await?;
        let (Some(from), Some(into)) = (
            Self::find_person_by_id(&mut tx, from_id).await?,
            Self::find_person_by_id(&mut tx, into_id).await?,
        ) else {
            return invalid("Person not found");
        };

        sqlx::query("UPDATE meeting_speakers SET person_id = ? WHERE person_id = ?")
            .bind(into_id)
            .bind(from_id)
            .execute(&mut *tx)
            .await?;

        let notes = match (into.notes.as_deref(), from.notes.as_deref()) {
            (Some(a), Some(b)) if a != b => Some(format!("{}\n\n(From {}) {}", a, from.name, b)),
            (Some(a), _) => Some(a.to_string()),
            (None, b) => b.map(str::to_string),
        };
        sqlx::query("UPDATE people SET notes = ?, updated_at = ? WHERE id = ?")
            .bind(notes)
            .bind(Utc::now().to_rfc3339())
            .bind(into_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM people WHERE id = ?")
            .bind(from_id)
            .execute(&mut *tx)
            .await?;

        let merged = Self::find_person_by_id(&mut tx, into_id).await?;
        tx.commit().await?;
        match merged {
            Some(person) => Ok(person),
            None => invalid("Person not found"),
        }
    }

    // -------------------------------------------------------------- speakers

    async fn fetch_speakers(
        conn: &mut SqliteConnection,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, SqlxError> {
        sqlx::query_as::<_, MeetingSpeaker>(
            r#"SELECT s.speaker_key, s.person_id, p.name AS person_name,
                      (SELECT COUNT(*) FROM transcripts t
                        WHERE t.meeting_id = s.meeting_id AND t.speaker = s.speaker_key) AS segment_count
               FROM meeting_speakers s
               LEFT JOIN people p ON p.id = s.person_id
               WHERE s.meeting_id = ?
               ORDER BY CAST(SUBSTR(s.speaker_key, 2) AS INTEGER)"#,
        )
        .bind(meeting_id)
        .fetch_all(conn)
        .await
    }

    pub async fn get_meeting_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        let mut conn = pool.acquire().await?;
        Ok(Self::fetch_speakers(&mut conn, meeting_id).await?)
    }

    async fn require_meeting(conn: &mut SqliteConnection, meeting_id: &str) -> Result<(), PeopleError> {
        let found: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM meetings WHERE id = ?")
            .bind(meeting_id)
            .fetch_optional(conn)
            .await?;
        match found {
            Some(_) => Ok(()),
            None => invalid("Meeting not found"),
        }
    }

    /// Create the speaker slot if it does not exist yet (no-op otherwise).
    async fn ensure_speaker(
        conn: &mut SqliteConnection,
        meeting_id: &str,
        speaker_key: &str,
    ) -> Result<(), PeopleError> {
        check_speaker_key(speaker_key)?;
        sqlx::query(
            "INSERT OR IGNORE INTO meeting_speakers (id, meeting_id, speaker_key, person_id, created_at)
             VALUES (?, ?, ?, NULL, ?)",
        )
        .bind(format!("speaker-{}", Uuid::new_v4()))
        .bind(meeting_id)
        .bind(speaker_key)
        .bind(Utc::now().to_rfc3339())
        .execute(conn)
        .await?;
        Ok(())
    }

    /// Add the next free speaker slot ("S3" after "S1", "S2") and return the updated list.
    pub async fn add_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        let mut tx = pool.begin().await?;
        Self::require_meeting(&mut tx, meeting_id).await?;

        let keys: Vec<(String,)> =
            sqlx::query_as("SELECT speaker_key FROM meeting_speakers WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&mut *tx)
                .await?;
        let next = keys
            .iter()
            .filter_map(|(k,)| speaker_number(k))
            .max()
            .unwrap_or(0)
            + 1;
        Self::ensure_speaker(&mut tx, meeting_id, &format!("S{}", next)).await?;

        let speakers = Self::fetch_speakers(&mut tx, meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }

    /// Link a speaker slot to a person, or clear the link with `None` (back to "Speaker N").
    /// Every line spoken by this slot picks up the change because lines only store the slot key.
    pub async fn set_speaker_person(
        pool: &SqlitePool,
        meeting_id: &str,
        speaker_key: &str,
        person_id: Option<&str>,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        let mut tx = pool.begin().await?;
        Self::require_meeting(&mut tx, meeting_id).await?;
        check_speaker_key(speaker_key)?;

        if let Some(pid) = person_id {
            if Self::find_person_by_id(&mut tx, pid).await?.is_none() {
                return invalid("Person not found");
            }
        }

        Self::ensure_speaker(&mut tx, meeting_id, speaker_key).await?;
        sqlx::query("UPDATE meeting_speakers SET person_id = ? WHERE meeting_id = ? AND speaker_key = ?")
            .bind(person_id)
            .bind(meeting_id)
            .bind(speaker_key)
            .execute(&mut *tx)
            .await?;

        let speakers = Self::fetch_speakers(&mut tx, meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }

    /// Reassign one transcript line to a speaker slot, or `None` to leave it unlabeled.
    /// Returns the meeting's refreshed speaker list (line counts change).
    pub async fn set_transcript_speaker(
        pool: &SqlitePool,
        transcript_id: &str,
        speaker_key: Option<&str>,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        let mut tx = pool.begin().await?;

        let meeting: Option<(String,)> =
            sqlx::query_as("SELECT meeting_id FROM transcripts WHERE id = ?")
                .bind(transcript_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((meeting_id,)) = meeting else {
            return invalid("Transcript line not found");
        };

        if let Some(key) = speaker_key {
            Self::ensure_speaker(&mut tx, &meeting_id, key).await?;
        }
        sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ?")
            .bind(speaker_key)
            .bind(transcript_id)
            .execute(&mut *tx)
            .await?;

        let speakers = Self::fetch_speakers(&mut tx, &meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }

    /// Fold `from_key` into `into_key`: all its lines move over and the slot is removed.
    /// Used when automatic detection split one real person into two speakers.
    pub async fn merge_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
        from_key: &str,
        into_key: &str,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        check_speaker_key(from_key)?;
        check_speaker_key(into_key)?;
        if from_key == into_key {
            return invalid("Choose two different speakers to merge");
        }

        let mut tx = pool.begin().await?;
        Self::require_meeting(&mut tx, meeting_id).await?;

        let from_person: Option<(Option<String>,)> = sqlx::query_as(
            "SELECT person_id FROM meeting_speakers WHERE meeting_id = ? AND speaker_key = ?",
        )
        .bind(meeting_id)
        .bind(from_key)
        .fetch_optional(&mut *tx)
        .await?;

        Self::ensure_speaker(&mut tx, meeting_id, into_key).await?;
        sqlx::query("UPDATE transcripts SET speaker = ? WHERE meeting_id = ? AND speaker = ?")
            .bind(into_key)
            .bind(meeting_id)
            .bind(from_key)
            .execute(&mut *tx)
            .await?;

        // Two halves of one person: their voice is the blend of both
        let voices: Vec<(String, Option<Vec<u8>>, Option<i64>)> = sqlx::query_as(
            "SELECT speaker_key, voiceprint, voiceprint_lines FROM meeting_speakers WHERE meeting_id = ? AND speaker_key IN (?, ?)",
        )
        .bind(meeting_id)
        .bind(from_key)
        .bind(into_key)
        .fetch_all(&mut *tx)
        .await?;
        let voice_of = |key: &str| {
            voices.iter().find(|(k, _, _)| k == key).and_then(|(_, bytes, lines)| {
                bytes.as_ref().map(|b| (decode_voice(b), lines.unwrap_or(1)))
            })
        };
        let merged_voice = match (voice_of(into_key), voice_of(from_key)) {
            (Some(a), Some(b)) => Some((blend_voices((&a.0, a.1), (&b.0, b.1)), a.1 + b.1)),
            (None, Some(b)) => Some(b),
            (a, None) => a,
        };
        sqlx::query("UPDATE meeting_speakers SET voiceprint = ?, voiceprint_lines = ? WHERE meeting_id = ? AND speaker_key = ?")
            .bind(merged_voice.as_ref().map(|(v, _)| encode_voice(v)))
            .bind(merged_voice.as_ref().map(|(_, n)| *n))
            .bind(meeting_id)
            .bind(into_key)
            .execute(&mut *tx)
            .await?;

        // Keep the name if only the slot being folded away had one.
        if let Some((Some(person_id),)) = from_person {
            sqlx::query(
                "UPDATE meeting_speakers SET person_id = ?
                 WHERE meeting_id = ? AND speaker_key = ? AND person_id IS NULL",
            )
            .bind(person_id)
            .bind(meeting_id)
            .bind(into_key)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND speaker_key = ?")
            .bind(meeting_id)
            .bind(from_key)
            .execute(&mut *tx)
            .await?;

        let speakers = Self::fetch_speakers(&mut tx, meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }

    /// Replace a meeting's speaker labels with the result of automatic detection.
    ///
    /// `assignments` pairs a transcript line id with its detected speaker (0-based, `None` to
    /// leave the line unlabelled). Speakers become "S1", "S2", ... in that order. Names the user
    /// already gave are not lost: a new speaker inherits a person when at least half of its lines
    /// were previously that person's, and one person is never given to two speakers.
    ///
    /// `voices[n]` is speaker `n`'s average voice. It is kept on the speaker so later meetings can
    /// suggest who it is; pass an empty slice to remember nothing.
    pub async fn apply_detected_speakers(
        pool: &SqlitePool,
        meeting_id: &str,
        assignments: &[(String, Option<usize>)],
        voices: &[Option<(Vec<f32>, usize)>],
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        use std::collections::{HashMap, HashSet};

        let mut tx = pool.begin().await?;
        Self::require_meeting(&mut tx, meeting_id).await?;

        let old_label_of_line: HashMap<String, Option<String>> =
            sqlx::query_as::<_, (String, Option<String>)>("SELECT id, speaker FROM transcripts WHERE meeting_id = ?")
                .bind(meeting_id)
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .collect();
        let person_of_old_speaker: HashMap<String, String> = sqlx::query_as::<_, (String, String)>(
            "SELECT speaker_key, person_id FROM meeting_speakers WHERE meeting_id = ? AND person_id IS NOT NULL",
        )
        .bind(meeting_id)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .collect();

        // Results computed from an older transcript (it was re-transcribed or lines were deleted while
        // detection ran) must not be applied: that would wipe the current speakers and their names.
        let current: Vec<&(String, Option<usize>)> =
            assignments.iter().filter(|(id, _)| old_label_of_line.contains_key(id)).collect();
        if !assignments.is_empty() && current.is_empty() {
            return invalid("The transcript changed while speakers were being detected. Please run detection again.");
        }
        let assignments = current;

        // How many of each new speaker's lines belonged to each named old speaker
        let mut lines_of: HashMap<usize, usize> = HashMap::new();
        let mut overlap: HashMap<(usize, String), usize> = HashMap::new();
        for (line_id, label) in assignments.iter().map(|a| (&a.0, &a.1)) {
            let Some(label) = label else { continue };
            if *label >= MAX_DETECTED_SPEAKERS {
                return invalid("Detected speaker number is out of range");
            }
            *lines_of.entry(*label).or_default() += 1;
            if let Some(Some(old_key)) = old_label_of_line.get(line_id) {
                if person_of_old_speaker.contains_key(old_key) {
                    *overlap.entry((*label, old_key.clone())).or_default() += 1;
                }
            }
        }
        let mut candidates: Vec<(usize, String, usize)> = overlap
            .into_iter()
            .filter(|((label, _), count)| count * 2 >= lines_of[label])
            .map(|((label, old_key), count)| (label, old_key, count))
            .collect();
        candidates.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));
        let mut carried: HashMap<usize, String> = HashMap::new();
        let mut people_used: HashSet<String> = HashSet::new();
        for (label, old_key, _) in candidates {
            if carried.contains_key(&label) {
                continue;
            }
            let person = &person_of_old_speaker[&old_key];
            if people_used.insert(person.clone()) {
                carried.insert(label, person.clone());
            }
        }

        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ?").bind(meeting_id).execute(&mut *tx).await?;
        sqlx::query("UPDATE transcripts SET speaker = NULL WHERE meeting_id = ?").bind(meeting_id).execute(&mut *tx).await?;

        let mut labels: Vec<usize> = lines_of.keys().copied().collect();
        labels.sort_unstable();
        for label in labels {
            let voice = voices.get(label).and_then(|v| v.as_ref());
            sqlx::query(
                "INSERT INTO meeting_speakers (id, meeting_id, speaker_key, person_id, created_at, voiceprint, voiceprint_lines)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(format!("speaker-{}", Uuid::new_v4()))
            .bind(meeting_id)
            .bind(format!("S{}", label + 1))
            .bind(carried.get(&label))
            .bind(Utc::now().to_rfc3339())
            .bind(voice.map(|(embedding, _)| encode_voice(embedding)))
            .bind(voice.map(|(_, lines)| *lines as i64))
            .execute(&mut *tx)
            .await?;
        }
        for (line_id, label) in assignments.iter().map(|a| (&a.0, &a.1)) {
            if let Some(label) = label {
                sqlx::query("UPDATE transcripts SET speaker = ? WHERE id = ? AND meeting_id = ?")
                    .bind(format!("S{}", label + 1))
                    .bind(line_id)
                    .bind(meeting_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        let speakers = Self::fetch_speakers(&mut tx, meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }

    /// Guess who the unnamed speakers of a meeting are by comparing their voices with the voices
    /// remembered for people in other meetings. Suggestions only: nothing is assigned.
    ///
    /// For each speaker up to three people at or above `MIN_SUGGESTION_SIMILARITY` are returned,
    /// best first. A person's score is their closest remembered voice (it varies with the room and
    /// microphone, so the nearest sample beats an average). Each person is the first suggestion
    /// (`is_best`) for at most one speaker: the one they match most closely.
    pub async fn suggest_people_for_meeting(
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<Vec<SpeakerSuggestion>, PeopleError> {
        use std::collections::{BTreeMap, HashSet};

        let slots: Vec<(String, Vec<u8>)> = sqlx::query_as(
            "SELECT speaker_key, voiceprint FROM meeting_speakers
             WHERE meeting_id = ? AND person_id IS NULL AND voiceprint IS NOT NULL",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        if slots.is_empty() {
            return Ok(Vec::new());
        }

        // person -> (name, remembered voices, meetings they come from)
        let rows: Vec<(String, String, Vec<u8>, String)> = sqlx::query_as(
            "SELECT ms.person_id, p.name, ms.voiceprint, ms.meeting_id
             FROM meeting_speakers ms JOIN people p ON p.id = ms.person_id
             WHERE ms.voiceprint IS NOT NULL AND ms.meeting_id != ?",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;
        let mut known: BTreeMap<String, (String, Vec<Vec<f32>>, HashSet<String>)> = BTreeMap::new();
        for (person_id, name, bytes, from_meeting) in rows {
            let entry = known.entry(person_id).or_insert_with(|| (name, Vec::new(), HashSet::new()));
            entry.1.push(decode_voice(&bytes));
            entry.2.insert(from_meeting);
        }

        // Every (speaker, person) pair that is alike enough
        let mut pairs: Vec<(String, String, f32)> = Vec::new();
        for (slot_key, bytes) in &slots {
            let voice = decode_voice(bytes);
            for (person_id, (_, voices, _)) in &known {
                let best = voices.iter().filter_map(|v| voice_similarity(&voice, v)).fold(f32::MIN, f32::max);
                if best >= MIN_SUGGESTION_SIMILARITY {
                    pairs.push((slot_key.clone(), person_id.clone(), best));
                }
            }
        }
        pairs.sort_by(|a, b| b.2.total_cmp(&a.2).then(a.0.cmp(&b.0)).then(a.1.cmp(&b.1)));

        // Strongest matches first: each speaker and each person is "best" at most once
        let (mut best_taken, mut people_taken): (HashSet<String>, HashSet<String>) = (HashSet::new(), HashSet::new());
        let mut suggestions: Vec<SpeakerSuggestion> = Vec::new();
        let mut per_speaker: BTreeMap<String, usize> = BTreeMap::new();
        for (slot_key, person_id, similarity) in pairs {
            let count = per_speaker.entry(slot_key.clone()).or_default();
            if *count >= MAX_SUGGESTIONS_PER_SPEAKER {
                continue;
            }
            *count += 1;
            let is_best = !best_taken.contains(&slot_key) && !people_taken.contains(&person_id);
            if is_best {
                best_taken.insert(slot_key.clone());
                people_taken.insert(person_id.clone());
            }
            let (name, _, meetings) = &known[&person_id];
            suggestions.push(SpeakerSuggestion {
                speaker_key: slot_key,
                person_id,
                person_name: name.clone(),
                similarity,
                strength: if similarity >= STRONG_SUGGESTION_SIMILARITY { "likely" } else { "possible" }.to_string(),
                meetings: meetings.len() as i64,
                is_best,
            });
        }
        // Stable order for the UI: by speaker number, best match first
        suggestions.sort_by(|a, b| {
            speaker_number(&a.speaker_key).cmp(&speaker_number(&b.speaker_key))
                .then(b.is_best.cmp(&a.is_best))
                .then(b.similarity.total_cmp(&a.similarity))
        });
        Ok(suggestions)
    }

    /// Erase the remembered voice of a person (their name and meetings stay). Returns how many
    /// stored voices were cleared.
    pub async fn forget_person_voice(pool: &SqlitePool, person_id: &str) -> Result<u64, PeopleError> {
        let cleared = sqlx::query(
            "UPDATE meeting_speakers SET voiceprint = NULL, voiceprint_lines = NULL
             WHERE person_id = ? AND voiceprint IS NOT NULL",
        )
        .bind(person_id)
        .execute(pool)
        .await?;
        Ok(cleared.rows_affected())
    }

    /// Remove a speaker slot; its lines become unlabeled.
    pub async fn delete_speaker(
        pool: &SqlitePool,
        meeting_id: &str,
        speaker_key: &str,
    ) -> Result<Vec<MeetingSpeaker>, PeopleError> {
        check_speaker_key(speaker_key)?;
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE transcripts SET speaker = NULL WHERE meeting_id = ? AND speaker = ?")
            .bind(meeting_id)
            .bind(speaker_key)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM meeting_speakers WHERE meeting_id = ? AND speaker_key = ?")
            .bind(meeting_id)
            .bind(speaker_key)
            .execute(&mut *tx)
            .await?;
        let speakers = Self::fetch_speakers(&mut tx, meeting_id).await?;
        tx.commit().await?;
        Ok(speakers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// In-memory database with the real migration chain applied. A single connection
    /// keeps every query on the same in-memory database.
    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn add_meeting(pool: &SqlitePool, id: &str) {
        let now = Utc::now();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(format!("Meeting {}", id))
            .bind(now)
            .bind(now)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn add_line(pool: &SqlitePool, id: &str, meeting_id: &str, start: f64, speaker: Option<&str>) {
        sqlx::query(
            "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, speaker)
             VALUES (?, ?, 'hello', '10:00:00', ?, ?)",
        )
        .bind(id)
        .bind(meeting_id)
        .bind(start)
        .bind(speaker)
        .execute(pool)
        .await
        .unwrap();
    }

    #[test]
    fn speaker_keys_have_one_spelling() {
        assert_eq!(speaker_number("S1"), Some(1));
        assert_eq!(speaker_number("S12"), Some(12));
        for bad in ["S0", "S01", "s1", "S", "1", "S-1", "S1 ", "Speaker 1", ""] {
            assert_eq!(speaker_number(bad), None, "{bad:?} should be rejected");
        }
    }

    #[test]
    fn names_are_cleaned_and_validated() {
        assert_eq!(normalize_name("  Prof   Ana  Lopez ").unwrap(), "Prof Ana Lopez");
        assert!(normalize_name("   ").is_err());
        assert!(normalize_name(&"x".repeat(MAX_NAME_LEN + 1)).is_err());
    }

    #[tokio::test]
    async fn typing_a_name_once_creates_one_person_reused_case_insensitively() {
        let pool = test_pool().await;
        let first = PeopleRepository::get_or_create_person(&pool, "Prof Smith").await.unwrap();
        let again = PeopleRepository::get_or_create_person(&pool, "  prof   smith ").await.unwrap();
        assert_eq!(first.id, again.id);
        assert_eq!(PeopleRepository::list_people(&pool, None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn renaming_a_speaker_mirrors_to_every_line() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        add_line(&pool, "t1", "m1", 1.0, Some("S1")).await;
        add_line(&pool, "t2", "m1", 5.0, Some("S2")).await;
        add_line(&pool, "t3", "m1", 9.0, Some("S1")).await;
        // Slots exist for the lines above (diarization will create them later; here by hand)
        PeopleRepository::set_transcript_speaker(&pool, "t1", Some("S1")).await.unwrap();
        PeopleRepository::set_transcript_speaker(&pool, "t2", Some("S2")).await.unwrap();

        let person = PeopleRepository::get_or_create_person(&pool, "Ms Reyes").await.unwrap();
        let speakers = PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&person.id))
            .await
            .unwrap();

        let s1 = speakers.iter().find(|s| s.speaker_key == "S1").unwrap();
        assert_eq!(s1.person_name.as_deref(), Some("Ms Reyes"));
        assert_eq!(s1.segment_count, 2, "both S1 lines resolve to the one slot");
        let s2 = speakers.iter().find(|s| s.speaker_key == "S2").unwrap();
        assert_eq!(s2.person_name, None);

        // Renaming the person itself also flows through, still one row to update.
        PeopleRepository::rename_person(&pool, &person.id, "Ms Reyes Jr").await.unwrap();
        let speakers = PeopleRepository::get_meeting_speakers(&pool, "m1").await.unwrap();
        assert_eq!(
            speakers.iter().find(|s| s.speaker_key == "S1").unwrap().person_name.as_deref(),
            Some("Ms Reyes Jr")
        );
    }

    #[tokio::test]
    async fn rename_rejects_a_name_that_belongs_to_someone_else() {
        let pool = test_pool().await;
        let a = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::get_or_create_person(&pool, "Ben").await.unwrap();
        assert!(PeopleRepository::rename_person(&pool, &a.id, "ben").await.is_err());
        // Changing only the case of your own name is fine.
        assert_eq!(PeopleRepository::rename_person(&pool, &a.id, "ANA").await.unwrap().name, "ANA");
    }

    #[tokio::test]
    async fn add_speaker_picks_the_next_free_number() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        PeopleRepository::add_speaker(&pool, "m1").await.unwrap();
        let speakers = PeopleRepository::add_speaker(&pool, "m1").await.unwrap();
        let keys: Vec<_> = speakers.iter().map(|s| s.speaker_key.as_str()).collect();
        assert_eq!(keys, ["S1", "S2"]);
        assert!(PeopleRepository::add_speaker(&pool, "nope").await.is_err());
    }

    #[tokio::test]
    async fn merging_moves_lines_and_keeps_the_name() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        add_line(&pool, "t1", "m1", 1.0, Some("S1")).await;
        add_line(&pool, "t2", "m1", 2.0, Some("S2")).await;
        PeopleRepository::set_transcript_speaker(&pool, "t1", Some("S1")).await.unwrap();
        PeopleRepository::set_transcript_speaker(&pool, "t2", Some("S2")).await.unwrap();
        let person = PeopleRepository::get_or_create_person(&pool, "Dr Kim").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S2", Some(&person.id)).await.unwrap();

        let speakers = PeopleRepository::merge_speakers(&pool, "m1", "S2", "S1").await.unwrap();
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].speaker_key, "S1");
        assert_eq!(speakers[0].segment_count, 2);
        assert_eq!(speakers[0].person_name.as_deref(), Some("Dr Kim"));
        assert!(PeopleRepository::merge_speakers(&pool, "m1", "S1", "S1").await.is_err());
    }

    #[tokio::test]
    async fn deleting_a_person_unlinks_but_keeps_the_speaker_slot() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        let person = PeopleRepository::get_or_create_person(&pool, "Mx Osei").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&person.id)).await.unwrap();

        assert!(PeopleRepository::delete_person(&pool, &person.id).await.unwrap());
        let speakers = PeopleRepository::get_meeting_speakers(&pool, "m1").await.unwrap();
        assert_eq!(speakers.len(), 1);
        assert_eq!(speakers[0].person_id, None);
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_its_speaker_slots() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        PeopleRepository::add_speaker(&pool, "m1").await.unwrap();
        sqlx::query("DELETE FROM meetings WHERE id = 'm1'").execute(&pool).await.unwrap();
        let left: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_speakers")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(left.0, 0);
    }

    #[tokio::test]
    async fn people_list_counts_meetings_and_filters_by_name() {
        let pool = test_pool().await;
        add_meeting(&pool, "m1").await;
        add_meeting(&pool, "m2").await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana Silva").await.unwrap();
        PeopleRepository::get_or_create_person(&pool, "Ben_100%").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m2", "S3", Some(&ana.id)).await.unwrap();

        let all = PeopleRepository::list_people(&pool, None).await.unwrap();
        let ana_row = all.iter().find(|p| p.name == "Ana Silva").unwrap();
        assert_eq!(ana_row.meeting_count, 2);
        assert!(ana_row.last_meeting_at.is_some());

        assert_eq!(PeopleRepository::list_people(&pool, Some("silv")).await.unwrap().len(), 1);
        // Wildcards in the query are literal, not patterns.
        assert_eq!(PeopleRepository::list_people(&pool, Some("_100%")).await.unwrap().len(), 1);
        assert_eq!(PeopleRepository::list_people(&pool, Some("%")).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn notes_round_trip_and_blank_clears() {
        let pool = test_pool().await;
        let p = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        assert_eq!(p.notes, None);
        let p = PeopleRepository::update_person_notes(&pool, &p.id, Some("  Liaison, J2 shop \n")).await.unwrap();
        assert_eq!(p.notes.as_deref(), Some("Liaison, J2 shop"));
        let p = PeopleRepository::update_person_notes(&pool, &p.id, Some("   ")).await.unwrap();
        assert_eq!(p.notes, None, "blank notes clear the field");
        assert!(PeopleRepository::update_person_notes(&pool, &p.id, Some(&"x".repeat(MAX_NOTES_LEN + 1))).await.is_err());
        assert!(PeopleRepository::update_person_notes(&pool, "nope", Some("x")).await.is_err());
    }

    async fn add_meeting_on(pool: &SqlitePool, id: &str, created: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id).bind(format!("Meeting {}", id)).bind(created).bind(created)
            .execute(pool).await.unwrap();
    }

    async fn add_line_text(pool: &SqlitePool, id: &str, meeting_id: &str, start: f64, speaker: &str, text: &str) {
        sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, speaker) VALUES (?, ?, ?, '10:00', ?, ?)")
            .bind(id).bind(meeting_id).bind(text).bind(start).bind(speaker)
            .execute(pool).await.unwrap();
    }

    /// Two meetings; Ana is S1 in both (one meeting also has her split across S1 and S3), Ben is S2.
    async fn seeded() -> (SqlitePool, Person, Person) {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        add_line_text(&pool, "a1", "m1", 5.0, "S1", "Привіт усім, починаємо зустріч").await;
        add_line_text(&pool, "b1", "m1", 9.0, "S2", "Budget is ready").await;
        add_line_text(&pool, "a2", "m1", 12.0, "S3", "Ще одне речення від Ани").await;
        add_line_text(&pool, "a3", "m2", 3.0, "S1", "Second meeting, still me").await;
        add_line_text(&pool, "b2", "m2", 8.0, "S2", "And me again").await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Оксана Коваль").await.unwrap();
        let ben = PeopleRepository::get_or_create_person(&pool, "Ben Okoye").await.unwrap();
        for (m, key, who) in [("m1", "S1", &ana), ("m1", "S3", &ana), ("m1", "S2", &ben), ("m2", "S1", &ana), ("m2", "S2", &ben)] {
            PeopleRepository::set_speaker_person(&pool, m, key, Some(&who.id)).await.unwrap();
        }
        (pool, ana, ben)
    }

    #[tokio::test]
    async fn a_persons_meetings_are_newest_first_with_their_line_counts() {
        let (pool, ana, ben) = seeded().await;
        let meetings = PeopleRepository::list_person_meetings(&pool, &ana.id).await.unwrap();
        let summary: Vec<_> = meetings.iter().map(|m| (m.meeting_id.as_str(), m.line_count, m.first_line_start)).collect();
        // m1: S1 + S3 both belong to her (2 lines, first at 5s); m2: 1 line
        assert_eq!(summary, [("m2", 1, Some(3.0)), ("m1", 2, Some(5.0))]);
        let ben_meetings = PeopleRepository::list_person_meetings(&pool, &ben.id).await.unwrap();
        assert_eq!(ben_meetings.iter().map(|m| m.line_count).collect::<Vec<_>>(), [1, 1]);
        let detail = PeopleRepository::get_person_detail(&pool, &ana.id).await.unwrap();
        assert_eq!(detail.person.name, "Оксана Коваль");
        assert!(PeopleRepository::get_person_detail(&pool, "nope").await.is_err());
    }

    #[tokio::test]
    async fn what_a_person_said_is_searchable_in_any_script_and_excludes_others() {
        let (pool, ana, _) = seeded().await;
        let all = PeopleRepository::list_person_lines(&pool, &ana.id, None, 100).await.unwrap();
        assert_eq!(all.len(), 3, "only Ana's lines, across both of her slots");
        assert!(all.iter().all(|l| l.text != "Budget is ready" && l.text != "And me again"));
        assert_eq!(all[0].meeting_id, "m2", "newest meeting first");

        // Uppercase query still finds the capitalised Cyrillic line (SQLite alone could not)
        let hits = PeopleRepository::list_person_lines(&pool, &ana.id, Some("ЗУСТРІЧ"), 100).await.unwrap();
        assert_eq!(hits.iter().map(|l| l.transcript_id.as_str()).collect::<Vec<_>>(), ["a1"]);
        assert_eq!(PeopleRepository::list_person_lines(&pool, &ana.id, None, 2).await.unwrap().len(), 2, "limit applies");
        assert!(PeopleRepository::list_person_lines(&pool, &ana.id, Some("budget"), 100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn searching_by_name_finds_that_persons_meetings() {
        let (pool, _, _) = seeded().await;
        let hits = PeopleRepository::search_meetings_by_person(&pool, "оксан").await.unwrap();
        assert_eq!(hits.iter().map(|h| h.meeting_id.as_str()).collect::<Vec<_>>(), ["m2", "m1"]);
        assert_eq!(hits[0].person_name, "Оксана Коваль");
        let hits = PeopleRepository::search_meetings_by_person(&pool, "  OKOYE ").await.unwrap();
        assert_eq!(hits.len(), 2);
        assert!(PeopleRepository::search_meetings_by_person(&pool, "   ").await.unwrap().is_empty());
        assert!(PeopleRepository::search_meetings_by_person(&pool, "zzz").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn merging_two_people_moves_everything_and_combines_notes() {
        let (pool, ana, ben) = seeded().await;
        PeopleRepository::update_person_notes(&pool, &ana.id, Some("J2 liaison")).await.unwrap();
        PeopleRepository::update_person_notes(&pool, &ben.id, Some("Visiting from AFRICOM")).await.unwrap();

        let merged = PeopleRepository::merge_people(&pool, &ben.id, &ana.id).await.unwrap();
        assert_eq!(merged.id, ana.id);
        assert_eq!(merged.notes.as_deref(), Some("J2 liaison\n\n(From Ben Okoye) Visiting from AFRICOM"));
        assert!(PeopleRepository::get_person(&pool, &ben.id).await.unwrap().is_none());
        // Ana now owns Ben's slots too, so she is in the same two meetings with all five lines
        let meetings = PeopleRepository::list_person_meetings(&pool, &ana.id).await.unwrap();
        assert_eq!(meetings.iter().map(|m| m.line_count).sum::<i64>(), 5);
        // Speaker names in the meeting reflect the merge
        let speakers = PeopleRepository::get_meeting_speakers(&pool, "m1").await.unwrap();
        assert!(speakers.iter().all(|s| s.person_name.as_deref() == Some("Оксана Коваль")));

        assert!(PeopleRepository::merge_people(&pool, &ana.id, &ana.id).await.is_err());
        assert!(PeopleRepository::merge_people(&pool, "nope", &ana.id).await.is_err());
    }

    fn assign(pairs: &[(&str, Option<usize>)]) -> Vec<(String, Option<usize>)> {
        pairs.iter().map(|(id, l)| (id.to_string(), *l)).collect()
    }

    #[tokio::test]
    async fn detection_creates_numbered_speakers_and_labels_the_lines() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        for (i, id) in ["l1", "l2", "l3", "l4"].iter().enumerate() {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time) VALUES (?, 'm1', 'x', '10:00', ?)")
                .bind(id).bind(i as f64).execute(&pool).await.unwrap();
        }
        let speakers = PeopleRepository::apply_detected_speakers(
            &pool, "m1", &assign(&[("l1", Some(0)), ("l2", Some(1)), ("l3", Some(0)), ("l4", None)]), &[],
        ).await.unwrap();
        let summary: Vec<_> = speakers.iter().map(|s| (s.speaker_key.as_str(), s.segment_count)).collect();
        assert_eq!(summary, [("S1", 2), ("S2", 1)]);
        let labels: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, speaker FROM transcripts ORDER BY id").fetch_all(&pool).await.unwrap();
        assert_eq!(labels[3], ("l4".to_string(), None), "an unlabelled line stays unlabelled");
        assert!(PeopleRepository::apply_detected_speakers(&pool, "nope", &[], &[]).await.is_err());
    }

    #[tokio::test]
    async fn re_detecting_keeps_names_for_voices_that_still_match() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        // Before: Ana = S1 (a1 a2 a3), Ben = S2 (b1 b2)
        for (id, key) in [("a1", "S1"), ("a2", "S1"), ("a3", "S1"), ("b1", "S2"), ("b2", "S2"), ("c1", "S2")] {
            add_line_text(&pool, id, "m1", 0.0, key, "x").await;
        }
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        let ben = PeopleRepository::get_or_create_person(&pool, "Ben").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S2", Some(&ben.id)).await.unwrap();

        // New detection numbers them the other way round, and puts c1 (was Ben's) with Ana's group
        let speakers = PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[
            ("b1", Some(0)), ("b2", Some(0)),                                   // new S1 = old Ben's voice
            ("a1", Some(1)), ("a2", Some(1)), ("a3", Some(1)), ("c1", Some(1)), // new S2: 3 of 4 lines were Ana's
        ]), &[]).await.unwrap();
        let by_key: std::collections::HashMap<_, _> = speakers.iter().map(|s| (s.speaker_key.as_str(), s.person_name.as_deref())).collect();
        assert_eq!(by_key["S1"], Some("Ben"));
        assert_eq!(by_key["S2"], Some("Ana"));
    }

    #[tokio::test]
    async fn a_name_is_not_carried_over_on_a_weak_match_or_given_to_two_speakers() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        for (id, key) in [("a1", "S1"), ("a2", "S1"), ("x1", "S2"), ("x2", "S2"), ("x3", "S2"), ("x4", "S2")] {
            add_line_text(&pool, id, "m1", 0.0, key, "x").await;
        }
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();

        // Ana's two lines are split across two new speakers: only a group where she is >= half keeps her
        let speakers = PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[
            ("a1", Some(0)), ("x1", Some(0)), ("x2", Some(0)),   // 1 of 3 lines: too weak
            ("a2", Some(1)), ("x3", Some(1)),                    // 1 of 2 lines: half, enough
            ("x4", Some(2)),
        ]), &[]).await.unwrap();
        let named: Vec<_> = speakers.iter().filter_map(|s| s.person_name.as_deref().map(|n| (s.speaker_key.as_str(), n))).collect();
        assert_eq!(named, [("S2", "Ana")], "Ana goes to exactly one speaker");
    }

    // ---- voice memory

    /// A unit vector along `axis` with a little of `other` mixed in: cosine to `axis` is about `closeness`.
    fn voice(dim: usize, axis: usize, other: usize, closeness: f32) -> Vec<f32> {
        let off = (1.0 - closeness * closeness).sqrt();
        (0..dim).map(|d| if d == axis { closeness } else if d == other { off } else { 0.0 }).collect()
    }

    /// Detect a meeting with the given (line id -> speaker) layout and one voice per speaker.
    async fn detect(pool: &SqlitePool, meeting: &str, lines: &[(&str, usize)], voices: Vec<Option<Vec<f32>>>) {
        for (i, (id, _)) in lines.iter().enumerate() {
            add_line_text(pool, id, meeting, i as f64, "", "x").await;
        }
        let assignments: Vec<(String, Option<usize>)> = lines.iter().map(|(id, l)| (id.to_string(), Some(*l))).collect();
        let voices: Vec<Option<(Vec<f32>, usize)>> = voices.into_iter().map(|v| v.map(|e| (e, 5))).collect();
        PeopleRepository::apply_detected_speakers(pool, meeting, &assignments, &voices).await.unwrap();
    }

    #[tokio::test]
    async fn an_unnamed_speaker_is_matched_to_a_remembered_voice() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        // Meeting 1: Ana (axis 0) and Ben (axis 1) are named
        detect(&pool, "m1", &[("a1", 0), ("b1", 1)], vec![Some(voice(8, 0, 7, 1.0)), Some(voice(8, 1, 7, 1.0))]).await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        let ben = PeopleRepository::get_or_create_person(&pool, "Ben").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S2", Some(&ben.id)).await.unwrap();

        // Meeting 2: S1 sounds like Ben (0.9), S2 like Ana (0.75), S3 like nobody
        detect(&pool, "m2", &[("x1", 0), ("x2", 1), ("x3", 2)], vec![
            Some(voice(8, 1, 6, 0.9)), Some(voice(8, 0, 6, 0.75)), Some(voice(8, 5, 6, 1.0)),
        ]).await;
        let suggestions = PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap();
        let summary: Vec<_> = suggestions.iter().map(|s| (s.speaker_key.as_str(), s.person_name.as_str(), s.strength.as_str(), s.is_best)).collect();
        assert_eq!(summary, [("S1", "Ben", "likely", true), ("S2", "Ana", "likely", true)]);
        assert!(suggestions.iter().all(|s| s.meetings == 1));
        assert!((suggestions[0].similarity - 0.9).abs() < 1e-4);
    }

    #[tokio::test]
    async fn weak_matches_named_speakers_and_the_meetings_own_voices_are_not_suggested() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        detect(&pool, "m1", &[("a1", 0)], vec![Some(voice(8, 0, 7, 1.0))]).await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        // Ana's own meeting never suggests her to herself
        assert!(PeopleRepository::suggest_people_for_meeting(&pool, "m1").await.unwrap().is_empty());

        // 0.5 is below the threshold; 0.6 is a "possible" match
        detect(&pool, "m2", &[("x1", 0), ("x2", 1)], vec![Some(voice(8, 0, 6, 0.5)), Some(voice(8, 0, 5, 0.6))]).await;
        let suggestions = PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap();
        assert_eq!(suggestions.len(), 1);
        assert_eq!((suggestions[0].speaker_key.as_str(), suggestions[0].strength.as_str()), ("S2", "possible"));

        // Once the speaker is named there is nothing left to suggest for it
        PeopleRepository::set_speaker_person(&pool, "m2", "S2", Some(&ana.id)).await.unwrap();
        assert!(PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_person_is_the_first_suggestion_for_only_one_speaker() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        detect(&pool, "m1", &[("a1", 0)], vec![Some(voice(8, 0, 7, 1.0))]).await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();

        // Detection split one voice in two: S1 is closer to Ana than S2 is
        detect(&pool, "m2", &[("x1", 0), ("x2", 1)], vec![Some(voice(8, 0, 6, 0.95)), Some(voice(8, 0, 5, 0.8))]).await;
        let suggestions = PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap();
        let best: Vec<_> = suggestions.iter().filter(|s| s.is_best).map(|s| s.speaker_key.as_str()).collect();
        assert_eq!(best, ["S1"], "Ana is offered first only to the closer speaker");
        assert!(suggestions.iter().any(|s| s.speaker_key == "S2" && !s.is_best), "still listed as an alternative for S2");
    }

    #[tokio::test]
    async fn the_closest_remembered_voice_counts_and_meetings_are_tallied() {
        let pool = test_pool().await;
        for (id, day) in [("m1", "01"), ("m2", "02"), ("m3", "03")] {
            add_meeting_on(&pool, id, &format!("2026-10-{day}T10:00:00Z")).await;
        }
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        // Ana sounded different in two meetings (a studio vs a noisy room); the new one matches the second
        detect(&pool, "m1", &[("a1", 0)], vec![Some(voice(8, 0, 7, 1.0))]).await;
        detect(&pool, "m2", &[("b1", 0)], vec![Some(voice(8, 1, 7, 1.0))]).await;
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m2", "S1", Some(&ana.id)).await.unwrap();
        detect(&pool, "m3", &[("c1", 0)], vec![Some(voice(8, 1, 6, 0.85))]).await;
        let s = PeopleRepository::suggest_people_for_meeting(&pool, "m3").await.unwrap();
        assert_eq!(s.len(), 1);
        assert!((s[0].similarity - 0.85).abs() < 1e-4, "nearest sample, not the average");
        assert_eq!(s[0].meetings, 2);
        assert_eq!(PeopleRepository::get_person_detail(&pool, &ana.id).await.unwrap().voice_meetings, 2);
    }

    #[tokio::test]
    async fn forgetting_a_voice_keeps_the_person_and_stops_suggestions() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        detect(&pool, "m1", &[("a1", 0)], vec![Some(voice(8, 0, 7, 1.0))]).await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        detect(&pool, "m2", &[("x1", 0)], vec![Some(voice(8, 0, 6, 0.9))]).await;
        assert_eq!(PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap().len(), 1);

        assert_eq!(PeopleRepository::forget_person_voice(&pool, &ana.id).await.unwrap(), 1);
        assert!(PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap().is_empty());
        let detail = PeopleRepository::get_person_detail(&pool, &ana.id).await.unwrap();
        assert_eq!((detail.voice_meetings, detail.meetings.len()), (0, 1), "still in the meeting, just not recognisable");
    }

    #[tokio::test]
    async fn remembering_can_be_switched_off_and_deleting_a_person_forgets_their_voice() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_line_text(&pool, "a1", "m1", 0.0, "", "x").await;
        // No voices passed: nothing stored
        PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[("a1", Some(0))]), &[]).await.unwrap();
        let (stored,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_speakers WHERE voiceprint IS NOT NULL").fetch_one(&pool).await.unwrap();
        assert_eq!(stored, 0);

        PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[("a1", Some(0))]), &[Some((voice(8, 0, 7, 1.0), 3))]).await.unwrap();
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        PeopleRepository::delete_person(&pool, &ana.id).await.unwrap();
        let (stored,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_speakers WHERE voiceprint IS NOT NULL").fetch_one(&pool).await.unwrap();
        assert_eq!(stored, 0, "no voice is left behind");
        assert_eq!(PeopleRepository::get_meeting_speakers(&pool, "m1").await.unwrap().len(), 1, "the speaker slot itself stays");
    }

    #[tokio::test]
    async fn merging_two_speakers_blends_their_voices() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_meeting_on(&pool, "m2", "2026-10-02T10:00:00Z").await;
        // One person split in two by detection: halves along axes 0 and 1
        detect(&pool, "m1", &[("a1", 0), ("a2", 1)], vec![Some(voice(8, 0, 7, 1.0)), Some(voice(8, 1, 7, 1.0))]).await;
        PeopleRepository::merge_speakers(&pool, "m1", "S2", "S1").await.unwrap();
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        // The blend sits between both halves, so a voice near either half now matches (~0.71 each)
        detect(&pool, "m2", &[("x1", 0)], vec![Some(voice(8, 1, 6, 1.0))]).await;
        let s = PeopleRepository::suggest_people_for_meeting(&pool, "m2").await.unwrap();
        assert_eq!(s.len(), 1);
        assert!((s[0].similarity - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-3, "similarity {}", s[0].similarity);
    }

    // ---- red team: hostile input

    #[tokio::test]
    async fn names_cannot_hide_behind_invisible_or_text_direction_characters() {
        let pool = test_pool().await;
        let real = PeopleRepository::get_or_create_person(&pool, "Prof Smith").await.unwrap();
        // zero-width space, zero-width joiner, soft hyphen, BOM, NUL and a right-to-left override
        for sneaky in ["Prof\u{200B} Smith", "Prof Smi\u{200D}th", "\u{FEFF}Prof Smith", "Prof Smith\u{0000}", "\u{202E}Prof Smith", "Prof\u{00AD} Smith"] {
            let found = PeopleRepository::get_or_create_person(&pool, sneaky).await.unwrap();
            assert_eq!(found.id, real.id, "{sneaky:?} must not create a look-alike duplicate");
            assert_eq!(found.name, "Prof Smith");
        }
        assert!(PeopleRepository::get_or_create_person(&pool, "\u{200B}\u{202E}\u{0000}").await.is_err(), "a name of only invisible characters is empty");
        let renamed = PeopleRepository::rename_person(&pool, &real.id, "Prof\u{202E}essor Smith").await.unwrap();
        assert_eq!(renamed.name, "Professor Smith");
        // Ordinary multi-script names are untouched
        assert_eq!(PeopleRepository::get_or_create_person(&pool, "Оксана  Коваль").await.unwrap().name, "Оксана Коваль");
        assert_eq!(PeopleRepository::get_or_create_person(&pool, "José García").await.unwrap().name, "José García");
    }

    #[tokio::test]
    async fn notes_drop_control_characters_but_keep_line_breaks() {
        let pool = test_pool().await;
        let p = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        let saved = PeopleRepository::update_person_notes(&pool, &p.id, Some("Line one\n\tindented\u{0000}\u{202E}tail\u{0007}")).await.unwrap();
        assert_eq!(saved.notes.as_deref(), Some("Line one\n\tindentedtail"));
    }

    #[tokio::test]
    async fn sql_looking_text_is_stored_and_searched_as_plain_text() {
        let pool = test_pool().await;
        let nasty = "Robert'); DROP TABLE people; --";
        let p = PeopleRepository::get_or_create_person(&pool, nasty).await.unwrap();
        assert_eq!(p.name, nasty);
        PeopleRepository::update_person_notes(&pool, &p.id, Some("'; DELETE FROM meetings; --")).await.unwrap();
        // The tables are intact and the text is found literally
        assert_eq!(PeopleRepository::list_people(&pool, Some("DROP TABLE")).await.unwrap().len(), 1);
        assert_eq!(PeopleRepository::list_people(&pool, Some("'); ")).await.unwrap().len(), 1);
        assert_eq!(PeopleRepository::list_people(&pool, Some("%")).await.unwrap().len(), 0, "wildcards are literal");
        assert_eq!(PeopleRepository::list_people(&pool, Some("\\")).await.unwrap().len(), 0);
        assert!(PeopleRepository::search_meetings_by_person(&pool, "x') OR ('1'='1").await.unwrap().is_empty());
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM people").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn speaker_keys_cannot_carry_sql_or_oversized_numbers() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        for bad in ["S1'); DROP TABLE meeting_speakers; --", "S99999999999999999999", "S1 OR 1=1", "../S1", "S٣"] {
            assert!(PeopleRepository::set_speaker_person(&pool, "m1", bad, None).await.is_err(), "{bad:?} accepted");
        }
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM meeting_speakers").fetch_one(&pool).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn an_absurd_detected_speaker_number_is_an_error_not_a_panic() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_line_text(&pool, "a1", "m1", 0.0, "", "x").await;
        for label in [usize::MAX, usize::MAX - 1, u32::MAX as usize + 5] {
            let result = PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[("a1", Some(label))]), &[]).await;
            assert!(result.is_err(), "label {label} should be rejected");
        }
    }

    #[tokio::test]
    async fn stale_detection_results_cannot_wipe_a_meetings_names() {
        let pool = test_pool().await;
        add_meeting_on(&pool, "m1", "2026-10-01T10:00:00Z").await;
        add_line_text(&pool, "a1", "m1", 0.0, "S1", "x").await;
        let ana = PeopleRepository::get_or_create_person(&pool, "Ana").await.unwrap();
        PeopleRepository::set_speaker_person(&pool, "m1", "S1", Some(&ana.id)).await.unwrap();
        // The transcript was replaced (e.g. re-transcribed) while detection ran: none of its line ids exist any more
        let result = PeopleRepository::apply_detected_speakers(&pool, "m1", &assign(&[("gone-1", Some(0)), ("gone-2", Some(1))]), &[]).await;
        assert!(result.is_err());
        let speakers = PeopleRepository::get_meeting_speakers(&pool, "m1").await.unwrap();
        assert_eq!(speakers.len(), 1, "existing speakers survive");
        assert_eq!(speakers[0].person_name.as_deref(), Some("Ana"), "and keep their name");
    }
}
