# Vault conformance fixtures

Shared test files for the vault format in `intent/private-meeting-recorder/sync-format.md` (section 18), for the desktop app and the future iPhone app. Every name and meeting here is made up.

* `canonical/`: a small vault written by the desktop's canonical writer. A conforming app reads every file and writes it back byte for byte. The note's `transcript.sha256` matches its sidecar.
* `grammar/invalid/`: note files that break the front matter grammar (section 4). A conforming reader rejects every one.
* `names.json`: folder display names and the directory names they must become (section 9, steps 1 to 5).

The desktop regenerates `canonical/` from its writers with `QUIRE_WRITE_FIXTURES=1 cargo test --lib vault::format`, only after a deliberate format change.
* `naming.json`: automatic meeting names (plan Phase L): which titles count as default names, how a model's reply becomes a topic, and how the name format is filled in.
* `thoughts.json`: thought mode (plan Phase N): the instructions that turn spoken thoughts into a prompt, the default name, the folder, and the note's `kind` key.
