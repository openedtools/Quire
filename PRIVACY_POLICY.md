# Quire Privacy Policy

Quire is a personal meeting recorder. It records, transcribes and summarises meetings on your own computer. This page says what it does with your data, based on the code in this repository.

## No usage statistics

Quire collects no usage statistics, analytics or crash reports, and has no account or sign-in. There is no update check: the app never contacts a server to look for new versions.

## What stays on your computer

* **Recordings** are saved as audio files in the folder you choose in Settings.
* **Transcripts, summaries, people, speakers and folders** are kept in a local database in the app's data folder (on Windows, `%APPDATA%\com.openedtools.notes`).
* **Transcription** runs on your computer with a local speech model (Parakeet or Whisper). Audio is never sent anywhere for transcription.
* **Settings** are stored in the same data folder.

Nothing is encrypted by the app itself. Your data is protected by your computer's account and disk protection, the same as your other files.

## When Quire uses the network

Quire contacts another computer only in these cases:

* **Downloading a model**, when you ask for one. Speech and summary models come from Hugging Face (`huggingface.co`). Only the file request is sent, and each Parakeet speech model file is checked against a fixed SHA-256 before it is used.
* **Summaries with a cloud provider**, only if you choose one in Settings (OpenAI, Anthropic Claude, Groq, OpenRouter, or a custom address). The transcript of the meeting you summarise is sent to that provider and is then covered by their privacy policy. With the built-in model or a local Ollama, summaries are made on your computer.
* **Local model servers** such as Ollama (`localhost:11434`) or LM Studio. These run on your own computer.
* **Links you click**, which open in your web browser.

## Deleting your data

Delete a meeting in the app to remove it and its audio. Uninstalling with "Delete app data" ticked removes the app's data folder. Recordings in a folder you chose are not removed automatically.
