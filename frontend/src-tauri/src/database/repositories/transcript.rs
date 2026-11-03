use crate::api::{TranscriptSearchResult, TranscriptSegment};
use chrono::Utc;
use sqlx::{Connection, Error as SqlxError, SqlitePool};
use tracing::{error, info};
use uuid::Uuid;

pub struct TranscriptsRepository;

impl TranscriptsRepository {
    /// Saves a recording's transcript segments, on a new meeting or, for a recording started
    /// from a notes-first meeting, on `into_meeting` (spec F-NM-3). That meeting is used only
    /// while it exists and has no recording yet; otherwise a new meeting is created, so a
    /// transcript is never merged into another recording or lost (F-NM-4).
    /// This function uses a transaction to ensure that either both the meeting
    /// and all its transcripts are saved, or none of them are.
    pub async fn save_transcript(
        pool: &SqlitePool,
        meeting_title: &str,
        transcripts: &[TranscriptSegment],
        folder_path: Option<String>,
        into_meeting: Option<&str>,
    ) -> Result<String, SqlxError> {
        let mut conn = pool.acquire().await?;
        let mut transaction = conn.begin().await?;

        let now = Utc::now();

        // 1. Use the meeting the recording was started from, when it is still free
        let attached = match into_meeting {
            Some(id) => {
                sqlx::query(
                    // The recording arrives now, so its retention starts now: the finalizer
                    // stamps it (a notes-first meeting may carry an earlier, audio-less stamp)
                    "UPDATE meetings SET folder_path = ?1, updated_at = ?2,
                       audio_retention = NULL, audio_expires_at = NULL, finalized_at = NULL, audio_state = 'none'
                     WHERE id = ?3 AND folder_path IS NULL
                       AND NOT EXISTS (SELECT 1 FROM transcripts WHERE meeting_id = ?3)",
                )
                .bind(&folder_path)
                .bind(now)
                .bind(id)
                .execute(&mut *transaction)
                .await?
                .rows_affected()
                    > 0
            }
            None => false,
        };

        let meeting_id = if attached {
            let id = into_meeting.unwrap_or_default().to_string();
            info!("Saving the recording into existing meeting {}", id);
            id
        } else {
            if into_meeting.is_some() {
                info!("The meeting the recording was started from is gone or already recorded; creating a new one");
            }
            let meeting_id = format!("meeting-{}", Uuid::new_v4());
            // Create the new meeting
            let result = sqlx::query(
                "INSERT INTO meetings (id, title, created_at, updated_at, folder_path) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&meeting_id)
            .bind(meeting_title)
            .bind(now)
            .bind(now)
            .bind(&folder_path)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!("Failed to create meeting {}: {}", meeting_id, e);
                transaction.rollback().await?;
                return Err(e);
            }

            info!("Successfully created meeting with id: {}", meeting_id);
            meeting_id
        };

        // 2. Save each transcript segment with audio timing fields
        for segment in transcripts {
            let transcript_id = format!("transcript-{}", Uuid::new_v4());
            let result = sqlx::query(
                "INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, duration)
                 VALUES (?, ?, ?, ?, ?, ?, ?)"
            )
            .bind(&transcript_id)
            .bind(&meeting_id)
            .bind(&segment.text)
            .bind(&segment.timestamp)
            .bind(segment.audio_start_time)
            .bind(segment.audio_end_time)
            .bind(segment.duration)
            .execute(&mut *transaction)
            .await;

            if let Err(e) = result {
                error!(
                    "Failed to save transcript segment for meeting {}: {}",
                    meeting_id, e
                );
                transaction.rollback().await?;
                return Err(e);
            }
        }

        info!(
            "Successfully saved {} transcript segments for meeting {}",
            transcripts.len(),
            meeting_id
        );

        // Commit the transaction
        transaction.commit().await?;

        Ok(meeting_id)
    }

    /// Searches for a query string within the transcripts.
    /// It returns a list of matching transcripts with context.
    pub async fn search_transcripts(
        pool: &SqlitePool,
        query: &str,
    ) -> Result<Vec<TranscriptSearchResult>, SqlxError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let search_query = format!("%{}%", query.to_lowercase());
        let [as_typed, lower, upper, capitalized] = Self::case_variants(query);

        // SQLite's LOWER()/LIKE only fold ASCII, so a lowercase Cyrillic (or accented) query
        // would never find "Привіт" or "ПРИВІТ". instr() is exact but handles any script, so
        // also try the query as typed, lowercase, UPPERCASE and Capitalised.
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT m.id, m.title, t.transcript, t.timestamp
             FROM meetings m
             JOIN transcripts t ON m.id = t.meeting_id
             WHERE LOWER(t.transcript) LIKE ?1
                OR instr(t.transcript, ?2) > 0
                OR instr(t.transcript, ?3) > 0
                OR instr(t.transcript, ?4) > 0
                OR instr(t.transcript, ?5) > 0",
        )
        .bind(&search_query)
        .bind(&as_typed)
        .bind(&lower)
        .bind(&upper)
        .bind(&capitalized)
        .fetch_all(pool)
        .await?;

        let results = rows
            .into_iter()
            .map(|(id, title, transcript, timestamp)| {
                let match_context = Self::get_match_context(&transcript, query);
                TranscriptSearchResult {
                    id,
                    title,
                    match_context,
                    timestamp,
                }
            })
            .collect();

        Ok(results)
    }

    /// The query as typed, lowercase, UPPERCASE and Capitalised (any script).
    fn case_variants(query: &str) -> [String; 4] {
        let lower = query.to_lowercase();
        let mut chars = lower.chars();
        let capitalized = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => String::new(),
        };
        [query.to_string(), lower, query.to_uppercase(), capitalized]
    }

    /// Helper function to extract a snippet of text around the first match of a query.
    ///
    /// Works on characters, not bytes: slicing a `&str` at a computed byte offset panics when it
    /// lands inside a multi-byte character (most Cyrillic, accented or CJK text). Characters are
    /// lowercased one-to-one so positions in the folded text line up with the original.
    fn get_match_context(transcript: &str, query: &str) -> String {
        const CONTEXT_CHARS: usize = 100;
        let fold = |c: char| c.to_lowercase().next().unwrap_or(c);

        let chars: Vec<char> = transcript.chars().collect();
        let folded: Vec<char> = chars.iter().copied().map(fold).collect();
        let needle: Vec<char> = query.chars().map(fold).collect();

        let match_index = if needle.is_empty() || needle.len() > folded.len() {
            None
        } else {
            folded.windows(needle.len()).position(|window| window == needle.as_slice())
        };

        match match_index {
            Some(match_index) => {
                let start_index = match_index.saturating_sub(CONTEXT_CHARS);
                let end_index = (match_index + needle.len() + CONTEXT_CHARS).min(chars.len());

                let mut context = String::new();
                if start_index > 0 {
                    context.push_str("...");
                }
                context.extend(&chars[start_index..end_index]);
                if end_index < chars.len() {
                    context.push_str("...");
                }
                context
            }
            None => chars.iter().take(200).collect(), // Fallback to the start of the transcript
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    /// Small deterministic PRNG so the fuzz test needs no extra dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    #[test]
    fn snippet_never_panics_on_non_ascii_text() {
        let letters: Vec<char> = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюяéñüß".chars().collect();
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for _ in 0..3000 {
            let words: Vec<String> = (0..40 + rng.next() % 40)
                .map(|_| {
                    (0..2 + rng.next() % 8)
                        .map(|_| letters[(rng.next() % letters.len() as u64) as usize])
                        .collect()
                })
                .collect();
            let at = (15 + rng.next() % 20) as usize;
            let query = words[at].clone();
            let context = TranscriptsRepository::get_match_context(&words.join(" "), &query);
            assert!(context.contains(&query), "context lost the match: {context:?} / {query:?}");
        }
    }

    #[test]
    fn snippet_matches_regardless_of_case_and_trims_with_ellipses() {
        let text = format!("{} Зустріч {}", "слово ".repeat(60), "кінець ".repeat(60));
        let context = TranscriptsRepository::get_match_context(&text, "зустріч");
        assert!(context.starts_with("...") && context.ends_with("..."));
        assert!(context.contains("Зустріч"), "original casing is kept in the snippet");
        // No match falls back to the start of the text
        assert!(TranscriptsRepository::get_match_context("short text", "zzz").starts_with("short"));
    }

    async fn pool_with(lines: &[&str]) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m1', 'Call', 'x', 'x')")
            .execute(&pool)
            .await
            .unwrap();
        for (i, line) in lines.iter().enumerate() {
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp) VALUES (?, 'm1', ?, '10:00')")
                .bind(format!("t{i}"))
                .bind(line)
                .execute(&pool)
                .await
                .unwrap();
        }
        pool
    }

    fn segments(n: usize) -> Vec<TranscriptSegment> {
        (0..n)
            .map(|i| TranscriptSegment {
                id: format!("s{i}"),
                text: format!("line {i}"),
                timestamp: "10:00".to_string(),
                audio_start_time: Some(i as f64),
                audio_end_time: Some(i as f64 + 1.0),
                duration: Some(1.0),
            })
            .collect()
    }

    async fn meeting_count(pool: &SqlitePool) -> i64 {
        sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM meetings").fetch_one(pool).await.unwrap().0
    }

    #[tokio::test]
    async fn a_recording_saves_into_the_meeting_it_was_started_from() {
        let pool = pool_with(&[]).await;
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m2', 'Planning', 'x', 'x')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES ('m2', '[]', 'agenda', 'x')")
            .execute(&pool).await.unwrap();

        let id = TranscriptsRepository::save_transcript(&pool, "Recorder name", &segments(3), Some("C:/rec/m2".into()), Some("m2"))
            .await
            .unwrap();

        assert_eq!(id, "m2");
        assert_eq!(meeting_count(&pool).await, 2, "no second meeting is created");
        let (title, folder): (String, Option<String>) = sqlx::query_as("SELECT title, folder_path FROM meetings WHERE id = 'm2'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(title, "Planning", "the meeting keeps its own title");
        assert_eq!(folder.as_deref(), Some("C:/rec/m2"));
        let lines: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transcripts WHERE meeting_id = 'm2'").fetch_one(&pool).await.unwrap();
        assert_eq!(lines.0, 3);
        let notes: (String,) = sqlx::query_as("SELECT body_markdown FROM meeting_user_notes WHERE meeting_id = 'm2'").fetch_one(&pool).await.unwrap();
        assert_eq!(notes.0, "agenda");
    }

    #[tokio::test]
    async fn a_meeting_that_already_has_a_recording_gets_a_new_meeting_instead() {
        // m1 already has lines; nothing is merged or lost (F-NM-4)
        let pool = pool_with(&["earlier"]).await;
        let id = TranscriptsRepository::save_transcript(&pool, "Second", &segments(2), None, Some("m1")).await.unwrap();
        assert_ne!(id, "m1");
        assert_eq!(meeting_count(&pool).await, 2);
        let old: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transcripts WHERE meeting_id = 'm1'").fetch_one(&pool).await.unwrap();
        assert_eq!(old.0, 1);
    }

    #[tokio::test]
    async fn a_recording_for_a_deleted_meeting_is_still_saved() {
        let pool = pool_with(&[]).await;
        let id = TranscriptsRepository::save_transcript(&pool, "Kept", &segments(2), None, Some("meeting-gone")).await.unwrap();
        assert_ne!(id, "meeting-gone");
        let lines: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM transcripts WHERE meeting_id = ?").bind(&id).fetch_one(&pool).await.unwrap();
        assert_eq!(lines.0, 2);
    }

    #[tokio::test]
    async fn a_lowercase_query_finds_capitalised_and_uppercase_non_english_text() {
        let pool = pool_with(&["Привіт усім, починаємо", "ПРИВІТ УСІМ", "інший рядок без збігу", "Reunión de hoy"]).await;
        let hits = TranscriptsRepository::search_transcripts(&pool, "привіт").await.unwrap();
        assert_eq!(hits.len(), 2, "capitalised and ALL-CAPS Cyrillic lines both match");
        let hits = TranscriptsRepository::search_transcripts(&pool, "reunión").await.unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn ascii_search_is_still_fully_case_insensitive() {
        let pool = pool_with(&["The Budget is ready", "bUdGeT mixed case", "unrelated"]).await;
        let hits = TranscriptsRepository::search_transcripts(&pool, "BUDGET").await.unwrap();
        assert_eq!(hits.len(), 2);
    }
}
