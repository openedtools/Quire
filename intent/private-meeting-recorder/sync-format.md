# Shared vault format (version 1)

Status: draft 2, accepted with the spec on 2026-11-01 for planning Phases A to E. It stays a draft until the shared fixtures pass and the device checks in section 15 are done, and two-way sync (Phase F) waits for them. Revised after an independent review. Part of the spec for `intent/private-meeting-recorder/`. Date: 2026-11-01.
Working app name: Quire. The identifiers `quire-vault`, `quire-format.json` and the `quire` key are fixed format identifiers, not brand values. They stay the same even if the app is renamed, because changing them would break every existing vault and the iPhone app. Only the vault root folder name (`Quire` in the examples) comes from the brand config.

This file is the contract between the desktop app and the future iPhone app. Both implement it independently, so it is written as a standalone document. MUST, SHOULD and MAY carry their usual meaning.

Metadata choice: intent.md asks for markdown plus JSON metadata, and the brief allowed JSON or front matter. Notes and people use YAML front matter (a strict subset, section 4) so one file is readable by a person and carries its metadata. Sidecars and folder files are JSON.

## 1. Goals and non-goals

Goals
* Notes, transcripts, people and folders live as plain files that any device can read, with only iCloud Drive between them.
* A human can open the vault in a file browser or text editor and understand it.
* Two devices editing at once never silently lose data, and every device that resolves the same conflict writes the same bytes.
* Deleting audio is a defined, testable operation, and the format says what can and cannot be promised.
* The format can grow without breaking older apps.

Non-goals (version 1)
* Real-time collaboration, merging text edits inside a note, or sharing a vault between different people.
* Syncing voiceprints, model files, API keys or app settings (section 14).
* Nested folders in version 1 writers (section 9 reserves room).

## 2. Vault layout

The vault is one directory chosen by the user. On Windows with iCloud for Windows it is a folder inside the iCloud Drive folder, for example `C:\Users\<name>\iCloudDrive\Quire`. On iPhone it is the matching folder in iCloud Drive.

```
Quire/                                           the vault root
  quire-format.json                              vault marker and version (section 5)
  01JC8ZQ4M3V7K2H9T5R6XWPD1N.md                  an unfiled note
  01JC8ZQ4M3V7K2H9T5R6XWPD1N.9f2c41ab.transcript.json
  Work Meetings/                                 a folder is a real directory
    _folder.json                                 folder identity (section 9)
    01JC90A2T8N4Y6B3D7F1GHKQ5S.md
    01JC90A2T8N4Y6B3D7F1GHKQ5S.3b7e09cd.transcript.json
  _people/                                       one file per person (section 8)
    01JC7M9X2R5T8V1B4D6F3HJKQ0.md
  _audio/                                        audio, only when audio sync is on (section 10)
    01JC90A2T8N4Y6B3D7F1GHKQ5S.m4a
  _trash/                                        deleted notes and tombstones (section 12)
  _conflicts/                                    losing sides of conflicts (section 11)
  _tmp/                                          write staging fallback, always safe to delete
```

Rules
1. Names that start with an underscore are reserved. A user folder name MUST NOT start with an underscore or a dot (the app strips them).
2. The format never uses dot-prefixed names, because iCloud Drive may skip them (to verify, section 15).
3. A note belongs to the folder whose directory contains its file. Notes at the vault root are unfiled. Moving a note is a file move and does not change its name.
4. A reader finds notes by scanning for note files (section 3) outside reserved directories and reading the `id` in the front matter. The `id` is the identity. The path is only a location.
5. A reader MUST ignore everything it does not recognise and MUST NOT delete or move anything it did not create.

## 3. Identifiers, names and text rules

* Every note, person and folder has an immutable ULID: 26 uppercase Crockford base32 characters, the first 10 encoding creation time in milliseconds. Regex: `^[0-9A-HJKMNP-TV-Z]{26}$`. Segment IDs are free strings, unique within a note.
* A note file is `<id>.md`. Its transcript sidecar is `<id>.<sha8>.transcript.json`, where `sha8` is the first 8 lowercase hex characters of the SHA-256 of the sidecar's bytes (section 7). Names never change when the title changes.
* A file is a note file if its name matches `^<ULID>\.md$`. A file whose name starts with a ULID followed by a space, `(`, `_` or `-` and ending in `.md` is a duplicate candidate (section 11). Its `id` MUST equal the 26-character prefix.
* Device ID: `dev-` plus 8 random lowercase hex characters, generated once per install.
* Timestamps are UTC ISO 8601 with milliseconds and a trailing `Z`: `2026-11-01T15:42:10.123Z`. In file names the compact form `YYYYMMDDTHHMMSSmmmZ` is used (19 characters).
* All text files are UTF-8 without a byte order mark, with LF line endings. Readers MUST accept a BOM and CRLF.
* Normalisation: file and directory names, and string values in front matter and JSON, are written in Unicode NFC. The note body is not normalised. Readers and writers preserve the body exactly as stored.
* File names are ASCII except folder directory names (section 9).

## 4. Front matter grammar

Front matter is a strict subset of YAML so that a Rust parser and a Swift parser agree. Implementations MUST parse exactly this grammar and MUST NOT hand the text to a general YAML library without checking it against these rules first.

* The file starts with a line `---` and the front matter ends at the next line that is exactly `---`.
* One entry per line: `key: value`, with keys matching `^[a-z][a-z0-9_]*$`. Nesting uses exactly 2 spaces per level. Lists use `- ` entries at the list's indent. No tabs.
* Values are one of: a double-quoted string, an integer (`-?[0-9]+`), `true`, `false`, `null`, `[]`, or `{}`. Nothing else: no floats written as bare words, no flow style except the empty forms, no anchors, aliases, tags, multi-line strings, merge keys or comments.
* Double-quoted strings use JSON escapes limited to `\"`, `\\`, `\n`, `\r`, `\t` and `\uXXXX`. A writer MUST NOT emit `\/`. A reader MUST reject any other escape.
* A duplicate key at the same level makes the file invalid.
* Writers emit keys in the order documented for each file type, then any unknown keys in their original order, then nothing else. A file that parses and is written back by a canonical writer without change is byte-identical.
* A reader MUST preserve unknown keys and unknown body text exactly, and rewrite a file only when the app changed it. Opening a note never rewrites it.

Size limits enforced by readers: note file 2 MB, transcript sidecar 20 MB, person file 256 KB, folder file 16 KB. A larger file is skipped and reported, never partially read.

## 5. `quire-format.json`

```json
{
  "format": "quire-vault",
  "version": 1,
  "min_reader": 1,
  "vault_id": "01JC8Z0000VAULT00000000000",
  "created": "2026-11-01T14:00:00.000Z",
  "created_by": "dev-7f3a91c2"
}
```

* `format` MUST equal `quire-vault`. A directory without this file is not a vault, and the app offers to create one only when the user asks.
* `version` is the format major version. `min_reader` is the lowest app format level allowed to write.
* `vault_id` identifies the vault. If two copies of this file appear (two devices created a vault at the same time, or an iCloud duplicate), the copy with the lexicographically smaller `vault_id` wins, and the other device adopts it and re-checks its sync state.
* The file is written once. Only a version migration (section 13) rewrites it.

## 6. Note file

Key order for writers: `quire`, `id`, `title`, `created`, `updated`, `updated_by`, `rev`, `started`, `ended`, `duration_s`, `language`, `source`, `mode`, `speakers`, `transcript`, `summary`, `audio`, then unknown keys.

`kind` (optional, added in Phase N, written after `audio` like an unknown key): `"thoughts"` marks a recording of one person's own thoughts, whose `## Summary` section holds a prompt made from the talk rather than meeting notes. Absent means an ordinary meeting. Readers that do not know it keep it, as for any unknown key.

```markdown
---
quire: 1
id: "01JC90A2T8N4Y6B3D7F1GHKQ5S"
title: "Weekly sync with Alex"
created: "2026-11-01T14:30:02.000Z"
updated: "2026-11-01T15:42:10.123Z"
updated_by: "dev-7f3a91c2"
rev: 3
started: "2026-11-01T14:30:02.000Z"
ended: "2026-11-01T15:22:11.000Z"
duration_s: 3129
language: "en"
source: "recorded"
mode: "audio"
speakers:
  - slot: 1
    name: "Alex Rivera"
    person: "01JC7M9X2R5T8V1B4D6F3HJKQ0"
  - slot: 2
    name: "Speaker 2"
    person: null
transcript:
  file: "01JC90A2T8N4Y6B3D7F1GHKQ5S.3b7e09cd.transcript.json"
  sha256: "3b7e09cd0000000000000000000000000000000000000000000000000000abcd"
  segments: 412
summary:
  status: "done"
  generated_at: "2026-11-01T15:25:40.000Z"
  model: "qwen3.5:4b"
audio:
  retention: "d7"
  expires: "2026-11-08T15:22:11.000Z"
  state: "present"
  deleted_at: null
  file: null
  sha256: null
  bytes: null
---

## Summary

Decisions and action items, as markdown.

## Notes

Anything typed by hand, on any device.
```

Field reference

| Field | Type | Meaning |
|---|---|---|
| `quire` | integer | Format level of this file. 1 in version 1. |
| `id` | string | Immutable ULID. MUST equal the 26-character ULID prefix of the file name. |
| `title` | string | Display title. Can change without renaming the file. |
| `created` | timestamp | Never changes. |
| `updated` | timestamp | When a device last changed the content on purpose. Used by conflict handling. |
| `updated_by` | string | Device ID of that device. |
| `rev` | integer | Starts at 1, increases by 1 on every content change and on every conflict resolution. |
| `started`, `ended`, `duration_s` | timestamp, timestamp, number | Recording times. `null` for imported or typed notes. |
| `language` | string or null | BCP 47 code of the transcript language. |
| `source` | `recorded`, `imported` or `typed` | Where the content came from. |
| `mode` | `audio` or `transcript_only` | Whether audio was ever written. |
| `speakers` | list | One entry per speaker slot. `name` is a snapshot of the display name, `person` links to a `_people` file or is null. |
| `transcript` | map or null | `file` is the exact sidecar name, `sha256` the hash of its bytes, `segments` the count. `file` MUST equal `<id>.<first 8 of sha256>.transcript.json`. |
| `summary` | map or null | Status is `none`, `pending`, `done` or `failed`. `model` is the model name only. |
| `audio` | map | Retention state, section 10. |

Body: free markdown. The app writes a `## Summary` heading for generated text. Everything else is user content. Writers preserve body text they did not change, byte for byte. The body is untrusted input (section 14).

## 7. Transcript sidecar

A sidecar holds the segments and is immutable and content-addressed. Its name carries the first 8 hex characters of its own SHA-256, and a changed transcript is written as a new file with a new name. The note then points at the new file. Because no two versions ever share a path, concurrent edits cannot overwrite each other.

```json
{
  "quire": 1,
  "note_id": "01JC90A2T8N4Y6B3D7F1GHKQ5S",
  "segments": [
    { "id": "seg-8d1e", "start": 12.4, "end": 15.9, "speaker": 1, "text": "Let's start with timelines.", "confidence": 0.93 },
    { "id": "seg-8d1f", "start": 16.2, "end": 21.0, "speaker": 2, "text": "Two weeks, maybe three.", "confidence": 0.9 }
  ]
}
```

* `start` and `end` are seconds from the start of the recording. `speaker` is a slot number from the note's `speakers` list, or `null`.
* Segments are sorted by `start`. Unknown keys are preserved.
* Writers write the sidecar first, then the note. The note's `transcript.sha256` is the commit point. A note whose sidecar is missing, or whose bytes do not hash to `transcript.sha256`, is shown as "transcript still syncing" and retried. A sidecar is never trusted without that check.
* Garbage collection: a sidecar that no note file in the vault or in `_conflicts` references, and that is older than 7 days, may be deleted by any device. Speaker renames never touch a sidecar, because segments refer to slots.

## 8. People

`_people/<id>.md`. Key order: `quire`, `id`, `name`, `aliases`, `created`, `updated`, `updated_by`, `rev`, `merged_into`, then unknown keys.

```markdown
---
quire: 1
id: "01JC7M9X2R5T8V1B4D6F3HJKQ0"
name: "Alex Rivera"
aliases:
  - "Alex R"
created: "2026-10-27T18:00:00.000Z"
updated: "2026-10-31T09:12:00.000Z"
updated_by: "dev-7f3a91c2"
rev: 2
merged_into: null
---

Free markdown notes about this person.
```

* Merging B into A: rewrite references from B to A in notes, write `merged_into: "<A id>"` into B, keep B's file for 30 days, then move it to `_trash`.
* Readers follow `merged_into` at most 8 hops and stop at the first repeated ID. A cycle ends at the first person seen twice.
* A `person` ID with no file shows the stored `name` snapshot and does not fail.
* Voiceprints are never part of a person file (section 14).

## 9. Folders

A folder is a real directory under the vault root. Inside it, `_folder.json` carries the identity:

```json
{ "quire": 1, "id": "01JC5K3N7P9R2T4V6X8Z0BDFGH", "name": "Work Meetings",
  "created": "2026-10-28T12:00:00.000Z", "updated": "2026-10-28T12:00:00.000Z", "updated_by": "dev-7f3a91c2" }
```

Directory naming. Apply these steps in order to the display name:
1. Convert to NFC.
2. Remove `< > : " / \ | ? *` and control characters.
3. Remove leading dots, leading underscores, leading and trailing spaces, and trailing dots.
4. Truncate to 80 UTF-16 code units at a scalar boundary, then repeat step 3.
5. If the result is empty, use `Folder`. If it equals a Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`), compared ignoring case and ignoring any extension, add ` folder` after the part before the first dot (`CON` becomes `CON folder`, `nul.txt` becomes `nul folder.txt`). Appending at the end would leave `nul.txt folder`, which Windows still reserves (amended in Phase E).
6. Collision: compare names after Unicode simple case folding of the NFC form (`Work` and `work` are the same directory on Windows and macOS). If a different folder ID already owns that name, the folder with the larger ID gets ` (2)`, then ` (3)`, and so on. The deterministic choice keeps all devices agreeing.

`_folder.json` keeps the real, uncleaned `name`. The directory name is only for humans.

Rename, move, delete
* Rename: rename the directory and update `_folder.json` (`name`, `updated`, `updated_by`). Note files do not change.
* Delete: allowed when the directory contains only `_folder.json`. Otherwise the notes inside are moved to the vault root first.
* A directory with no `_folder.json` is treated as an unidentified folder named after the directory for 10 minutes (the grace period G in section 11). After that a device writes `_folder.json` with a deterministic ID: time part 0, random part the first 80 bits of the SHA-256 of the case-folded NFC directory name. Two devices therefore write the same bytes.
* Moves and renames reach other devices as delete plus add. Section 11 explains how readers avoid treating that as a deletion.

Nesting: the format allows up to 3 levels. Version 1 writers create depth 1 only. Version 1 readers list nested folders and never flatten or move them.

## 10. Audio and retention

Audio sync is off by default. When it is off, audio files stay in each device's own storage and never enter the vault, but every device still enforces the same deadline on its own copy, because the deadline is in the note.

```yaml
audio:
  retention: "d7"                       # off | h24 | d7 | d14 | d30 | keep
  expires: "2026-11-08T15:22:11.000Z"   # null when retention is keep or state is none
  state: "present"                      # none | present | deleted
  deleted_at: null                      # set only when state is deleted
  file: null                            # "_audio/<id>.<ext>" when audio sync is on
  sha256: null                          # hash of the audio file when it is in the vault
  bytes: null                           # size of that file
```

Meaning
* `retention` is the policy chosen at recording time. `expires` is the moment audio becomes due for deletion: the recording end plus the period for h24 to d30, the moment finalization completed for `off`, null for `keep`. The clock starts when the recording (or finalization for `off`) ends, in UTC.
* `state` describes policy, not a guarantee that any device has a copy. `none` means audio was never written. `present` means a copy may exist somewhere. `deleted` is written only when a person explicitly deletes the audio ("Delete audio now" or deleting a meeting).
* Retention `off` and mode `transcript_only` never upload audio, even when audio sync is on.
* Audio path is derived, never read from the note: it is `_audio/<id>.<ext>` where `<id>` is the note's own ID and `<ext>` is one of `m4a`, `mp4`, `wav`, `mp3`, `flac`, `ogg`, `caf`. If `audio.file` is present and differs from a derived path, readers ignore it and report the note. A reader MUST NOT delete any path it did not derive this way.
* Before a vault audio file is played or copied, its size and SHA-256 must equal `bytes` and `sha256`. A partial download is treated as not ready.

Effective state, computed by every device and not written back:
* If `state` is `deleted`, audio is deleted.
* Else if `state` is `none`, there is no audio.
* Else if `expires` is not null and now is at or after `expires`, the audio is expired and the device MUST delete its own copy (and the vault copy, section below).
* Otherwise the audio is retained.

Expiry never rewrites the note. This avoids every online device writing the same change at the same moment and creating a conflict for every expired note. Apps display "Audio deleted on <expires>" for an expired note.

Rules
1. Extending: a person can set a later `expires`, or `keep`, before the effective state becomes expired. The change is a normal content change (section 11). After a device has deleted its copy, extending does not bring the copy back. A device never recreates audio it deleted.
2. Delete now: writes `state: deleted`, `deleted_at: <now>`, `file: null`, `sha256: null`, `bytes: null`. The merge in section 11 treats `deleted` as absorbing.
3. Vault copy: any device that finds `_audio/<id>.*` for a note whose effective state is deleted or expired removes it, whether or not that device has audio sync turned on. A phone that uploaded audio can therefore be cleaned up by the desktop.
4. Validation: `expires` earlier than `ended` is invalid. A note with an invalid or unparseable `expires` is treated as `keep` for deletion purposes and reported. A device never deletes on a clock reading earlier than the last time the app ran (clock went backwards).
5. Mass-expiry guard: a sweep that would delete audio for more than 20 recordings, or more than a quarter of all recordings, pauses and asks for confirmation. This limits damage from a wrong clock or a hostile vault.
6. A device that is off or unused cannot delete anything. Its copy stays until the app next runs. The app says so in its settings text.

What deletion does and does not promise
* The app removes the file from the vault folder and its own storage and verifies it is gone.
* iCloud keeps deleted files in a recovery area for up to 30 days and the user can restore them. An offline device keeps its copy until it reconnects. Backups, snapshots and SSD wear leveling are outside the app's control.
* So "deleted" means "removed from every place this app can reach", not "unrecoverable". The interface says so wherever retention or audio sync is described. This is the main reason audio sync defaults to off.

## 11. Conflicts, duplicates and moves

Definitions
* A note's canonical content is its front matter without `updated`, `updated_by` and `rev`, its body bytes, and the SHA-256 of the sidecar it references.
* A device keeps, for each synced note, the SHA-256 of the file bytes and the `rev` it last saw (the base). A conflict exists when the local copy and the vault copy both differ from the base.
* G, the grace period, is 10 minutes.

The merge function. Every implementation uses exactly this one, for any two copies of one note. It is deterministic, so two devices resolving the same pair write identical bytes.
1. If both copies have equal canonical content, there is no conflict. Keep the copy with the larger tuple in step 2. Nothing is written to `_conflicts`.
2. Otherwise order the copies by the tuple (`updated` as an instant, `rev` as a number, SHA-256 of the file bytes as a lowercase hex string). The larger tuple wins. Compare field by field.
3. The result is the winner's content with: `rev` = the largest `rev` among all copies considered, plus 1; `updated` and `updated_by` unchanged from the winner; and, if any copy has `audio.state` equal to `deleted`, the result has `audio.state: deleted`, `audio.deleted_at` equal to the earliest non-null `deleted_at` among copies, and `audio.file`, `audio.sha256` and `audio.bytes` null. This audio rule is the only field-level merge in version 1.
4. The loser is written in full to `_conflicts/<id>.<updated_by>.<compact updated>.md` where both come from the loser. Its sidecar stays where it is (sidecars are immutable) and remains referenced by the copy in `_conflicts`, so garbage collection keeps it.
5. A resolver whose own copy is the winner and who changes nothing else writes only the bumped `rev`. Because step 3 gives every resolver the same result, simultaneous resolutions converge.

Rules that follow
1. If only the vault copy changed, import it. If only the local copy changed, export it.
2. A person is told about every conflict (a Conflicts count and a banner). They can open the loser or restore it as a new note with a new ID.
3. Clock skew can make the wrong side win. The loser is always preserved, so that costs a click, not data.
4. There is no field-level merge other than the audio rule.

iCloud duplicates. iCloud for Windows and the Files app may produce extra copies such as `<id> 2.md` or `<id> (conflicted copy).md`. A file matching the duplicate pattern in section 3 whose `id` equals the ULID prefix is a copy of that note and is merged with the others by the function above. Its name is never trusted beyond the prefix. On Apple platforms the system can also expose unresolved file versions instead of extra files. The iPhone app MUST read every unresolved version as a copy, merge them, and mark the versions resolved.

Moves, renames and absence. A move or a folder rename arrives as delete plus add, in either order.
* The same ID at two paths with identical bytes is a move in progress. Keep both for G. If both still exist after G, keep the one whose file has the later modification time and move the other to `_conflicts`.
* The same ID at two paths with different bytes is a conflict between copies, merged as above, and the result is written to the path of the winner.
* A note that was synced before and is now missing from every path and from `_trash` is not treated as deleted until it has been missing for G.

## 12. Deleting notes, folders and people

Notes
* Delete: move `<id>.md` into `_trash/<id>/` and write `_trash/<id>/_deleted.json` with `{ "id", "deleted_at", "deleted_by", "folder_at_deletion" }`, where `folder_at_deletion` is the folder's ID, or null for the vault root (the directory name is only for humans). Sidecars follow the normal garbage collection once nothing references them. The vault audio file is removed at delete time.
* A device that finds a tombstone for a note it holds does not erase it at once. It hides the note and keeps its local row for 30 days. If its local copy differs from the base (unsynced edits), it first writes that copy to `_conflicts` so nothing is lost.
* Edit after delete: if a device's copy has an `updated` later than the tombstone's `deleted_at`, the note is restored from `_trash`, `audio.state` is forced to `deleted` (the audio was removed at delete time), and a conflict record is written. Notes prefer keeping data, audio prefers deletion.
* Absence: a device whose sync record has a note that is missing everywhere, with no tombstone (for example, the tombstone expired while the device was offline for a long time), MUST NOT re-export it. It treats the note as deleted elsewhere. If it has unsynced edits it writes them to `_conflicts` instead.
* Retention of deletion records: the note content in `_trash/<id>/` is purged after 30 days. The `_deleted.json` tombstone stays for 365 days. A device offline longer than that is told to rescan from the vault and not to push old notes back without confirmation.

Folders and people
* Folders: section 9. A deleted person gets a tombstone in `_trash` like a note. Notes that referenced the person keep the `name` snapshot with `person: null`.

Audio
* Section 10.

## 13. Versioning and compatibility

* The vault `version` is a single integer. Version 1 is this document.
* Additive changes (new optional keys or files) do not change the version. Old apps ignore and preserve them.
* A change that old apps cannot safely write alongside raises `version` and `min_reader`. An app with a format level below `min_reader` opens the vault read-only and says "Update the app to edit this vault". It MUST NOT write.
* An app with a format level above the vault `version` may offer an upgrade. Upgrading is one way. It first copies the whole vault to `_conflicts/backup-<compact date>/` (or refuses if there is no room), then rewrites `quire-format.json` last.
* Every note and person file carries `quire: <level>` so a stray newer file can be identified.

## 14. Trust, privacy and what never syncs

The vault is untrusted input. Another device, a sync glitch, or a hostile file dropped into the folder can all put bytes there.

Readers MUST
* Validate IDs against the ULID regex and require the ID to match the file name prefix.
* Derive every path they touch (audio, sidecar, folder) from validated IDs and sanitized names. Never use a path taken from file content. Reject paths that are absolute, contain `..` or a drive letter, or resolve outside the vault. Do not follow symbolic links or Windows reparse points, except the cloud-placeholder kind.
* Enforce the grammar and size limits in section 4, and reject duplicate keys.
* Validate dates, `retention` values, numbers and `merged_into` chains before acting on them.
* Render markdown through a sanitizing renderer that does not run raw HTML, scripts or `javascript:` links. The desktop has more than one markdown path (the summary editor and a viewer), and each one MUST pass the hostile-body fixtures.
* On the desktop, vault content reaches the interface only through Tauri commands that return validated data. The interface does not read vault paths itself.
* Treat unparseable or half-written files as "not ready, try again later". Never delete, overwrite or repair a file that could not be parsed.

Never synced
* Voiceprints and speaker embeddings. They are biometric-like, and different apps use different embedding models, so they are not comparable across devices.
* API keys, model files, settings, the retention sweep log, crash-recovery data.
* Audio, unless the user turns audio sync on, and never audio for retention `off` or transcript-only notes.

Data that does sync lands on Apple's servers once iCloud uploads it. Without Advanced Data Protection, Apple holds the encryption keys. With it, iCloud Drive is end-to-end encrypted. The app shows this when sync is first enabled and does not decide for the user.

## 15. Windows and iCloud constraints

Known from Apple documentation and sources checked during design (re-verify on the real devices in Stage 4)
* iCloud for Windows shows iCloud Drive as a folder. Files can be kept on the device ("Always keep on this device") or exist as online-only placeholders that download when opened. A file must be downloaded before another program can read it.
* Files deleted from iCloud Drive stay recoverable for up to 30 days.
* Apple does not publish how iCloud Drive resolves two simultaneous edits of one file. The format assumes extra copies can appear and that delivery is eventual and unordered across files.
* Windows names are case-insensitive, cannot contain `< > : " / \ | ? *`, reserve names such as `CON` and `NUL`, cannot end in a dot or space, and paths are limited to 260 characters unless long paths are enabled.

Design consequences
* Stable ASCII names built from IDs, with titles in front matter, so renames never touch a file name.
* Path budget. The longest relative path the format produces is 132 characters: an 80-unit folder name, a slash, and a note ID, 8-character hash and `.transcript.json` (51 characters). The conflicts path is shorter. The desktop warns when the vault root path is longer than 100 characters and refuses above 120, so the total stays under 260.
* Writes are atomic. Write the new bytes to a staging file on the same volume, flush, then rename into place. Prefer a staging folder outside the vault (the app's own data folder) when it is on the same volume, so the staging file is never uploaded. Otherwise use `_tmp/`. A reader ignores `.part` files and everything in `_tmp`.
* Write order for a changed recording: sidecar first, then the note.
* Hydration: before reading a file the desktop detects an online-only placeholder (the recall-on-data-access attribute), asks for it to download by opening it with a timeout, and shows "waiting for iCloud" rather than treating the file as missing. The setting text recommends "Always keep on this device" for the vault folder.
* Change detection uses a file watcher plus a full rescan at launch, on window focus and every 5 minutes while running. Watchers are unreliable on cloud folders, so the rescan is the source of truth.
* An iOS app container appears in iCloud for Windows only if the iOS app declares it as a public document container. This is an iOS-side decision (section 17). The format does not depend on it.

Still unverified (must be tested on the real PC and iPhone before the desktop ships two-way sync)
* Whether dot-prefixed names are skipped (the format avoids them).
* The placeholder attribute iCloud for Windows sets, and download latency for a 5 MB sidecar.
* What iCloud for Windows does with two simultaneous writes, and how it names the extra copy (the duplicate pattern in section 3 is a guess).
* Whether renaming a directory of 200 files is delivered as one rename or as delete plus add.
* Whether a rename from outside the iCloud folder into it behaves like a normal create (used by staging).

## 16. Migrating the desktop's existing notes

The desktop stores everything in SQLite (spec section 1). Migration into the vault is one way, safe to repeat, and never deletes or changes existing rows except for the new columns.

1. The user turns on sync and picks a folder. The app writes `quire-format.json` only after the user confirms.
2. A new nullable column `sync_id` (ULID) is added to meetings, people and folders. It is filled the first time a row is exported, with the row's `created_at` as the time part, and never changes after that. The old `meeting-<uuid>` ID stays as is and is written into the note as the unknown-to-iOS key `legacy_id`. A column `sync_deleted_at` supports the 30-day local soft delete in section 12.
3. For each meeting: write the sidecar, then the note (front matter from the meetings, speakers and summary tables, body from the summary markdown, written exactly as stored), then record `(sync_id, sha256, rev, path)` in a local `sync_state` table.
4. People become `_people` files and folders become directories with `_folder.json`.
5. Verification: after the export the app re-reads the vault into memory and compares counts and content hashes with the database. A mismatch is reported and nothing is marked as synced.
6. Re-running finds every row in `sync_state` and writes only what changed since the last sync. A body is written back byte-identically unless the person edited it, so a lossy markdown round trip in the editor never rewrites an untouched note.
7. Importing (two-way phase) goes through the same repository functions the interface uses, so name cleaning, invisible-character stripping and uniqueness rules apply to everything that arrives. The desktop's unique-name rules for folders and people are case-insensitive, so a name that collides is imported with a suffix (` (2)`) and the person is offered "possible duplicate, merge?". The device then records the vault file's hash as its base, and exports only when the database content changes after that. Normalising a name on import is not a change to export.
8. Where the truth lives: in version 1 SQLite stays the desktop's working store and the vault is the shared exchange. SQLite rows are authoritative on the desktop until a vault change is imported through section 11. The alternative, files as the only store with SQLite as a rebuildable index, is cleaner but rewrites most of the persistence layer. The spec recommends the first option in phases (export only, then two-way) and asks for a decision at the gate.

## 17. What the iOS app needs to do (informative)

Not a desktop requirement. It lists what the separate iOS project must decide or do.
* Reach the vault: declare a public iCloud Drive container, or let the user pick the folder once with the document picker and keep a security-scoped bookmark. The second does not depend on container visibility.
* Keep a local base copy (last synced hash and rev) for every note. An app that edits the vault file in place cannot otherwise detect that another device changed it.
* Coordinate access with `NSFileCoordinator`, request downloads of online-only files, observe changes with `NSMetadataQuery`, and read and resolve unresolved file versions as section 11 requires.
* Implement sections 3 to 14 exactly, including the grammar, the merge function, preserving unknown content, and the audio rules.
* Run the retention sweep at launch and in a background task, because iOS does not promise timers (from intent.md).

## 18. Conformance fixtures

A shared set of fixture vaults will live in the desktop repository under `docs/vault-fixtures/` and be copied into the iOS project. Each fixture is a folder tree plus an `expected.json`. Both apps must produce the same results.
* Round trip: a canonical vault reads and rewrites byte-identical. A non-canonical but valid file is rewritten only when the app changes it.
* Unknown keys and unknown body text survive a rewrite.
* Grammar: duplicate keys, bad escapes, anchors, tags, tabs and flow style are rejected.
* Merge: each case in section 11, including equal canonical content with different bookkeeping, a three-device merge in all orders (the result bytes are identical), and the audio absorbing rule.
* Duplicates and moves: `<id> 2.md`, same ID at two paths with equal and with different bytes, absent for less than and more than G.
* Delete: tombstone, edit-after-delete restore with audio forced to deleted, absence after tombstone expiry, local unsynced edits saved to `_conflicts`.
* Audio: expiry derived without a rewrite, vault audio removed by a device with audio sync off, `audio.file` pointing at another note's audio (ignored), an `expires` before `ended`, the mass-expiry guard, Off and transcript-only never uploaded.
* Sidecars: concurrent edits produce two sidecars with no overwrite, a missing or mismatching sidecar shows "syncing", garbage collection keeps a sidecar referenced from `_conflicts`.
* Hostile files: path traversal, an alias bomb, an oversized file, an ID that does not match the name prefix, a truncated sidecar, `merged_into` cycles, invisible and bidirectional characters in names, raw HTML and script in bodies through every renderer.
* Version gating: `min_reader` above the app's level opens read-only.
* Windows names: reserved names, trailing and leading dots, long and Unicode folder names, `Work` and `work`, emoji names that exceed 80 UTF-16 units, two folders whose cleaned names collide, and a path budget check.
