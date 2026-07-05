# Windows Deployment

This document separates two different concerns:

1. developer packaging prerequisites
2. end-user runtime prerequisites

The app is a Tauri desktop application. A packaged installer is suitable for
operators on Windows and does not require Node.js or Rust on the operator's
machine.

## 1. What the operator needs on Windows

For a normal operator workstation, install or prepare the following:

- Windows 10 or Windows 11 x64
- internet access to:
  - `api.deepseek.com`
  - `dashscope.aliyuncs.com`
  - Douyin endpoints used by the downloader
- Microsoft Edge WebView2 Runtime
  - current bundle config uses Tauri's offline installer mode and can install it without internet
  - installer size will increase accordingly, but this is friendlier for USB / offline delivery
- one of these two runtime delivery modes:
  - fully bundled installer:
    - Python / Playwright / Chromium / FFmpeg / clean `douyin-downloader` are already built into the installer
    - no extra Python or FFmpeg installation is needed on the operator machine
  - light installer:
    - `ffmpeg` available in `PATH`
    - a prepared `douyin-downloader` working directory
    - default expected path: `D:\agent_prac\douyin-downloader`
    - override path with environment variable:
      `MICROCODEX_DOUYIN_DOWNLOADER_DIR`
    - Python environment required by `douyin-downloader`
    - Playwright plus Chromium installed inside the downloader environment
- one-time Douyin cookie login completed in that downloader environment
- API keys entered in the app settings:
  - DeepSeek text key
  - Qwen VL / ASR key

## 2. What the operator does not need

End users do not need these for normal app usage:

- Node.js
- npm
- Rust toolchain
- Cargo
- Tauri CLI

Those are only needed on the machine that builds the installer.

## 3. Recommended operator handoff package

For a smooth customer deployment, hand over these items together:

1. NSIS installer for the app
2. if using the light installer:
   - `ffmpeg` bundle or installer instructions
   - pre-prepared `douyin-downloader` folder
3. short setup note explaining:
   - where to put the downloader folder
   - where to paste DeepSeek key
   - where to paste Qwen key
   - how to run one-time cookie login

## 4. Full bundled installer flow

On the build machine, stage the offline runtime before building NSIS:

```powershell
.\scripts\prepare_windows_bundle.ps1
npm run tauri:installer
```

That staging step copies these into `bundle-assets/windows-runtime/`:

- current local Python runtime
- FFmpeg binaries
- local Playwright Chromium browser cache
- a clean `douyin-downloader` working copy

The application will then:

- detect the bundled runtime on startup
- use bundled Python / FFmpeg / Playwright first
- create a writable downloader workspace under:
  - `%APPDATA%\MicrocodeXShortVideo\bundled-douyin-downloader`
- keep API keys and Douyin cookies out of the installer

Important for the open-source repository:

- `bundle-assets/windows-runtime/` is generated locally and ignored from Git
- `dist/`, `dist-portable/`, and `src-tauri/target/` are also ignored
- rebuild those artifacts on the packaging machine before producing a customer build

## 5. Portable green build

If you want a no-install delivery package that can run directly from a folder or
USB drive, build the portable package:

```powershell
npm run tauri:portable
```

The portable output includes:

- executable
- fixed WebView2 runtime
- bundled Python / FFmpeg / Playwright / Chromium
- bundled clean `douyin-downloader`
- `portable.mode` marker
- local `data/` folder

In portable mode, settings, jobs, logs, and helper scripts are written to the
portable folder's own `data\` directory instead of `%APPDATA%`.

## 6. One-time downloader preparation on the operator machine

The external downloader should be verified before the app is handed over:

1. clone `douyin-downloader`
2. install Python dependencies
3. install Playwright
4. install Chromium
5. create `config.yml`
6. run cookie fetch once and confirm login works
7. test one Douyin URL download successfully

If you prefer a zero-setup handoff, prepare this downloader folder in advance and
ship it together with the desktop app.

## 7. Build machine prerequisites

The Windows machine that creates the installer should have:

- Node.js 20 LTS or newer
- npm
- Rust `1.96.x`
- Tauri CLI `2.8.x`
- MinGW target toolchain for `x86_64-pc-windows-gnu`
- WebView2 available locally for testing
- if building the fully bundled installer:
  - local Python with required downloader packages installed
  - local Playwright Chromium already installed
  - FFmpeg already available on the build machine
  - a sanitized `douyin-downloader` source directory
- if building the portable green package:
  - local WebView2 runtime available under `C:\Program Files (x86)\Microsoft\EdgeWebView\Application\...`
    or provide an explicit fixed runtime path to the portable build script

## 8. Build the installer

From the project root:

```bash
npm install
npm run tauri:installer
```

For the fully bundled build, run the staging script first:

```powershell
.\scripts\prepare_windows_bundle.ps1
npm run tauri:installer
```

Expected output:

```text
src-tauri/target/x86_64-pc-windows-gnu/release/bundle/nsis/
```

The generated `.exe` in that directory is the main Windows installer to give to
the operator.

## 9. First-run checklist for the operator

After installation:

1. open the app
2. fill in DeepSeek text key
3. fill in Qwen VL / ASR key
4. if using the light installer, verify the downloader directory path
5. run one material extraction task
6. confirm:
   - stage log updates
   - material pack is created
   - prompt regeneration works
   - dashboard cost changes after prompt rewrite

## 10. Suggested support policy

For customer delivery, keep these support assumptions explicit:

- the app binary is packaged and stable
- bundled mode removes Python / FFmpeg / downloader preinstallation, but cookie refresh is still an operator task
- API keys are owned by the operator/customer
- WebView2 remains a machine-level prerequisite, handled by the installer bootstrapper
- WebView2 remains a machine-level prerequisite, but the current installer already embeds the offline installer
