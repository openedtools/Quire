# Your to-do list (kept up to date as the work goes on)

Things only you can do, collected from every phase. Tick them off in any order within each group; the groups are in the order that unblocks the most.

Last updated: 2026-11-03, after Phase N and the clean history.

## 1. Decisions waiting for you
- [x] **PN6, the GitHub history:** done. GitHub is one commit now; the old history is backed up in `~/Developer/backups` on the Mac.
- [ ] **On the PC:** delete the old Quire folder and clone again (`git clone https://github.com/openedtools/Quire.git`). A `git pull` will not work after the rewrite.

## 2. TestFlight set-up (a browser on the Mac, about 15 minutes)
- [ ] developer.apple.com, Certificates, IDs & Profiles, Identifiers, **+**, App ID, App: bundle ID `com.openedtools.notes`, description "Quire" (skip if it is already listed).
- [ ] appstoreconnect.apple.com, Apps, **+**, New App: iOS, name Quire (or "Quire Notes" if taken), bundle ID `com.openedtools.notes`, SKU `quire-ios`.
- [ ] App Store Connect, Users and Access, Integrations, App Store Connect API, Team Keys, **+**: name "Quire upload", access App Manager. Download the `.p8` (only once) and put it in `~/.config/quire/` on the Mac.
- [ ] Send me the Key ID, the Issuer ID and your Team ID (developer.apple.com, Membership). Never paste what is inside the `.p8`.
- [ ] After I upload: add yourself as an internal tester in App Store Connect, TestFlight, and install Quire from the TestFlight app.

## 3. iPhone tests (once it is on your phone)
- [ ] Choose the `Quire` vault folder in iCloud Drive; your desktop meetings, folders and people appear, read-only.
- [ ] Download the speech model on Wi-Fi. Record a short made-up conversation with the screen locked for a while; lines keep coming. Try Transcript only. Import a voice memo.
- [ ] Record one test with retention Off and one with 24 hours; both reach the `Quire` folder in the Files app, and the Off one says "Audio deleted".
- [ ] With Apple Intelligence on: the summary appears, the meeting gets a name in your format (Settings, Meeting names), one Ask question with a time you can tap. Then turn Apple Intelligence off and check the app says so.
- [ ] A one-hour made-up talk (a podcast playing nearby is enough) with the screen locked: note the battery used and whether the phone warmed.
- [ ] The record button's rings pulse; with Reduce Motion on they stay still.
- [ ] The tabs are Home, Meetings and People. Meetings shows folders first; search finds a meeting by name.
- [ ] Make a folder in Meetings (top right), rename it with a long press, and move one of your phone's meetings into it (the meeting's More menu).
- [ ] The second time, one tap on Record starts recording at once; a long press or Recording options shows the choices.
- [ ] Record my thoughts for several minutes with the screen locked: no consent reminder; afterwards the Prompt tab has one clear prompt, with Copy, and Copy on the transcript.
- [ ] Settings sections are short; More about this opens the details.

## 4. Desktop tests (Windows PC and Mac, latest release)
- [ ] Install the newest release from GitHub (`Quire-windows-x64-setup.exe` on the PC).
- [ ] Settings, Models: "Which model suits this computer" names a model for your PC; run Test speed with Gemma 3 1B and note the estimate.
- [ ] Record a short made-up conversation: the first line appears soon after your first pause; keep talking without pausing for 20 seconds and a line still appears. A second recording starts transcribing at once.
- [ ] The meeting is named in your format (Settings, Models, Meeting names). Rename one by hand; it stays. In the rename box, Suggest a name fills in a suggestion.
- [ ] Edit a transcript line, then Undo it. Run Check on a meeting: accept one suggestion, edit one, skip one.
- [ ] The look: hills on the home screen and while recording, the waveform follows your voice, Copy on My notes and on an Ask answer, in light and dark.
- [ ] Record my thoughts (under the record button): no consent reminder; the recording lands in a Thoughts folder; the meeting's summary button says Make it a prompt.
- [ ] Settings sections are short; More about this opens the details.
- [ ] From Phase K: Settings shows the daily audio check while retention is not Keep; Task Scheduler lists it; after a day with the app closed, an expired recording's audio is gone.

## Notes
- The screenshots I take use made-up meetings only; none of your real meetings are opened.
