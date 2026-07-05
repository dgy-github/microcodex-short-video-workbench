# MicrocodeX Short Video Material Workbench

A standalone Windows desktop workbench for:

- Douyin link import
- local video import
- batch material extraction
- editable material packs
- generated-video review
- competitor analysis

This project is intentionally separate from the original `nanocodex` repo.

## Stack

- Rust `1.96.x`
- Tauri `2.8.x`
- Svelte `5`
- Vite `6`
- TypeScript `5`

## Current scaffold status

This repository currently includes:

- project-space bootstrap documents
- a standalone Tauri desktop shell
- business-oriented navigation and page skeleton
- runtime settings storage
- DeepSeek Flash / Pro switching
- Qwen VL fixed-preset settings
- cost pre-estimation API and UI

The following are still placeholders for the next phase:

- Douyin downloader integration
- OCR integration
- ASR integration
- VL batch frame analysis
- real job queue execution
- material pack persistence

## Project structure

```text
docs/
  project-space/
src/
src-tauri/
tests/
resources/
```

## Getting started

```bash
npm install
npm run tauri dev
```

## Windows packaging

To build a single installer that already contains Python, FFmpeg, Playwright,
Chromium, and a clean `douyin-downloader` runtime:

```powershell
.\scripts\prepare_windows_bundle.ps1
npm run tauri:installer
```

For a lighter installer that depends on machine-level Python / FFmpeg /
downloader preparation, you can still run:

```bash
npm run tauri:installer
```

The current NSIS config also embeds the offline WebView2 installer, which makes
USB handoff much more reliable than the default online bootstrapper mode.

## Open-source repository scope

This GitHub repository is intended to stay as a clean source repository.

It does not commit generated local payloads such as:

- `node_modules/`
- `dist/`
- `dist-portable/`
- `src-tauri/target/`
- `bundle-assets/windows-runtime/`

Before building the fully bundled installer or the portable package, generate
the local offline runtime payload on the build machine with:

```powershell
.\scripts\prepare_windows_bundle.ps1
```

## Portable build

To build a green portable folder + zip:

```powershell
npm run tauri:portable
```

This produces a self-contained directory with:

- app executable
- fixed WebView2 runtime
- Python / FFmpeg / Playwright / Chromium
- bundled `douyin-downloader`
- local `data/` directory for settings and jobs

Installer output is expected under:

```text
src-tauri/target/x86_64-pc-windows-gnu/release/bundle/nsis/
```

See [docs/WINDOWS_DEPLOYMENT.md](docs/WINDOWS_DEPLOYMENT.md) for:

- end-user Windows prerequisites
- build machine prerequisites
- fully bundled installer flow
- first-run operator checklist

## Product configuration

Customer runtime settings are stored separately from developer agent config.

Recommended runtime path:

- `%APPDATA%\MicrocodeXShortVideo\settings.json`

## References

- `docs/project-space/`
- `docs/WINDOWS_DEPLOYMENT.md`
- `docs/short-video-agent-design.md`
- `docs/short-video-desktop-spec.md`
