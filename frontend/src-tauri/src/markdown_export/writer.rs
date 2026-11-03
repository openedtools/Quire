//! Writes, moves and removes exported files inside the chosen folder (spec F-MD-4, F-MD-6,
//! F-MD-7). The app only ever changes a file whose bytes are still exactly what it last wrote,
//! unless the person chose "Replace with Quire's version".

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use unicode_normalization::UnicodeNormalization;

use super::render::{self, ExportMeeting};
use super::store;

/// Clashing names get ` (2)` up to this number before the meeting is skipped.
const MAX_CLASH: u32 = 99;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The file was created, updated, moved or renamed.
    Written,
    /// Nothing changed since the last export.
    Unchanged,
    /// The file was edited, moved or removed in another app and was left alone.
    ChangedOutside,
    /// The meeting was deleted and its untouched file removed.
    Removed,
    /// The meeting was deleted and its file, edited elsewhere, was kept.
    Kept,
    /// A meeting with nothing in it, never exported.
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    Clean,
    Edited,
    Missing,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Safe to show in the interface.
    #[error("{0}")]
    Refused(String),
    #[error("file error: {0}")]
    Io(#[from] io::Error),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Checks the chosen folder and returns its canonical path.
pub fn open_root(folder: &str) -> Result<PathBuf, ExportError> {
    let path = Path::new(folder.trim());
    if folder.trim().is_empty() || !path.is_absolute() {
        return Err(ExportError::Refused("Choose a folder on this computer".to_string()));
    }
    let root = fs::canonicalize(path).map_err(|_| ExportError::Refused("That folder could not be found".to_string()))?;
    if !root.is_dir() {
        return Err(ExportError::Refused("That is not a folder".to_string()));
    }
    if root.parent().is_none() {
        return Err(ExportError::Refused("Choose a folder, not the top of a disk".to_string()));
    }
    Ok(root)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A stored relative path as checked components, or None if it could leave the folder.
fn components(rel: &str) -> Option<Vec<&str>> {
    let parts: Vec<&str> = rel.split('/').collect();
    let ok = !parts.is_empty()
        && parts.len() <= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.contains('\\')
                && !p.contains(':')
                && matches!(Path::new(p).components().next(), Some(Component::Normal(_)))
                && Path::new(p).components().count() == 1
        });
    ok.then_some(parts)
}

fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    components(rel).map(|parts| parts.iter().fold(root.to_path_buf(), |p, c| p.join(c)))
}

fn is_link(path: &Path) -> bool {
    fs::symlink_metadata(path).map(|m| m.file_type().is_symlink()).unwrap_or(false)
}

/// Whether the file at `rel` is still what the app wrote. A link or a folder in its place
/// counts as edited, so it is never touched.
pub fn file_state(root: &Path, rel: &str, sha256: &str) -> FileState {
    let Some(path) = resolve(root, rel) else {
        return FileState::Missing;
    };
    if let Some(parent) = path.parent() {
        if parent != root && is_link(parent) {
            return FileState::Edited;
        }
    }
    match fs::symlink_metadata(&path) {
        Err(_) => FileState::Missing,
        Ok(m) if !m.file_type().is_file() => FileState::Edited,
        Ok(_) => match fs::read(&path) {
            Ok(bytes) if sha256_hex(&bytes) == sha256 => FileState::Clean,
            Ok(_) => FileState::Edited,
            Err(_) => FileState::Edited,
        },
    }
}

fn split_rel(rel: &str) -> (Option<&str>, &str) {
    match rel.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, rel),
    }
}

fn join_rel(dir: Option<&str>, name: &str) -> String {
    match dir {
        Some(d) => format!("{d}/{name}"),
        None => name.to_string(),
    }
}

/// Names compared as Windows and macOS compare them: ignoring case and Unicode composition.
fn fold(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

fn same_name(a: &str, b: &str) -> bool {
    fold(a) == fold(b)
}

/// True when `rel` is where a meeting with this folder and stem already belongs, including a
/// ` (n)` name it was given for a clash, so a clash name is not renamed back and forth.
fn same_place(rel: &str, dir: Option<&str>, stem: &str) -> bool {
    let (rel_dir, name) = split_rel(rel);
    let dirs_match = match (rel_dir, dir) {
        (None, None) => true,
        (Some(a), Some(b)) => same_name(a, b),
        _ => false,
    };
    if !dirs_match {
        return false;
    }
    let Some(base) = name.strip_suffix(".md") else {
        return false;
    };
    if same_name(base, stem) {
        return true;
    }
    base.rsplit_once(" (")
        .filter(|(head, tail)| same_name(head, stem) && tail.strip_suffix(')').map_or(false, |n| n.parse::<u32>().is_ok()))
        .is_some()
}

/// The first free name for this stem: not another meeting's file, and nothing on disk with the
/// same name ignoring case (as Windows and macOS compare names), links included.
fn pick_target(root: &Path, dir: Option<&str>, stem: &str, claimed: &[String]) -> Option<String> {
    let dir_path = match dir {
        Some(d) => resolve(root, d)?,
        None => root.to_path_buf(),
    };
    let on_disk: Vec<String> = fs::read_dir(&dir_path)
        .map(|entries| entries.filter_map(|e| e.ok()).map(|e| fold(&e.file_name().to_string_lossy())).collect())
        .unwrap_or_default();
    (1..=MAX_CLASH).find_map(|n| {
        let name = render::file_name(stem, n);
        let rel = join_rel(dir, &name);
        let free = !claimed.iter().any(|c| same_name(c, &rel)) && !on_disk.contains(&fold(&name));
        free.then_some(rel)
    })
}

/// The meeting's folder inside the export folder, created if needed. A link in its place is
/// refused, so nothing is ever written outside the chosen folder.
fn ensure_dir(root: &Path, dir: Option<&str>) -> Result<PathBuf, ExportError> {
    let Some(dir) = dir else {
        return Ok(root.to_path_buf());
    };
    let path = resolve(root, dir).ok_or_else(|| ExportError::Refused("Unusable folder name".to_string()))?;
    match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() => Err(ExportError::Refused("A link is in the way of a folder".to_string())),
        Ok(m) if m.is_dir() => Ok(path),
        Ok(_) => Err(ExportError::Refused("A file is in the way of a folder".to_string())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::create_dir(&path)?;
            Ok(path)
        }
        Err(e) => Err(e.into()),
    }
}

/// Writes through a hidden staging file in the same folder, then moves it into place, so
/// Obsidian never sees half a file. With `replace` false the move fails rather than replace a
/// file that appeared since the name was chosen.
fn write_atomic(path: &Path, bytes: &[u8], replace: bool) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no parent"))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("export.md");
    let staging = dir.join(format!(".{name}.quire-part"));
    if let Ok(m) = fs::symlink_metadata(&staging) {
        if !m.file_type().is_file() {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, "staging name taken"));
        }
        fs::remove_file(&staging)?;
    }
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&staging)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if replace {
            fs::rename(&staging, path)
        } else {
            // A hard link never replaces an existing file
            fs::hard_link(&staging, path)?;
            fs::remove_file(&staging)
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

/// Removes the app's file at `rel`, but only a plain file inside a real folder of the export
/// folder: never through a link put in place of a folder.
fn remove_owned(root: &Path, rel: &str) -> io::Result<()> {
    let Some(path) = resolve(root, rel) else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        if parent != root {
            match fs::symlink_metadata(parent) {
                Ok(m) if m.is_dir() && !m.file_type().is_symlink() => {}
                _ => return Ok(()),
            }
        }
    }
    match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_file() => fs::remove_file(&path),
        _ => Ok(()),
    }
}

/// Brings one meeting's file up to date. `replace` is the person's explicit choice to put
/// Quire's version back over a file changed in another app.
pub async fn export_meeting(pool: &SqlitePool, root: &Path, meeting_id: &str, replace: bool) -> Result<Outcome, ExportError> {
    let row = store::row(pool, meeting_id).await?;
    let Some(meeting) = store::load_meeting(pool, meeting_id).await? else {
        let Some(row) = row else {
            return Ok(Outcome::Skipped);
        };
        let outcome = match file_state(root, &row.rel_path, &row.sha256) {
            FileState::Clean => {
                remove_owned(root, &row.rel_path)?;
                Outcome::Removed
            }
            FileState::Edited => Outcome::Kept,
            FileState::Missing => Outcome::Removed,
        };
        store::delete_row(pool, meeting_id).await?;
        return Ok(outcome);
    };
    write_meeting(pool, root, &meeting, row, replace).await
}

async fn write_meeting(
    pool: &SqlitePool,
    root: &Path,
    meeting: &ExportMeeting,
    row: Option<store::ExportRow>,
    replace: bool,
) -> Result<Outcome, ExportError> {
    if row.is_none() && meeting.is_empty() {
        return Ok(Outcome::Skipped);
    }
    let bytes = render::render(meeting).into_bytes();
    let sha = sha256_hex(&bytes);
    let dir = render::folder_dir(meeting);
    let stem = render::file_stem(meeting);

    let current = match &row {
        Some(row) => {
            let state = file_state(root, &row.rel_path, &row.sha256);
            if state != FileState::Clean && !replace {
                return Ok(Outcome::ChangedOutside);
            }
            Some((row.rel_path.clone(), state))
        }
        None => None,
    };

    if let Some((rel, state)) = &current {
        if same_place(rel, dir.as_deref(), &stem) && resolve(root, rel).is_some() {
            if *state == FileState::Clean && row.as_ref().map_or(false, |r| r.sha256 == sha) {
                return Ok(Outcome::Unchanged);
            }
            let dir_path = ensure_dir(root, split_rel(rel).0)?;
            let path = dir_path.join(split_rel(rel).1);
            if is_link(&path) {
                return Err(ExportError::Refused("A link is in the way of a file".to_string()));
            }
            write_atomic(&path, &bytes, path.exists())?;
            store::put_row(pool, &meeting.id, rel, &sha).await?;
            return Ok(Outcome::Written);
        }
    }

    let claimed: Vec<String> = store::rows(pool)
        .await?
        .into_iter()
        .filter(|r| r.meeting_id != meeting.id)
        .map(|r| r.rel_path)
        .collect();
    ensure_dir(root, dir.as_deref())?;
    let target = pick_target(root, dir.as_deref(), &stem, &claimed)
        .ok_or_else(|| ExportError::Refused("Too many files with the same name".to_string()))?;
    let path = resolve(root, &target).ok_or_else(|| ExportError::Refused("Unusable file name".to_string()))?;
    write_atomic(&path, &bytes, false)?;
    store::put_row(pool, &meeting.id, &target, &sha).await?;

    if let Some((old, _)) = current {
        if !same_name(&old, &target) {
            remove_owned(root, &old)?;
        }
    }
    Ok(Outcome::Written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn meeting(pool: &SqlitePool, id: &str, title: &str, notes: &str) {
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(id)
            .bind(title)
            .bind("2026-11-02T14:05:00Z")
            .bind(Utc::now().to_rfc3339())
            .execute(pool)
            .await
            .unwrap();
        set_notes(pool, id, notes).await;
    }

    async fn set_notes(pool: &SqlitePool, id: &str, notes: &str) {
        sqlx::query("INSERT OR REPLACE INTO meeting_user_notes (meeting_id, body_blocks, body_markdown, updated_at) VALUES (?, '[]', ?, 'x')")
            .bind(id)
            .bind(notes)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn rename(pool: &SqlitePool, id: &str, title: &str) {
        sqlx::query("UPDATE meetings SET title = ? WHERE id = ?").bind(title).bind(id).execute(pool).await.unwrap();
    }

    async fn file_into(pool: &SqlitePool, id: &str, folder: Option<&str>) {
        let folder_id = match folder {
            Some(name) => {
                let fid = format!("f-{name}");
                sqlx::query("INSERT OR IGNORE INTO folders (id, name, created_at, updated_at) VALUES (?, ?, 'x', 'x')")
                    .bind(&fid)
                    .bind(name)
                    .execute(pool)
                    .await
                    .unwrap();
                Some(fid)
            }
            None => None,
        };
        sqlx::query("UPDATE meetings SET folder_id = ? WHERE id = ?").bind(folder_id).bind(id).execute(pool).await.unwrap();
    }

    async fn rel(pool: &SqlitePool, id: &str) -> String {
        store::row(pool, id).await.unwrap().unwrap().rel_path
    }

    /// Every file under `dir`, as relative paths, sorted.
    fn listing(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        for entry in walk(dir) {
            out.push(entry.strip_prefix(dir).unwrap().to_string_lossy().replace('\\', "/"));
        }
        out.sort();
        out
    }

    fn walk(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for e in fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if fs::symlink_metadata(&p).unwrap().is_dir() {
                out.extend(walk(&p));
            } else {
                out.push(p);
            }
        }
        out
    }

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("vault");
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        (tmp, root)
    }

    #[tokio::test]
    async fn writes_once_then_is_unchanged() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Weekly planning", "- venue").await;
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Written);
        let rel = rel(&pool, "m1").await;
        assert!(rel.ends_with(" Weekly planning.md") && !rel.contains('/'), "{rel}");
        let text = fs::read_to_string(root.join(&rel)).unwrap();
        assert!(text.contains("## My notes\n\n- venue\n"));
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Unchanged);
        assert_eq!(listing(&root), vec![rel], "no staging file left behind");
    }

    #[tokio::test]
    async fn content_change_updates_the_same_file() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "one").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let first = rel(&pool, "m1").await;
        set_notes(&pool, "m1", "two").await;
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Written);
        assert_eq!(rel(&pool, "m1").await, first);
        assert!(fs::read_to_string(root.join(&first)).unwrap().contains("\ntwo\n"));
    }

    #[tokio::test]
    async fn title_and_folder_changes_move_the_one_file() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "x").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        rename(&pool, "m1", "Budget").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let renamed = rel(&pool, "m1").await;
        assert!(renamed.ends_with(" Budget.md"));
        assert_eq!(listing(&root), vec![renamed.clone()]);

        file_into(&pool, "m1", Some("Work: Q4")).await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let moved = rel(&pool, "m1").await;
        assert!(moved.starts_with("Work Q4/") && moved.ends_with(" Budget.md"), "{moved}");
        assert_eq!(listing(&root), vec![moved]);

        file_into(&pool, "m1", None).await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        assert_eq!(listing(&root), vec![renamed]);
    }

    #[tokio::test]
    async fn edited_files_are_never_overwritten_until_replace() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "mine").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let first = rel(&pool, "m1").await;
        fs::write(root.join(&first), "edited in Obsidian").unwrap();

        set_notes(&pool, "m1", "newer").await;
        rename(&pool, "m1", "Renamed").await;
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::ChangedOutside);
        assert_eq!(fs::read_to_string(root.join(&first)).unwrap(), "edited in Obsidian");
        assert_eq!(listing(&root), vec![first.clone()]);

        assert_eq!(export_meeting(&pool, &root, "m1", true).await.unwrap(), Outcome::Written);
        let now = rel(&pool, "m1").await;
        assert!(now.ends_with(" Renamed.md"));
        assert_eq!(listing(&root), vec![now.clone()], "replace puts Quire's version in place of the edited file");
        assert!(fs::read_to_string(root.join(&now)).unwrap().contains("newer"));
    }

    #[tokio::test]
    async fn a_removed_file_is_not_recreated_until_replace() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "x").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let first = rel(&pool, "m1").await;
        fs::remove_file(root.join(&first)).unwrap();
        assert_eq!(file_state(&root, &first, "x"), FileState::Missing);
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::ChangedOutside);
        assert!(listing(&root).is_empty());
        assert_eq!(export_meeting(&pool, &root, "m1", true).await.unwrap(), Outcome::Written);
        assert_eq!(listing(&root), vec![first]);
    }

    #[tokio::test]
    async fn deleting_a_meeting_removes_only_an_untouched_file() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "x").await;
        meeting(&pool, "m2", "Other", "y").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        export_meeting(&pool, &root, "m2", false).await.unwrap();
        let edited = rel(&pool, "m2").await;
        fs::write(root.join(&edited), "mine now").unwrap();

        for id in ["m1", "m2"] {
            crate::database::repositories::meeting::MeetingsRepository::delete_meeting(&pool, id).await.unwrap();
        }
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Removed);
        assert_eq!(export_meeting(&pool, &root, "m2", false).await.unwrap(), Outcome::Kept);
        assert_eq!(listing(&root), vec![edited]);
        assert!(store::rows(&pool).await.unwrap().is_empty());
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Skipped);
    }

    #[tokio::test]
    async fn clashes_get_numbers_and_foreign_files_are_untouched() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "a").await;
        meeting(&pool, "m2", "plan", "b").await;
        let stem = render::file_stem(&store::load_meeting(&pool, "m1").await.unwrap().unwrap());
        fs::write(root.join(format!("{stem}.md")), "someone else's note").unwrap();

        export_meeting(&pool, &root, "m1", false).await.unwrap();
        export_meeting(&pool, &root, "m2", false).await.unwrap();
        assert_eq!(rel(&pool, "m1").await, format!("{stem} (2).md"));
        assert!(rel(&pool, "m2").await.ends_with(" plan (3).md"));
        assert_eq!(fs::read_to_string(root.join(format!("{stem}.md"))).unwrap(), "someone else's note");

        // A clash name is kept on the next export, not renamed back and forth
        set_notes(&pool, "m1", "a2").await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        assert_eq!(rel(&pool, "m1").await, format!("{stem} (2).md"));
    }

    #[tokio::test]
    async fn empty_meetings_are_not_exported() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Meeting, 2 Nov 2026 14:05", "").await;
        assert_eq!(export_meeting(&pool, &root, "m1", false).await.unwrap(), Outcome::Skipped);
        assert!(listing(&root).is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn links_are_never_followed() {
        let (tmp, root) = setup();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "x").await;
        file_into(&pool, "m1", Some("Work")).await;
        std::os::unix::fs::symlink(&outside, root.join("Work")).unwrap();
        assert!(matches!(export_meeting(&pool, &root, "m1", false).await, Err(ExportError::Refused(_))));
        assert!(fs::read_dir(&outside).unwrap().next().is_none(), "nothing written through the link");

        // A link where the file would go is not overwritten; the next free name is used
        fs::remove_file(root.join("Work")).unwrap();
        file_into(&pool, "m1", None).await;
        let stem = render::file_stem(&store::load_meeting(&pool, "m1").await.unwrap().unwrap());
        let target = outside.join("target.md");
        fs::write(&target, "outside").unwrap();
        std::os::unix::fs::symlink(&target, root.join(format!("{stem}.md"))).unwrap();
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        assert_eq!(rel(&pool, "m1").await, format!("{stem} (2).md"));
        assert_eq!(fs::read_to_string(&target).unwrap(), "outside");
    }

    #[test]
    fn stored_paths_cannot_leave_the_folder() {
        for bad in ["../x.md", "/etc/x.md", "a/../../x.md", "a/b/c.md", "", "C:x.md", "a\\..\\x.md", "./x.md"] {
            assert!(components(bad).is_none(), "{bad}");
        }
        assert!(components("Work/2026-11-02 1405 Plan.md").is_some());
    }

    #[test]
    fn the_root_must_be_a_real_folder() {
        let (_t, root) = setup();
        assert!(open_root("relative/path").is_err());
        assert!(open_root("").is_err());
        assert!(open_root(&root.join("missing").to_string_lossy()).is_err());
        fs::write(root.join("f.md"), "x").unwrap();
        assert!(open_root(&root.join("f.md").to_string_lossy()).is_err());
        #[cfg(unix)]
        assert!(open_root("/").is_err());
        assert_eq!(open_root(&root.to_string_lossy()).unwrap(), root);
    }

    #[test]
    fn same_place_understands_clash_names() {
        assert!(same_place("A (2).md", None, "A"));
        assert!(same_place("w/a.md", Some("W"), "A"));
        assert!(!same_place("A (x).md", None, "A"));
        assert!(!same_place("w/A.md", None, "A"));
        assert!(same_place("A (2).md", None, "A (2)"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn replace_never_deletes_through_a_linked_folder() {
        let (tmp, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Plan", "x").await;
        file_into(&pool, "m1", Some("Work")).await;
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        let first = rel(&pool, "m1").await;
        let name = first.rsplit_once('/').unwrap().1.to_string();

        // The folder is swapped for a link to a folder holding a file of the same name
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join(&name), "not Quire's").unwrap();
        fs::remove_dir_all(root.join("Work")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("Work")).unwrap();

        file_into(&pool, "m1", None).await;
        let _ = export_meeting(&pool, &root, "m1", true).await;
        assert_eq!(fs::read_to_string(outside.join(&name)).unwrap(), "not Quire's");
    }

    #[tokio::test]
    async fn a_foreign_file_with_a_decomposed_name_is_not_overwritten() {
        let (_t, root) = setup();
        let pool = pool().await;
        meeting(&pool, "m1", "Caf\u{e9}", "x").await;
        let stem = render::file_stem(&store::load_meeting(&pool, "m1").await.unwrap().unwrap());
        let decomposed: String = stem.replace('\u{e9}', "e\u{301}");
        fs::write(root.join(format!("{decomposed}.md")), "someone else's").unwrap();
        export_meeting(&pool, &root, "m1", false).await.unwrap();
        assert_eq!(rel(&pool, "m1").await, format!("{stem} (2).md"));
        let foreign = fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| fs::read_to_string(e.path()).unwrap())
            .filter(|t| t == "someone else's")
            .count();
        assert_eq!(foreign, 1, "the other file is still there, unchanged");
    }
}
