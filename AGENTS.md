# AGENTS.md — HeliLingo v2.5

Guide for AI agents (and people) working in this folder. Read it fully before
changing anything. Task list: [TODO.md](TODO.md). Design inventory and tokens:
[docs/design.md](docs/design.md). `CLAUDE.md` only points here.

## What this is

**HeliLingo v2.5** — a DeepL-class translator for Windows: select text in any
app, press a shortcut, and the translation **streams** into a bubble next to the
selection. Written from scratch in Rust + Tauri 2 + Svelte 5.

- **Ultra-fast async pipeline**: every stage (capture → detect → cache →
  route → stream → render) is async, cancellable, and starts as early as it can.
- **Translation streaming**: text is shown as it arrives, never "wait for the
  whole answer".
- **Hybrid operation**: classic web translators (Google, Yandex, DeepL, Bing),
  cloud LLMs through **OpenRouter**, and offline models on the user's PC
  (OPUS-MT, Argos, NLLB through CTranslate2; TranslateGemma and Granite Vision
  through llama.cpp).
- **Ships as one `.exe`.** No installer.

**No older version is a reference** (decided 2026-10-14). Don't read, port or
cite earlier HeliLingo code. Design every feature from the Figma file, the
user's words and current documentation found on the internet.

## Golden rules

1. **When unsure — stop and ask the user.** Do not guess product decisions,
   design details, model choices, or anything paid. Look things up on the
   internet first (docs, crates.io, GitHub, Hugging Face); if still unsure, ask.
2. **Design comes from Figma.** File `HeliLingo — редизайн`, key
   `6KVEWhFnxuyuMjFQQDYiOT`
   (<https://www.figma.com/design/6KVEWhFnxuyuMjFQQDYiOT>). Before building a
   screen, open its frame with the Figma connector (node ids in
   `docs/design.md`; screenshots of section 4 in `docs/design/s4/`) and compare
   a screenshot of the real window with it. Screens missing from Figma are
   composed from the Figma components and tokens; show screenshots to the user.
3. **Font: Golos Text** everywhere (400 / 500 / 600), bundled as woff2 — no
   network fonts at runtime.
4. **Three themes**: Синяя (navy, periwinkle accent, default), Тёмная (dark,
   amber), Белая (light, amber), plus Системная. All colours come from the
   tokens in `docs/design.md`; no hard-coded colours in components.
5. **Tauri 2.x, not 3.** Pin 2.x crates and `@tauri-apps/*` 2.x packages.
6. **Never block the UI thread or the Tokio runtime.** CPU-heavy or blocking
   work (CTranslate2, lingua on big texts, redb writes, Win32 calls that wait)
   goes to `spawn_blocking`, a dedicated thread, or a worker process.
7. **Offline is opt-in.** Until the user turns offline models on in Settings →
   Офлайн и ускорение, nothing local is downloaded, started or loaded; even
   then a model downloads only when the user presses «Скачать».
8. **Secrets** (OpenRouter, DeepL, Google Cloud, Yandex Cloud, Azure keys) live
   in Windows Credential Manager, never in config files, logs, history, or the
   frontend's storage.
9. **The user works on the same PC while you test.** No synthetic Ctrl+C /
   Ctrl+A / clipboard writes on their desktop, don't overwrite their clipboard,
   don't change their autostart or system settings. **Never kill their running
   `helilingo.exe`**: to rebuild a locked exe, rename it aside first; for debug
   checks use `cargo check`. Ask before large downloads (models > ~200 MB).
10. Version-control actions (git init, commits, pushes, releases) only when the
    user asks. This folder is not a git repo yet.
11. **Previews look finished.** Every settings choice that changes how
    something looks (bubble style, Quick layout, text size, opacity…) shows a
    live preview drawn with the real components — the Figma file doesn't
    design these previews, make them neat yourself.
12. **Bubbles are styles.** The double-Ctrl bubble has several looks (Figma
    section 4 plus the full card); Settings → «Баблы и Вики» previews each and
    the user picks. Quick translate layouts are chosen the same way. There is
    no icon next to the selection.
13. **Features waiting for approval** (marked ⏸ in TODO.md) are not built and
    not described here until the user approves them.

## Stack (pinned majors, checked 2026-10-14)

| Area | Choice | Version |
|---|---|---|
| Language | Rust, edition 2024 | rustc 1.98 |
| Async runtime | `tokio` (rt-multi-thread, macros, sync, time, process) | 1.53 |
| HTTP | `reqwest` (`stream`, `json`, native-tls, `http2`, `gzip`) | 0.13 |
| SSE parsing | `eventsource-stream` | 0.2 |
| Memory cache (L1) | `moka` (`future` feature) | 0.12 |
| Disk store (L2) | `redb` | 4.x |
| Language detection | `lingua`, 10 languages (en ru uk de fr es it pt pl tr) + script rules | 1.8 |
| Classic providers | Google, Yandex, DeepL, Bing (keyless web endpoints or the user's key) | — |
| Cloud LLM gateway | OpenRouter (`/api/v1/chat/completions`, streaming) | — |
| Offline seq2seq | `ct2rs` (CTranslate2, Ruy CPU; CUDA feature) + `sentencepiece` | 0.10 / 0.13 |
| Offline GGUF | `llama-server` (llama.cpp release build, ≥ b9534) as a child process | — |
| Windows | `windows` crate | 0.62 |
| Desktop shell | `tauri` 2 + `tauri-build` 2 | 2.12 / 2.7 |
| Tauri plugins | global-shortcut, single-instance, autostart (2.x) | 2.x |
| Frontend | **Svelte 5 + Vite + TypeScript** | — |
| Logging | `tracing` + `tracing-subscriber` | 0.1 |

`ct2rs` builds CTranslate2 from source: needs CMake (path in
`.cargo/config.toml`) and the static CRT (`+crt-static`, also there). Long
target paths break the CMake build — keep `CARGO_TARGET_DIR` short.

Before adding or bumping a dependency, check its current version and
changelog. Prefer few dependencies.

## Layout

```
HeliLingo v2.5/
  AGENTS.md  CLAUDE.md  TODO.md  docs/design.md
  Cargo.toml                 # workspace
  crates/
    hl-core/                 # types, hedged router, requests, text/code splitting, prompts, glossary
    hl-cache/                # moka L1 + redb L2, history, statistics
    hl-detect/               # lingua (10 languages) + script rules
    hl-pipeline/             # one translation end to end (+ CLI example)
    hl-providers/            # google, yandex, deepl, bing, openrouter (SSE), wiki
    hl-offline/              # catalog, downloads, ct2 workers, llama-server, Granite OCR
    hl-docs/                 # file translation: DOCX/PPTX/XLSX/TXT/SRT units in, rebuilt file out
    hl-win/                  # hooks, UIA + clipboard capture, focus, OCR, PDF pages, dialogs, screen capture, network
  src-tauri/                 # the Tauri app: commands, channels, windows, tray, plugins
  vscode/                    # VS Code extension (talks to the app's local API, see below)
  ui/                        # Svelte 5 + Vite frontend
    src/lib/                 # components (lib/ui), stores, ipc.ts, tokens.css, i18n
    src/windows/             # popup, quick, tray, translator, settings, ultra, history, welcome, capture
```

Crates other than `src-tauri` must not depend on Tauri, so the pipeline can be
tested headless (`cargo run -p hl-pipeline --example translate -- "text" ru`,
`cargo run -p hl-offline --example offline -- translate opus "Hello" ru`).

## Pipeline

```
hotkey / hook / tray / IPC
  └─ capture selection (hl-win: UI Automation first, clipboard fallback + restore)
      └─ normalize (programmer-mode split, Ultra fragments)
          └─ detect locally (hl-detect) → choose the target; engines still get "auto"
              └─ cache: moka (RAM) → redb (disk)    ── hit → Done at once
                  └─ router: hedged engine chain (hl-core)
                      ├─ classic: Google / Yandex / DeepL / Bing (one response)
                      ├─ OpenRouter: stream=true, SSE deltas
                      └─ offline: ct2 worker / llama-server (SSE)
                          └─ events → Tauri Channel → Svelte window (16 ms coalescing)
                              └─ on finish: cache, history, stats, context memory
```

Rules:

- **One request id per translation**; a new request from the same surface
  cancels the previous one; closing the window cancels. Cancelling an HTTP
  stream = dropping the response.
- **Events** (`hl_core::TranslateEvent`): `Planned`, `Started`, `Delta`,
  `Replace`, `Done{text, alternatives, dict, …}`, `Error`. Through
  `tauri::ipc::Channel`, not global events.
- **SSE** (OpenRouter, llama-server): `eventsource-stream` over
  `bytes_stream()`; ignore comments; errors can come mid-stream; stop at
  `[DONE]`.
- **Cache key** = hash of (normalized text, source, target, engine, prompt
  version). With «Переводить с контекстом» on, the cache isn't read (the same
  text may translate differently).
- **Dictionary data**: Google and Yandex return groups by part of speech
  (`DictGroup`), shown in word bubbles.

### Hybrid routing (decided)

1. Engine 0 starts at once.
2. The next engine starts when every running one has failed, **or** none has
   produced text within the hedge delay (600 ms, ×2 after that; capped by
   «Время ожидания ответа»).
3. The **first engine to emit text wins**; the others are cancelled. If the
   winner fails mid-stream, the next takes over and the UI gets `Replace`.
4. «Переключаться при ошибке» off → engine 0 only.
5. Offline models join after the cloud engines when switched on; without a
   network (`hl_win::net::online`) the chain skips cloud engines at once.
6. Rate-limited / bad-key engines are benched (1 min doubling to 30 min).
   Google's per-endpoint cool-downs survive restarts (`google-cooldown.json`).

### Providers

- Google: keyless (API, site, dictionary, mobile endpoints staggered) or Cloud
  key; mode Auto (key, falling back to keyless) / Web / API only.
- Yandex: keyless mobile-app endpoint + dictionary (started in parallel using
  the local language guess), or Yandex Cloud key + folder id.
- Bing: keyless through the bing.com/translator session (IG + key + token,
  ~1 h), or Azure key + region.
- DeepL: key only (Free `:fx` vs Pro host); earlier fragments go in `context`.
- OpenRouter: key + model (no default model; picker from `/api/v1/models`,
  cached a day), SSE, glossary and context in the prompt.

## Offline models (hl-offline)

Opt-in (rule 7). Fastest first; all pinned to Hugging Face commits with
SHA-256 (checked 2026-10-14):

| Model | What | Download | Runtime |
|---|---|---|---|
| OPUS-MT | en ⇄ ru, CT2 int8 | 157 MB | ct2 worker |
| Argos | en ⇄ ru packages 1.9 | ~350 MB | ct2 worker |
| NLLB 600M | 200 languages, CT2 int8 (CC-BY-NC) | ~600 MB | ct2 worker |
| NLLB 1.3B | 200 languages, CT2 int8 (CC-BY-NC) | ~1.3 GB | ct2 worker |
| TranslateGemma 4B | GGUF Q4_K_M, context + glossary | 2.3 GB | llama-server |
| TranslateGemma 12B | GGUF Q4_K_M | 6.8 GB | llama-server |
| Granite Vision 4.1 | IBM, reads text from screenshots (English-trained) | 3.0 GB + runtime | llama-server `--mmproj` |

- CTranslate2 runs in a **worker process** (`helilingo.exe --offline-worker
  <spec>`): unloading = ending the worker.
- llama-server: started on first use (127.0.0.1, free port, `/health`), Job
  Object, stopped after 15 idle minutes; TranslateGemma via raw `/completion`
  in its own template, streamed. The two Gemma sizes never run together.
- llama.cpp build chosen for the PC: CUDA 13 / 12.4 for NVIDIA, Vulkan for
  other cards, else CPU (GitHub release assets with SHA-256 digests).
- Any model beyond this list — ask the user first.

## Windows integration (hl-win)

- Double Ctrl and double Ctrl+C: `WH_KEYBOARD_LL`; mouse side buttons:
  `WH_MOUSE_LL`; both on one hook thread with a watchdog. Chords:
  `tauri-plugin-global-shortcut`.
- Selection: UI Automation Text pattern first (`hl_win::uia`, on its own COM
  thread, 250 ms), then synthetic Ctrl+C with the clipboard restored.
- The bubble must **not steal focus** (`WS_EX_NOACTIVATE`, shown without
  activating); Esc / Ctrl+C / click-away come through the hook.
- Skip shortcuts while a fullscreen app or an ignored app is in front.
- OCR: `Windows.Media.Ocr` (default) or Granite Vision (offline, optional).
- «Скриншот области»: an overlay window on the monitor under the cursor, GDI
  copy of the rectangle, PNG through WinRT's encoder, then the translator's
  Images tab.

## Frontend (ui/)

- Svelte 5 (runes), TypeScript strict, Vite. No UI kits; components follow the
  Figma "Элементы интерфейса" sheet. Tokens as CSS variables per theme
  (`[data-theme="blue|dark|light"]`).
- Quick, popup and tray are created hidden at start; the others on first use.
- All IPC through typed wrappers in `ui/src/lib/ipc.ts`. Outside the app
  (plain browser) `ipc.ts` answers with canned data, so windows can be
  screenshotted with headless Edge: `quick.html?theme=light&demo=<text>`,
  `popup.html?demo=cat&wiki=1`, `settings.html?page=offline`,
  `capture.html?demo`.
- Icons: Figma SVGs in `ui/src/assets/icons/`, colour → `currentColor`.
  Restart `vite` after adding an icon, or it renders blank.
- **Check components against Figma before using them.**
- **Screenshot after every large block of UI changes** (the real rendered
  window) and check: icons straight (glyph, centred, size, stroke, not blurry
  or cropped); Golos Text actually loaded, right weight/size, no clipped
  descenders, no ellipsis where text should fit, Cyrillic and Latin; layout
  matches the Figma frame in every theme; and say honestly what is still off.
  Check at the real window width (settings: 800 px) — notes must not wrap
  where Figma has one line.
- Interface languages: **only Russian (default) and English**. Every visible
  string lives in `ui/src/lib/i18n.svelte.ts` (window titles in
  `src-tauri/src/windows.rs`).

## Editor API and the VS Code extension

- `src-tauri/src/api.rs`: HTTP on 127.0.0.1 only, random port, random token, both in
  `%LOCALAPPDATA%\HeliLingo\api.json` (removed on exit). `GET /status`,
  `POST /translate {text, source?, target?, engine?}` through the normal pipeline (the
  «already in the target language → second language» rule included). Every request needs
  `Authorization: Bearer <token>`. Setting `editorApi` (on by default).
- `vscode/`: TypeScript, bundled with esbuild (`npm run build`), packaged with vsce
  (`npm run package` → `helilingo.vsix`), `npm test` for the comment finder. The extension
  reads `api.json` on every call (the port changes per start).
- `helilingo.exe --capture [window]` starts the screen-area overlay in the running app.

## Updates (prepared for GitHub `Helisy420/HeliLingo`)

The repo isn't ready yet; everything must switch on by changing config only.

- Tauri's updater plugin is **not** used (it installs NSIS/MSI only). A small
  self-updater in `src-tauri/src/update.rs`: GitHub releases API → compare
  semver → download `helilingo.exe` + `.minisig` → verify with
  `minisign-verify` against the embedded public key → rename the running exe
  aside, move the new one in, restart.
- `UPDATES_ENABLED = false` until the repo has releases; the About page then
  says updates aren't set up yet.
- Release recipe goes to `docs/release.md` when implemented. Publishing only
  as the user's account, no co-author lines.

## Performance targets (confirmed 2026-10-14)

- Hotkey → bubble visible: ≤ 50 ms.
- Cache hit → full text shown: ≤ 10 ms after the bubble.
- Our overhead before the first network byte / first token: ≤ 5 ms.
- Idle memory: as low as WebView2 allows; nothing offline loaded unless used.

## Build & check

```bash
cd ui && npm install && npm run check && npm run build
cargo clippy --workspace --all-targets
cargo test --workspace
cargo tauri build --no-bundle      # target/release/helilingo.exe
```

The release output is a **single `helilingo.exe`** (frontend embedded). It
relies on the WebView2 runtime that ships with Windows 10/11. Offline runtimes
and models are downloaded later from Settings, only if the user wants them.

Before saying a task is done: it builds, clippy is clean, tests pass, and UI
work was compared against its Figma frame with a screenshot of the real window.

## Data

- `%APPDATA%\HeliLingo\` — settings.json, helilingo.redb (cache, history,
  stats), glossary.tsv.
- `%LOCALAPPDATA%\HeliLingo\` — logs, models (default), google-cooldown.json,
  openrouter-models.json.
- No translated text or keys in logs.
