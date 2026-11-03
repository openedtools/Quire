//! Exporter tests over generated, fictional meetings (AC-SYNC-1, AC-SYNC-6, AC-SYNC-8, AC-SYNC-9).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

use super::export::{self, ExportReport};
use super::format;
use super::store;

const VOICEPRINT: &[u8] = b"VOICEPRINT-NEVER-IN-THE-VAULT";
const KEY: &str = "sk-never-in-the-vault";

async fn pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn exec(pool: &SqlitePool, sql: &str, binds: &[&str]) {
    let mut q = sqlx::query(sql);
    for b in binds {
        q = q.bind(*b);
    }
    q.execute(pool).await.unwrap();
}

/// `n` fictional meetings across three folders (two collide once cleaned, one is a reserved name),
/// five people, transcripts with two speakers, summaries on even meetings, notes on odd ones.
async fn seed(pool: &SqlitePool, n: usize) {
    // Created a day apart, so the older folder keeps the plain name when two collide
    for (id, name, day) in [("f1", "Work", "01"), ("f2", "Work?", "02"), ("f3", "CON", "03")] {
        let at = format!("2026-09-{day}T10:00:00Z");
        exec(pool, "INSERT INTO folders (id, name, created_at, updated_at) VALUES (?, ?, ?, ?)", &[id, name, &at, &at]).await;
    }
    for i in 0..5 {
        let id = format!("p{i}");
        let name = format!("Person {i}");
        exec(pool, "INSERT INTO people (id, name, created_at, updated_at, notes) VALUES (?, ?, '2026-09-02T10:00:00Z', '2026-09-02T10:00:00Z', 'Likes tea.')", &[&id, &name]).await;
    }
    for i in 0..n {
        let id = format!("meeting-{i}");
        let title = format!("Planning session {i}: \"budget\"");
        let created = format!("2026-10-{:02}T{:02}:00:00Z", 1 + i % 28, 8 + i % 10);
        let folder = match i % 4 {
            0 => None,
            1 => Some("f1"),
            2 => Some("f2"),
            _ => Some("f3"),
        };
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at, folder_id, audio_retention, audio_state, mode) VALUES (?, ?, ?, ?, ?, 'd7', 'present', 'audio')")
            .bind(&id)
            .bind(&title)
            .bind(&created)
            .bind(&created)
            .bind(folder)
            .execute(pool)
            .await
            .unwrap();
        for (j, (speaker, text)) in [("S1", "We agree the plan."), ("S2", "I will send the note."), ("S1", "Thanks, done.")].iter().enumerate() {
            let tid = format!("t{i}-{j}");
            sqlx::query("INSERT INTO transcripts (id, meeting_id, transcript, timestamp, audio_start_time, audio_end_time, speaker) VALUES (?, ?, ?, ?, ?, ?, ?)")
                .bind(&tid)
                .bind(&id)
                .bind(format!("{text} ({i})"))
                .bind(&created)
                .bind(j as f64 * 5.0)
                .bind(j as f64 * 5.0 + 4.5)
                .bind(*speaker)
                .execute(pool)
                .await
                .unwrap();
        }
        let person = format!("p{}", i % 5);
        sqlx::query("INSERT INTO meeting_speakers (id, meeting_id, speaker_key, person_id, created_at, voiceprint) VALUES (?, ?, 'S1', ?, ?, ?)")
            .bind(format!("ms-{i}"))
            .bind(&id)
            .bind(&person)
            .bind(&created)
            .bind(VOICEPRINT)
            .execute(pool)
            .await
            .unwrap();
        if i % 2 == 0 {
            let result = serde_json::json!({ "markdown": format!("- Decision {i}\n- Action for Person {}", i % 5) }).to_string();
            exec(pool, "INSERT INTO summary_processes (meeting_id, status, created_at, updated_at, result) VALUES (?, 'completed', ?, ?, ?)", &[&id, &created, &created, &result]).await;
        } else {
            exec(pool, "INSERT INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES (?, '[]', ?, ?)", &[&id, &format!("My note {i}"), &created]).await;
        }
    }
    exec(pool, "INSERT INTO settings (id, provider, model, whisperModel, openaiApiKey) VALUES ('1', 'openai', 'm', 'large-v3', ?)", &[KEY]).await;
}

fn files(root: &Path) -> HashMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut HashMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = HashMap::new();
    walk(root, root, &mut out);
    out
}

async fn run(pool: &SqlitePool, root: &Path) -> ExportReport {
    export::export(pool, root.to_str().unwrap()).await.unwrap()
}

fn vault() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

#[tokio::test]
async fn fifty_meetings_round_trip_and_a_second_export_writes_nothing() {
    let pool = pool().await;
    seed(&pool, 50).await;
    let (_dir, root) = vault();
    let first = run(&pool, &root).await;
    assert!(first.verified, "{first:?}");
    assert_eq!((first.notes, first.people, first.folders, first.failed), (50, 5, 3, 0));
    // 50 notes + 50 sidecars + 5 people + 3 folders
    assert_eq!(first.written, 108);

    let on_disk = files(&root);
    assert!(on_disk.contains_key("quire-format.json"));
    assert!(on_disk.keys().all(|k| !k.starts_with("_tmp")), "no staging file left behind");
    // Folder names follow the Windows rules: a collision after cleaning and the reserved name
    let dirs: Vec<&str> = on_disk.keys().filter(|k| k.ends_with("/_folder.json")).map(|k| k.split('/').next().unwrap()).collect();
    for want in ["Work", "Work (2)", "CON folder"] {
        assert!(dirs.contains(&want), "{want} in {dirs:?}");
    }

    // Read every note back and compare with the database
    let notes: Vec<format::Note> = on_disk
        .iter()
        .filter(|(k, _)| k.ends_with(".md") && !k.starts_with("_people/"))
        .map(|(_, b)| format::parse_note(std::str::from_utf8(b).unwrap()).unwrap())
        .collect();
    assert_eq!(notes.len(), 50);
    let people: HashMap<String, String> = on_disk
        .iter()
        .filter(|(k, _)| k.starts_with("_people/"))
        .map(|(_, b)| format::parse_person(std::str::from_utf8(b).unwrap()).unwrap())
        .map(|p| (p.id, p.name))
        .collect();
    assert_eq!(people.len(), 5);
    for i in 0..50 {
        let (sync_id, title, created): (String, String, String) =
            sqlx::query_as("SELECT sync_id, title, created_at FROM meetings WHERE id = ?").bind(format!("meeting-{i}")).fetch_one(&pool).await.unwrap();
        let note = notes.iter().find(|n| n.id == sync_id).unwrap();
        assert_eq!(note.title, title);
        assert_eq!(note.created, format::timestamp(store::stored_time(&created).unwrap()), "timestamps");
        let sidecar_rel = on_disk.keys().find(|k| k.ends_with(&note.transcript.as_ref().unwrap().file)).unwrap();
        let (note_id, segments) = format::parse_sidecar(std::str::from_utf8(&on_disk[sidecar_rel]).unwrap()).unwrap();
        assert_eq!(note_id, sync_id);
        assert_eq!(segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), vec![
            format!("We agree the plan. ({i})"),
            format!("I will send the note. ({i})"),
            format!("Thanks, done. ({i})"),
        ]);
        assert_eq!(segments.iter().map(|s| s.speaker).collect::<Vec<_>>(), vec![Some(1), Some(2), Some(1)]);
        assert_eq!(format::sha256_hex(&on_disk[sidecar_rel]), note.transcript.as_ref().unwrap().sha256);
        // Speaker 1 is linked to a person whose file exists
        let linked = note.speakers[0].person.as_ref().unwrap();
        assert_eq!(people[linked], format!("Person {}", i % 5));
        assert_eq!(note.speakers[1].name, "Speaker 2");
        if i % 2 == 0 {
            assert!(note.body.contains(&format!("## Summary\n\n- Decision {i}\n")), "summaries");
            assert_eq!(note.summary.as_ref().unwrap().status, "done");
        } else {
            assert!(note.body.contains(&format!("## Notes\n\nMy note {i}\n")), "notes");
        }
        assert_eq!((note.audio.retention.as_str(), note.audio.state.as_str()), ("d7", "present"));
    }

    let second = run(&pool, &root).await;
    assert!(second.verified);
    assert_eq!((second.written, second.moved, second.deleted), (0, 0, 0), "nothing changed, nothing written");
    assert_eq!(files(&root), on_disk, "byte for byte the same vault");
}

#[tokio::test]
async fn voiceprints_keys_audio_and_settings_never_reach_the_vault() {
    let pool = pool().await;
    seed(&pool, 6).await;
    let (_dir, root) = vault();
    assert!(run(&pool, &root).await.verified);
    for (rel, bytes) in files(&root) {
        assert!(!bytes.windows(VOICEPRINT.len()).any(|w| w == VOICEPRINT), "{rel} holds a voiceprint");
        assert!(!String::from_utf8_lossy(&bytes).contains(KEY), "{rel} holds a key");
        for ext in [".m4a", ".mp4", ".wav", ".mp3", ".flac", ".ogg", ".caf", ".aac", ".onnx", ".gguf", ".bin", ".sqlite"] {
            assert!(!rel.ends_with(ext), "{rel} is not a vault file");
        }
        assert!(!rel.starts_with("_audio"), "no audio in Phase E");
    }
}

#[tokio::test]
async fn a_change_writes_a_new_rev_and_a_new_sidecar_and_keeps_the_old_one_for_now() {
    let pool = pool().await;
    seed(&pool, 2).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings WHERE id = 'meeting-0'").fetch_one(&pool).await.unwrap();
    exec(&pool, "UPDATE transcripts SET transcript = 'Edited line.' WHERE id = 't0-0'", &[]).await;
    exec(&pool, "UPDATE meetings SET title = 'Renamed' WHERE id = 'meeting-0'", &[]).await;
    let report = run(&pool, &root).await;
    assert!(report.verified);
    assert_eq!(report.written, 2, "the new sidecar, then the note");
    let note = format::parse_note(&std::fs::read_to_string(root.join(format!("{sync_id}.md"))).unwrap()).unwrap();
    assert_eq!((note.title.as_str(), note.rev), ("Renamed", 2));
    let sidecars = files(&root).into_keys().filter(|k| k.starts_with(&sync_id) && k.ends_with(".transcript.json")).count();
    assert_eq!(sidecars, 2, "sidecars are immutable; the old one waits for the 7-day clean-up");
}

#[tokio::test]
async fn a_file_changed_in_the_vault_is_never_overwritten() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
    let path = root.join(format!("{sync_id}.md"));
    let edited = std::fs::read_to_string(&path).unwrap() + "\nAdded on the phone.\n";
    std::fs::write(&path, &edited).unwrap();
    exec(&pool, "UPDATE meetings SET title = 'Changed on the desktop'", &[]).await;
    let report = run(&pool, &root).await;
    assert_eq!(report.changed_in_vault, 1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), edited, "left alone");
}

#[tokio::test]
async fn moving_a_meeting_moves_its_file_and_deleting_one_leaves_a_tombstone() {
    let pool = pool().await;
    seed(&pool, 2).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let (moved, gone): (String, String) = (
        sqlx::query_scalar("SELECT sync_id FROM meetings WHERE id = 'meeting-0'").fetch_one(&pool).await.unwrap(),
        sqlx::query_scalar("SELECT sync_id FROM meetings WHERE id = 'meeting-1'").fetch_one(&pool).await.unwrap(),
    );
    exec(&pool, "UPDATE meetings SET folder_id = 'f1' WHERE id = 'meeting-0'", &[]).await;
    exec(&pool, "DELETE FROM meetings WHERE id = 'meeting-1'", &[]).await;
    let report = run(&pool, &root).await;
    assert!(report.verified);
    assert_eq!((report.moved, report.deleted), (1, 1));
    assert!(root.join(format!("Work/{moved}.md")).exists());
    assert!(!root.join(format!("{moved}.md")).exists());
    assert!(root.join(format!("_trash/{gone}/{gone}.md")).exists());
    let stone: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join(format!("_trash/{gone}/_deleted.json"))).unwrap()).unwrap();
    assert_eq!(stone["id"], gone.as_str());
    let work: String = sqlx::query_scalar("SELECT sync_id FROM folders WHERE id = 'f1'").fetch_one(&pool).await.unwrap();
    assert_eq!(stone["folder_at_deletion"], work.as_str(), "the folder's vault ID, not its name");
}

#[tokio::test]
async fn a_run_that_stops_part_way_never_leaves_a_note_stuck() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
    exec(&pool, "UPDATE meetings SET title = 'Changed once'", &[]).await;
    // A run that wrote the note but stopped before it was verified: the old record stays, and
    // the pending record of what it wrote is all that is new
    let before = store::state(&pool).await.unwrap();
    run(&pool, &root).await;
    let written: Vec<store::StateRow> = store::state(&pool).await.unwrap().into_iter().filter(|r| !before.contains(r)).collect();
    assert!(!written.is_empty());
    store::replace_state(&pool, &before).await.unwrap();
    for r in &written {
        store::put_pending(&pool, r).await.unwrap();
    }
    let after_first = std::fs::read_to_string(root.join(format!("{sync_id}.md"))).unwrap();
    let rev_first = format::parse_note(&after_first).unwrap().rev;
    // The next run recognises its own file, keeps the same rev, and writes later changes
    let again = run(&pool, &root).await;
    assert_eq!((again.changed_in_vault, again.written), (0, 0), "{again:?}");
    exec(&pool, "UPDATE meetings SET title = 'Changed twice'", &[]).await;
    let third = run(&pool, &root).await;
    assert_eq!((third.changed_in_vault, third.written), (0, 1), "{third:?}");
    let note = format::parse_note(&std::fs::read_to_string(root.join(format!("{sync_id}.md"))).unwrap()).unwrap();
    assert_eq!((note.title.as_str(), note.rev), ("Changed twice", rev_first + 1));
}

#[tokio::test]
async fn renaming_a_folder_only_in_letter_case_keeps_its_notes() {
    let pool = pool().await;
    seed(&pool, 4).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let filed: String = sqlx::query_scalar("SELECT sync_id FROM meetings WHERE id = 'meeting-1'").fetch_one(&pool).await.unwrap();
    exec(&pool, "UPDATE folders SET name = 'WORK' WHERE id = 'f1'", &[]).await;
    for _ in 0..2 {
        let report = run(&pool, &root).await;
        assert!(report.verified, "{report:?}");
        assert_eq!(report.deleted, 0);
    }
    let names: Vec<String> = std::fs::read_dir(&root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert!(names.contains(&"WORK".to_string()), "{names:?}");
    assert!(root.join(format!("WORK/{filed}.md")).exists());
}

#[tokio::test]
async fn choosing_the_same_folder_again_continues_the_rev() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    let (_other_dir, other) = vault();
    super::enable(&pool, root.to_str().unwrap()).await.unwrap();
    super::run(&pool).await.unwrap();
    exec(&pool, "UPDATE meetings SET title = 'Second version'", &[]).await;
    super::run(&pool).await.unwrap();
    super::enable(&pool, other.to_str().unwrap()).await.unwrap();
    super::enable(&pool, root.to_str().unwrap()).await.unwrap();
    exec(&pool, "UPDATE meetings SET title = 'Third version'", &[]).await;
    let report = super::run(&pool).await.unwrap().unwrap();
    assert_eq!(report.changed_in_vault, 0, "its own files are recognised without a record");
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
    let note = format::parse_note(&std::fs::read_to_string(root.join(format!("{sync_id}.md"))).unwrap()).unwrap();
    assert_eq!((note.title.as_str(), note.rev), ("Third version", 3), "rev never goes back");
}

#[tokio::test]
async fn every_note_carries_its_desktop_id() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
    let note = format::parse_note(&std::fs::read_to_string(root.join(format!("{sync_id}.md"))).unwrap()).unwrap();
    assert_eq!(note.extra, vec![("legacy_id".to_string(), format::Value::Str("meeting-0".into()))]);
}

#[tokio::test]
async fn a_deleted_meeting_leaves_the_vault_after_30_days_and_its_tombstone_after_a_year() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    run(&pool, &root).await;
    let sync_id: String = sqlx::query_scalar("SELECT sync_id FROM meetings").fetch_one(&pool).await.unwrap();
    exec(&pool, "DELETE FROM meetings", &[]).await;
    run(&pool, &root).await;
    let content = root.join(format!("_trash/{sync_id}/{sync_id}.md"));
    let stone = root.join(format!("_trash/{sync_id}/_deleted.json"));
    assert!(content.exists() && stone.exists());
    exec(&pool, "UPDATE sync_state SET exported_at = '2026-01-01T00:00:00.000Z' WHERE kind = 'trash'", &[]).await;
    run(&pool, &root).await;
    assert!(!content.exists(), "the meeting's content is purged after 30 days");
    assert!(stone.exists(), "the tombstone stays longer");
    exec(&pool, "UPDATE sync_state SET exported_at = '2024-01-01T00:00:00.000Z' WHERE kind = 'tombstone'", &[]).await;
    run(&pool, &root).await;
    assert!(!stone.exists());
}

#[tokio::test]
async fn removing_the_vault_files_also_takes_the_trash_and_the_old_sidecars() {
    let pool = pool().await;
    seed(&pool, 2).await;
    let (_dir, root) = vault();
    super::enable(&pool, root.to_str().unwrap()).await.unwrap();
    super::run(&pool).await.unwrap();
    exec(&pool, "UPDATE transcripts SET transcript = 'Edited.' WHERE id = 't0-0'", &[]).await;
    exec(&pool, "DELETE FROM meetings WHERE id = 'meeting-1'", &[]).await;
    super::run(&pool).await.unwrap();
    let report = super::remove_files(&pool).await.unwrap();
    assert_eq!(report.kept, 0);
    assert!(files(&root).is_empty(), "left: {:?}", files(&root).keys().collect::<Vec<_>>());
}

#[tokio::test]
async fn verification_catches_a_file_that_does_not_match() {
    let (_dir, root) = vault();
    std::fs::write(root.join("quire-format.json"), "{}\n").unwrap();
    let files = vec![("quire-format.json".to_string(), "something else\n".to_string(), "marker")];
    assert!(export::verify(&root, &files).is_err());
}

#[tokio::test]
async fn a_folder_that_is_something_else_or_too_long_is_refused() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    std::fs::write(root.join("quire-format.json"), "{\"format\":\"other\"}").unwrap();
    assert!(export::export(&pool, root.to_str().unwrap()).await.is_err());
    let long = format!("/{}", "a".repeat(export::ROOT_REFUSE_CHARS + 1));
    assert!(export::check_root(&long).unwrap_err().contains("characters long"));
}

#[tokio::test]
async fn an_existing_vault_is_adopted_not_replaced() {
    let pool = pool().await;
    seed(&pool, 1).await;
    let (_dir, root) = vault();
    let marker = format::Marker { format: "quire-vault".into(), version: 1, min_reader: 1, vault_id: "01JC8Z00000000000000000000".into(), created: "x".into(), created_by: "dev-99999999".into() };
    std::fs::write(root.join("quire-format.json"), format::render_marker(&marker)).unwrap();
    assert!(run(&pool, &root).await.verified);
    assert_eq!(store::settings(&pool).await.unwrap().vault_id.as_deref(), Some("01JC8Z00000000000000000000"));
    assert_eq!(std::fs::read_to_string(root.join("quire-format.json")).unwrap(), format::render_marker(&marker));
}

#[tokio::test]
async fn turning_off_leaves_the_files_and_removing_takes_only_the_apps_unchanged_files() {
    let pool = pool().await;
    seed(&pool, 3).await;
    let (_dir, root) = vault();
    super::enable(&pool, root.to_str().unwrap()).await.unwrap();
    assert!(super::run(&pool).await.unwrap().unwrap().verified);
    let before = files(&root).len();
    store::disable(&pool).await.unwrap();
    assert_eq!(super::run(&pool).await.unwrap(), None, "off: nothing runs");
    assert_eq!(files(&root).len(), before, "off: files stay");

    std::fs::write(root.join("mine.txt"), "a file the app did not write").unwrap();
    let edited = files(&root).into_keys().find(|k| k.starts_with("_people/")).unwrap();
    std::fs::write(root.join(&edited), "changed by hand").unwrap();
    let report = super::remove_files(&pool).await.unwrap();
    assert_eq!(report.kept, 1);
    let left: Vec<String> = files(&root).into_keys().collect();
    left.iter().for_each(|k| assert!(k == "mine.txt" || *k == edited || k == "quire-format.json", "{k} should be gone"));
    assert!(left.contains(&"mine.txt".to_string()) && left.contains(&edited));
}

#[tokio::test]
async fn the_vault_and_the_markdown_export_never_share_a_folder() {
    let pool = pool().await;
    let (_dir, root) = vault();
    crate::markdown_export::store::enable(&pool, root.to_str().unwrap()).await.unwrap();
    assert!(super::enable(&pool, root.to_str().unwrap()).await.unwrap_err().contains("Markdown export"));
    let inside = root.join("Vault");
    assert!(super::enable(&pool, inside.to_str().unwrap()).await.is_err());
}

#[tokio::test]
async fn a_folder_another_device_made_is_never_written_into() {
    // The phone made "Thoughts" first; the desktop's own Thoughts folder gets " (2)" (Phase N review)
    let pool = pool().await;
    let (_dir, root) = vault();
    std::fs::create_dir_all(root.join("Thoughts")).unwrap();
    let phones = "{\"quire\":1,\"id\":\"01JC5K3N7P9R2T4V6X8Z0PH0NE\",\"name\":\"Thoughts\",\"created\":\"2026-11-01T09:00:00.000Z\",\"updated\":\"2026-11-01T09:00:00.000Z\",\"updated_by\":\"dev-0a1b2c3d\"}\n";
    std::fs::write(root.join("Thoughts/_folder.json"), phones).unwrap();
    exec(&pool, "INSERT INTO folders (id, name, created_at, updated_at) VALUES ('t1', 'Thoughts', '2026-11-02T10:00:00Z', '2026-11-02T10:00:00Z')", &[]).await;
    exec(&pool, "INSERT INTO meetings (id, title, created_at, updated_at, folder_id) VALUES ('m1', 'Garden app ideas', '2026-11-02T10:00:00Z', '2026-11-02T10:00:00Z', 't1')", &[]).await;
    let report = run(&pool, &root).await;
    assert!(report.verified, "{report:?}");
    assert_eq!(std::fs::read_to_string(root.join("Thoughts/_folder.json")).unwrap(), phones, "the phone's folder is untouched");
    assert!(root.join("Thoughts (2)/_folder.json").exists());
    let notes: Vec<String> = std::fs::read_dir(root.join("Thoughts")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    assert_eq!(notes, vec!["_folder.json".to_string()], "no desktop note in the phone's folder");
}

#[tokio::test]
async fn a_recording_of_thoughts_says_so_in_its_note() {
    let pool = pool().await;
    let (_dir, root) = vault();
    exec(&pool, "INSERT INTO meetings (id, title, created_at, updated_at, kind) VALUES ('m1', 'Garden app ideas', '2026-11-02T10:00:00Z', '2026-11-02T10:00:00Z', 'thoughts')", &[]).await;
    exec(&pool, "INSERT INTO meetings (id, title, created_at, updated_at) VALUES ('m2', 'Bench vote', '2026-11-02T11:00:00Z', '2026-11-02T11:00:00Z')", &[]).await;
    assert!(run(&pool, &root).await.verified);
    let notes: Vec<String> = std::fs::read_dir(&root).unwrap().filter_map(|e| {
        let p = e.unwrap().path();
        (p.extension().is_some_and(|x| x == "md")).then(|| std::fs::read_to_string(p).unwrap())
    }).collect();
    let thoughts = notes.iter().find(|n| n.contains("Garden app ideas")).unwrap();
    assert!(thoughts.contains("\n  bytes: null\nkind: \"thoughts\"\n"), "{thoughts}");
    assert!(!notes.iter().find(|n| n.contains("Bench vote")).unwrap().contains("kind:"));
}
