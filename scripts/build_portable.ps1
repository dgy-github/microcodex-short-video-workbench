param(
  [string]$PortableRoot = "",
  [string]$WebView2RuntimeDir = "",
  [string]$BundleRuntimeDir = "",
  [string]$RustTarget = "x86_64-pc-windows-gnu",
  [switch]$SkipBundlePrepare,
  [switch]$SkipZip
)

$ErrorActionPreference = "Stop"

function Invoke-Robocopy {
  param(
    [string]$Source,
    [string]$Target,
    [string[]]$ExcludeDirs = @(),
    [string[]]$ExcludeFiles = @()
  )

  New-Item -ItemType Directory -Force -Path $Target | Out-Null

  $args = @($Source, $Target, "/E", "/NFL", "/NDL", "/NJH", "/NJS", "/NP", "/R:1", "/W:1")
  if ($ExcludeDirs.Count -gt 0) {
    $args += "/XD"
    $args += $ExcludeDirs
  }
  if ($ExcludeFiles.Count -gt 0) {
    $args += "/XF"
    $args += $ExcludeFiles
  }

  & robocopy @args | Out-Null
  if ($LASTEXITCODE -gt 7) {
    throw "robocopy failed with exit code $LASTEXITCODE for $Source"
  }
}

function Remove-PathSafe([string]$Path) {
  if (-not (Test-Path $Path)) {
    return
  }

  $item = Get-Item -LiteralPath $Path -Force
  if ($item.PSIsContainer) {
    cmd /c rmdir /s /q "$Path" | Out-Null
    if (Test-Path $Path) {
      throw "Failed to remove directory: $Path"
    }
  } else {
    Remove-Item -LiteralPath $Path -Force
  }
}

function Test-BundleRuntimeReady([string]$Path) {
  return (Test-Path (Join-Path $Path "python\python.exe")) -and
    (Test-Path (Join-Path $Path "ffmpeg\bin\ffmpeg.exe")) -and
    (Test-Path (Join-Path $Path "playwright-browsers")) -and
    (Test-Path (Join-Path $Path "douyin-downloader"))
}

function Resolve-WebView2RuntimeDir([string]$Preferred) {
  if ($Preferred) {
    if (-not (Test-Path (Join-Path $Preferred "msedgewebview2.exe"))) {
      throw "WebView2 runtime folder must contain msedgewebview2.exe: $Preferred"
    }
    return (Resolve-Path $Preferred).Path
  }

  $roots = @(
    "C:\Program Files (x86)\Microsoft\EdgeWebView\Application",
    "C:\Program Files\Microsoft\EdgeWebView\Application"
  )

  $candidates = foreach ($root in $roots) {
    if (-not (Test-Path $root)) { continue }
    Get-ChildItem $root -Directory |
      Where-Object { Test-Path (Join-Path $_.FullName "msedgewebview2.exe") } |
      Sort-Object Name -Descending
  }

  $selected = $candidates | Select-Object -First 1
  if (-not $selected) {
    throw "Could not locate a local WebView2 runtime folder. Supply -WebView2RuntimeDir explicitly."
  }
  return $selected.FullName
}

function Prepare-FixedRuntimeBuildPath([string]$Source, [string]$Target) {
  Remove-PathSafe $Target
  try {
    New-Item -ItemType Junction -Path $Target -Target $Source -ErrorAction Stop | Out-Null
  } catch {
    Invoke-Robocopy -Source $Source -Target $Target
  }
}

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $BundleRuntimeDir) {
  $BundleRuntimeDir = Join-Path $ProjectRoot "bundle-assets\windows-runtime"
}
if (-not $PortableRoot) {
  $PortableRoot = Join-Path $ProjectRoot "dist-portable\mcx-portable"
}

$resolvedBundleRuntimeDir = [System.IO.Path]::GetFullPath($BundleRuntimeDir)
$resolvedPortableRoot = [System.IO.Path]::GetFullPath($PortableRoot)
$zipPath = Join-Path (Split-Path $resolvedPortableRoot -Parent) "mcx-portable_0.1.0-alpha.5_x64.zip"
$buildFixedRuntimePath = Join-Path $ProjectRoot "src-tauri\Microsoft.WebView2.FixedVersionRuntime"

if (-not (Test-BundleRuntimeReady $resolvedBundleRuntimeDir)) {
  if ($SkipBundlePrepare) {
    throw "Bundle runtime is incomplete at $resolvedBundleRuntimeDir"
  }
  & powershell -ExecutionPolicy Bypass -File (Join-Path $ProjectRoot "scripts\prepare_windows_bundle.ps1")
  if (-not (Test-BundleRuntimeReady $resolvedBundleRuntimeDir)) {
    throw "Bundle runtime is still incomplete after preparation: $resolvedBundleRuntimeDir"
  }
}

$resolvedWebView2RuntimeDir = Resolve-WebView2RuntimeDir $WebView2RuntimeDir
Prepare-FixedRuntimeBuildPath -Source $resolvedWebView2RuntimeDir -Target $buildFixedRuntimePath

Push-Location $ProjectRoot
try {
  & npx tauri build --target $RustTarget --no-bundle -c src-tauri/tauri.portable.conf.json
} finally {
  Pop-Location
  Remove-PathSafe $buildFixedRuntimePath
}

$builtExe = Join-Path $ProjectRoot ("src-tauri\target\{0}\release\microcodex-short-video-workbench.exe" -f $RustTarget)
$builtFixedRuntimeDir = Join-Path $ProjectRoot ("src-tauri\target\{0}\release\Microsoft.WebView2.FixedVersionRuntime" -f $RustTarget)
if (-not (Test-Path $builtExe)) {
  throw "Built executable not found: $builtExe"
}

if (Test-Path $resolvedPortableRoot) {
  Remove-PathSafe $resolvedPortableRoot
}
New-Item -ItemType Directory -Force -Path $resolvedPortableRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $resolvedPortableRoot "data") | Out-Null

$portableExe = Join-Path $resolvedPortableRoot "MicrocodeX Short Video Workbench.exe"
Copy-Item $builtExe $portableExe -Force
Set-Content (Join-Path $resolvedPortableRoot "portable.mode") "portable=1" -Encoding ASCII

Invoke-Robocopy -Source $resolvedBundleRuntimeDir -Target (Join-Path $resolvedPortableRoot "bundle\windows-runtime")
if (Test-Path $builtFixedRuntimeDir) {
  Invoke-Robocopy -Source $builtFixedRuntimeDir -Target (Join-Path $resolvedPortableRoot "Microsoft.WebView2.FixedVersionRuntime")
} else {
  Invoke-Robocopy -Source $resolvedWebView2RuntimeDir -Target (Join-Path $resolvedPortableRoot "Microsoft.WebView2.FixedVersionRuntime")
}

@"
@echo off
setlocal
set "MICROCODEX_PORTABLE_ROOT=%~dp0"
start "" "%~dp0MicrocodeX Short Video Workbench.exe"
"@ | Set-Content (Join-Path $resolvedPortableRoot "Start MicrocodeX Portable.bat") -Encoding ASCII

@"
MicrocodeX Short Video Workbench Portable

1. Double-click Start MicrocodeX Portable.bat
2. On first launch, fill in the DeepSeek text key
3. Fill in the Qwen VL / ASR key
4. Complete at least one Douyin cookie login

Notes:
- Settings, jobs, and logs are written to the local data\ folder
- This portable build already includes Python, FFmpeg, Playwright, Chromium,
  and a fixed WebView2 runtime
- Node.js, Rust, and Cargo are not required on the operator machine
"@ | Set-Content (Join-Path $resolvedPortableRoot "README-Portable.txt") -Encoding UTF8

if ((Test-Path $zipPath) -and -not $SkipZip) {
  Remove-PathSafe $zipPath
}
if (-not $SkipZip) {
  Compress-Archive -Path $resolvedPortableRoot -DestinationPath $zipPath -CompressionLevel Optimal
}

$folderSize = (Get-ChildItem $resolvedPortableRoot -Recurse -File | Measure-Object Length -Sum).Sum
Write-Host ""
Write-Host "Portable folder ready:"
Write-Host "  $resolvedPortableRoot"
Write-Host "Size:"
Write-Host ("  {0} GB" -f [math]::Round($folderSize / 1GB, 3))
if (-not $SkipZip) {
  Write-Host "ZIP:"
  Write-Host "  $zipPath"
}
