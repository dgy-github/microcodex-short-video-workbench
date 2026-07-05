# Bundle Assets

This directory is intentionally kept in the source repository as a staging area
for offline Windows packaging assets.

The generated runtime payload under `bundle-assets/windows-runtime/` is ignored
from Git because it can contain large binary dependencies such as:

- Python runtime
- FFmpeg
- Playwright browser cache
- bundled `douyin-downloader`

Prepare that directory locally before building the fully bundled installer or
portable package:

```powershell
.\scripts\prepare_windows_bundle.ps1
```
