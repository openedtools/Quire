# Third-party notices

Quire includes code used under the MIT licence; that copyright and permission notice is kept in `LICENSE.md`. Quire also uses the components and models below. Models are downloaded only when you ask for them, and each stays under its own licence.

## Speech and speaker models

* **Parakeet TDT 0.6B v3** (NVIDIA), speech recognition. Licence: Creative Commons Attribution 4.0 (CC BY 4.0). Converted to ONNX for this app; downloaded from the original project's model host.
* **Whisper** (OpenAI), speech recognition, through the whisper.cpp model files (`ggerganov/whisper.cpp` on Hugging Face). Licence: MIT.
* **WeSpeaker ResNet34 (VoxCeleb)**, speaker detection, as the ONNX export published by the sherpa-onnx project (k2-fsa). WeSpeaker is licensed under the Apache License 2.0. Confirm the terms of the pretrained model on its model card before redistributing it.
* **Silero VAD**, voice activity detection, through the silero-rs crate. Licence: MIT.

## Summary models (optional downloads)

* **Qwen 3.5** (2B and 4B, GGUF files from `unsloth` on Hugging Face). Licence as stated on the model card (Qwen models have been released under the Apache License 2.0); confirm on the card.
* **Gemma 3** (1B and 4B, GGUF files from `bartowski` on Hugging Face). Subject to the Gemma Terms of Use.

## Engines and libraries

* **ONNX Runtime** 1.22 (Microsoft), bundled with the app. Licence: MIT.
* **whisper.cpp** and **llama.cpp** (ggml), through the whisper-rs and llama-cpp-2 crates. Licence: MIT.
* **FFmpeg** 8.0.1, bundled as a separate program for audio conversion (the "essentials" build). That build is licensed under the GNU General Public License version 3. FFmpeg is a trademark of Fabrice Bellard.
* **Tauri**, **Next.js**, **React**, **BlockNote** and the other open-source packages listed in `frontend/package.json` and `frontend/src-tauri/Cargo.toml`, each under its own licence (mostly MIT or Apache 2.0).

## Fonts

* **Source Serif 4** and **Source Sans 3** (Adobe), bundled through the fontsource packages. Licence: SIL Open Font License 1.1.
