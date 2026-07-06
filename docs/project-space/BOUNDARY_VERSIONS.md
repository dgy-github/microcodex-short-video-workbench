# Boundary Versions

This file freezes the initial version boundaries for the new standalone project.

## Release policy

- project lifecycle start: `0.1.0-alpha.4`
- first internal dogfood target: `0.1.x`
- first external pilot target: `0.2.0-beta.1`
- first stable customer deployment target: `1.0.0`

Project versioning follows Semantic Versioning discipline.

## Platform boundary

- supported OS: `Windows 10 22H2+`
- supported OS: `Windows 11 23H2+`
- architecture: `x64 only` for v1
- no ARM64 support in v1

## Build/runtime boundary

- Rust: `1.96.x`
- Rust edition: `2021`
- Tauri: `2.8.x`
- tauri-build: `2.4.0`
- Svelte: `5.x`
- Vite: `6.x`
- TypeScript: `5.x`
- Node.js build baseline: `20.x LTS`

## Media/runtime boundary

- ffmpeg sidecar: `pinned per release manifest`
- ffprobe sidecar: `pinned per release manifest`
- optional Python sidecar baseline if retained: `3.11.x`

## Model boundary

### Text

- default text tier: `Flash`
- flash model: `deepseek-chat`
- pro model: `deepseek-v4-pro`
- official text endpoint: `https://api.deepseek.com/beta`

### Vision

- default vision model: `qwen3-vl-plus`
- default vision endpoint: `https://dashscope.aliyuncs.com/compatible-mode/v1`

## API/config boundary

The customer-facing app must keep its own product config. It must not require
the customer to edit the generic `~/.nanocodex/config.toml` directly.

Recommended product config path:

- `%APPDATA%\MicrocodeXShortVideo\settings.json`

## Compatibility policy

The following changes are breaking and require at least a minor release before
`1.0.0`, and a major release after `1.0.0`:

- job JSON schema changes
- material pack shape changes
- handoff template changes that remove fields
- settings keys removed or renamed
- review report keys removed or renamed
