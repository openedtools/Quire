<div align="center">

<img src="brand/source/mark.webp" width="96" alt="" />

# Quire

Meeting notes that stay on your computer.

**[Download for Mac (Apple silicon)](https://github.com/openedtools/Quire/releases/latest/download/Quire-mac-arm64.dmg)** · **[Download for Windows (64-bit)](https://github.com/openedtools/Quire/releases/latest/download/Quire-windows-x64-setup.exe)** · [All releases](https://github.com/openedtools/Quire/releases)

</div>

Quire is a private, local-first meeting recorder for the desktop. It records your microphone and your computer's audio, transcribes on your own computer, and keeps your notes, summaries and transcripts in a database on that computer. Nothing leaves it unless you choose a cloud model or a synced folder.

## What it does

* **Transcribes locally** with Parakeet or Whisper models that you download once.
* **My notes**: type your own notes before, during and after a meeting; summaries build on them.
* **Summaries and Ask**: summarise a meeting or ask questions about it with a built-in local model, Ollama or LM Studio. Answers cite the times they came from.
* **Speakers and people**: detect who spoke, name them once, and find every meeting a person was in.
* **Folders** to organise meetings, and **New meeting** to start taking notes before you record.
* **Markdown export**: every meeting as a readable `.md` file in a folder you choose, for Obsidian or another notes app. Files you edit there are never overwritten.
* A calm paper look in light and dark.

## Installing

The installers are not code-signed yet.

* **Mac**: open the `.dmg` and drag Quire to Applications. The first time, right-click Quire, choose Open, then Open again.
* **Windows**: run the setup file. If SmartScreen warns, choose More info, then Run anyway.

Each release lists its changes and SHA-256 checksums.

## Use responsibly

Quire is for personal use. Do not record anything you are not allowed to record or keep on a personal computer. Recording and transcription rules differ by place, and some apply even when no audio is kept: tell people when you record them, get a clear yes, and follow your organisation's policy. This is not legal advice.

## Status

Built in phases, each with its own spec, plan, tests and independent review (`intent/private-meeting-recorder/`):

| Phase | Content | State |
|---|---|---|
| A | Own identity, data folder, paper look, clean-up | Done |
| D | My notes, Ask, notes-first meetings | Done |
| M | Markdown export for Obsidian | Done |
| B | Audio retention, transcript-only mode, consent reminder | Planned |
| C | Models screen, cloud switch, key storage | Planned |
| E, F | Shared vault with the iPhone app, then two-way sync | Planned |

## Building from source

See [CLAUDE.md](CLAUDE.md) for the commands and the Windows, Linux and macOS setup. `frontend/scripts/release.sh` builds both installers on a Mac and publishes a release.

## Licence

See [LICENSE.md](LICENSE.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md). The installers include FFmpeg as a separate program under the GNU GPL; each release carries its licence, source code and notes.
