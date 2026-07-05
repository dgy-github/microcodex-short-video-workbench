param(
  [string]$DownloaderDir = "D:\agent_prac\douyin-downloader",
  [string]$PythonDir = "",
  [string]$PlaywrightDir = "",
  [string]$BundleDir = "",
  [switch]$KeepExisting
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

function Resolve-PythonDir {
  if ($PythonDir) {
    return $PythonDir
  }
  return (& python -c "import sys; print(sys.prefix)").Trim()
}

$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
if (-not $BundleDir) {
  $BundleDir = Join-Path $ProjectRoot "bundle-assets\windows-runtime"
}
if (-not $PlaywrightDir) {
  $PlaywrightDir = Join-Path $env:LOCALAPPDATA "ms-playwright"
}

$PythonDir = Resolve-PythonDir
$ffmpegPath = (Get-Command ffmpeg -ErrorAction Stop).Source
$ffprobePath = (Get-Command ffprobe -ErrorAction Stop).Source

if (-not (Test-Path $DownloaderDir)) {
  throw "Downloader directory not found: $DownloaderDir"
}
if (-not (Test-Path $PythonDir)) {
  throw "Python directory not found: $PythonDir"
}
if (-not (Test-Path $PlaywrightDir)) {
  throw "Playwright browser directory not found: $PlaywrightDir"
}

$resolvedBundleDir = [System.IO.Path]::GetFullPath($BundleDir)
$expectedPrefix = [System.IO.Path]::GetFullPath((Join-Path $ProjectRoot "bundle-assets"))
if (-not $resolvedBundleDir.StartsWith($expectedPrefix, [System.StringComparison]::OrdinalIgnoreCase)) {
  throw "BundleDir must stay under $expectedPrefix"
}

if ((Test-Path $resolvedBundleDir) -and -not $KeepExisting) {
  Remove-Item -LiteralPath $resolvedBundleDir -Recurse -Force
}

New-Item -ItemType Directory -Force -Path $resolvedBundleDir | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $resolvedBundleDir "ffmpeg\bin") | Out-Null

Copy-Item $ffmpegPath (Join-Path $resolvedBundleDir "ffmpeg\bin\ffmpeg.exe") -Force
Copy-Item $ffprobePath (Join-Path $resolvedBundleDir "ffmpeg\bin\ffprobe.exe") -Force

Invoke-Robocopy -Source $PythonDir -Target (Join-Path $resolvedBundleDir "python") -ExcludeDirs @(
  "Doc",
  "share",
  "include",
  "Scripts",
  "site-packages",
  "__pycache__"
) -ExcludeFiles @("*.pyc", "*.pyo")

$targetSitePackages = Join-Path $resolvedBundleDir "python\Lib\site-packages"
New-Item -ItemType Directory -Force -Path $targetSitePackages | Out-Null

$tempSitePackages = Join-Path $env:TEMP "mcx-short-video-bundle-site-packages"
if (Test-Path $tempSitePackages) {
  Remove-Item -LiteralPath $tempSitePackages -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $tempSitePackages | Out-Null

$pythonExe = Join-Path $PythonDir "python.exe"
& $pythonExe -m pip install --disable-pip-version-check --no-compile --target $tempSitePackages pip setuptools wheel
& $pythonExe -m pip install --disable-pip-version-check --no-compile --target $tempSitePackages -r (Join-Path $DownloaderDir "requirements.txt")
& $pythonExe -m pip install --disable-pip-version-check --no-compile --target $tempSitePackages playwright

Invoke-Robocopy -Source $tempSitePackages -Target $targetSitePackages -ExcludeDirs @("__pycache__") -ExcludeFiles @("*.pyc", "*.pyo")
Remove-Item -LiteralPath $tempSitePackages -Recurse -Force

New-Item -ItemType Directory -Force -Path (Join-Path $resolvedBundleDir "playwright-browsers") | Out-Null
Get-ChildItem $PlaywrightDir -Directory |
  Where-Object { $_.Name -like "chromium-*" -or $_.Name -like "chromium_headless_shell-*" -or $_.Name -like "ffmpeg-*" -or $_.Name -like "winldd-*" } |
  ForEach-Object {
    Invoke-Robocopy -Source $_.FullName -Target (Join-Path $resolvedBundleDir "playwright-browsers\$($_.Name)")
  }

Invoke-Robocopy -Source $DownloaderDir -Target (Join-Path $resolvedBundleDir "douyin-downloader") -ExcludeDirs @(
  ".git",
  "Downloaded",
  "__pycache__",
  ".venv",
  ".pytest_cache",
  ".mypy_cache"
) -ExcludeFiles @(
  ".cookies.json",
  "dy_downloader.db",
  "*.pyc",
  "*.pyo"
)

$bundledDownloaderDir = Join-Path $resolvedBundleDir "douyin-downloader"
$bundledConfigPath = Join-Path $bundledDownloaderDir "config.yml"
if (Test-Path $bundledConfigPath) {
  Remove-Item $bundledConfigPath -Force
}

$manifest = [ordered]@{
  generatedAt = (Get-Date).ToString("s")
  projectRoot = $ProjectRoot
  bundleDir = $resolvedBundleDir
  downloaderDir = (Resolve-Path $DownloaderDir).Path
  pythonDir = (Resolve-Path $PythonDir).Path
  playwrightDir = (Resolve-Path $PlaywrightDir).Path
  ffmpegPath = $ffmpegPath
  ffprobePath = $ffprobePath
}
$manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $resolvedBundleDir "bundle-manifest.json") -Encoding UTF8

Write-Host ""
Write-Host "Prepared offline Windows bundle:"
Write-Host "  $resolvedBundleDir"
Write-Host ""
Write-Host "Next:"
Write-Host "  npm run tauri:installer"
