# HeliLingo 2.5

**Select text anywhere. Double-tap Ctrl. Read the translation.**
A fast translator for Windows that lives in the tray, streams the translation as it arrives, and can also run fully offline.

[Русский](README.ru.md) · [Features](#features) · [Shortcuts](#shortcuts) · [Providers](#providers) · [Offline models](#offline-models) · [Build](#build-from-source)

![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-7589db?style=flat-square)
![Rust](https://img.shields.io/badge/Rust-2024-7589db?style=flat-square&logo=rust&logoColor=white)
![Tauri 2](https://img.shields.io/badge/Tauri-2-7589db?style=flat-square)
![Svelte 5](https://img.shields.io/badge/Svelte-5-7589db?style=flat-square)
![Version](https://img.shields.io/badge/version-2.5.0-7589db?style=flat-square)

HeliLingo 2.5 is a rewrite. It is a single `helilingo.exe` with no installer. The
2.0 source has been removed from this repository; it stays in the git history.

## Features

- **Translate in place.** Select text in any app and double-tap **Ctrl**. A bubble
  appears next to the selection. It never takes focus, so the selection stays intact.
- **Streaming.** The text appears while the engine is still writing it.
- **Quick window** (**Ctrl+Shift+L**): type, and the translation follows as you type.
- **Translator window** (**Ctrl+Alt+T**): full text, history, alternatives.
- **Ultra mode** (**Ctrl+Shift+S**): a selection in several languages is split into
  fragments, each detected and translated on its own.
- **Screen and image translation** (**Ctrl+Shift+O** for a screen area): text is read
  with Windows OCR, or with an optional offline vision model.
- **Replace or copy.** Swap the selected text for its translation in the source app,
  or copy the translation.
- **Word bubbles.** Dictionary entries grouped by part of speech, with alternatives
  and a Wikipedia summary.
- **Hybrid routing.** Several engines run in order. The first one that produces text
  wins; if it fails midway, the next one takes over.
- **Three themes** (navy, dark, light, or the system theme) and Russian or English
  interface. Font: Golos Text, bundled.
- **Programmer mode.** In source code only string literals and comments are
  translated; the code stays byte for byte.
- **Glossary** and **context**: earlier fragments help the translation stay
  consistent.
- **VS Code extension** (see [VS Code](#vs-code-extension)).
- **Private by design.** API keys are stored in Windows Credential Manager. History
  and statistics stay on your PC and can be turned off.

## Providers

| Engine | Needs |
|---|---|
| Google | nothing (keyless), or a Cloud key |
| Yandex | nothing (keyless), or a Yandex Cloud key and folder id |
| Bing | nothing (keyless), or an Azure key and region |
| DeepL | a DeepL key (Free or Pro) |
| OpenRouter | an OpenRouter key and a model you choose from the list |
| Offline models | a one-time download, see below |

Offline is **opt-in**. Until you turn offline models on in *Settings → Offline and
acceleration*, nothing local is downloaded, started or loaded, and even then a model
downloads only when you press **Download**.

## Offline models

| Model | What it is | Download | Runs on |
|---|---|---|---|
| OPUS-MT | en ⇄ ru, fastest | ~157 MB | CPU, CTranslate2 |
| Argos | en ⇄ ru packages | ~350 MB | CPU, CTranslate2 |
| NLLB 600M / 1.3B | 200 languages (CC-BY-NC licence) | ~0.6 / ~1.3 GB | CPU or GPU, CTranslate2 |
| TranslateGemma 4B / 12B | LLM translation with context and glossary | 2.3 / 6.8 GB | CPU or GPU, llama.cpp |
| Granite Vision 4.1 | reads text from screenshots (English-trained) | 3.0 GB + runtime | llama.cpp |

Models are pinned to Hugging Face commits and checked by SHA-256. Downloads resume
after an interruption.

## Shortcuts

Defaults; every shortcut can be changed in *Settings → Hotkeys*.

| Action | Shortcut |
|---|---|
| Translate the selection | double **Ctrl** |
| Quick window | **Ctrl+Shift+L** |
| Translator window | **Ctrl+Alt+T** |
| Ultra mode | **Ctrl+Shift+S** |
| Translate a screen area | **Ctrl+Shift+O** |
| Copy the translation | **Ctrl+Shift+C** |
| Translate and paste over the selection | **Ctrl+Alt+R** |

Mouse buttons 4 and 5 and chords with modifiers can be recorded as shortcuts too.

## VS Code extension

The extension in [`vscode/`](vscode) talks to the running app over `127.0.0.1` with a
secret key the app writes to `%LOCALAPPDATA%\HeliLingo\api.json`. It translates the
selection or the comment and string under the cursor, and can replace a selection in
place. The app's local API is on by default and can be switched off in *Settings →
Context and modes*.

## Updates

The About page can check the latest version against `version.json` in this
repository. The automatic update flow is built but **off** in this version; it is
turned on once signed releases exist.

## Build from source

Requirements:

- Windows 10 or 11
- Rust (edition 2024) with the MSVC toolchain
- Node.js (for the Svelte UI)
- CMake from the Visual Studio Build Tools (needed by `ct2rs`, the CTranslate2 binding).
  Edit the path in [`.cargo/config.toml`](.cargo/config.toml) if yours differs.
- Keep the build path short: CMake fails on very long target paths.

```bash
cd ui && npm install && npm run check && npm run build && cd ..
cargo tauri build --no-bundle
```

The exe is written to `target/release/helilingo.exe`. For development checks use
`cargo check`. Core crates can be tested without the UI, for example:

```bash
cargo run -p hl-pipeline --example translate -- "Hello" ru
```

## Layout

```
crates/
  hl-core/       types, hedged router, requests, text and code splitting, prompts, glossary
  hl-cache/      memory cache (moka) + disk store (redb), history, statistics
  hl-detect/     language detection (lingua, 10 languages) + script rules
  hl-pipeline/   one translation end to end
  hl-providers/  Google, Yandex, DeepL, Bing, OpenRouter (streaming), Wikipedia
  hl-offline/    model catalog, downloads, CTranslate2 workers, llama-server, Granite
  hl-docs/       file translation: DOCX / PPTX / XLSX / TXT / SRT
  hl-win/        hooks, UI Automation and clipboard capture, focus, OCR, screen capture
src-tauri/       the Tauri app: commands, windows, tray, plugins
ui/              Svelte 5 + Vite + TypeScript frontend
vscode/          VS Code extension
docs/            design notes
```

Developer notes are in [AGENTS.md](AGENTS.md) and the work list in [TODO.md](TODO.md).

## Privacy

- Translation requests go only to the engine you chose (and to the offline model
  running on your PC).
- Keys are kept in Windows Credential Manager, not in config files.
- History and statistics are local and can be turned off.

## License

MIT. See the license field in [`Cargo.toml`](Cargo.toml) and [`vscode/package.json`](vscode/package.json).
