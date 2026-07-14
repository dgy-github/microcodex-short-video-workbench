# SSBA: Space Setup and Baseline Agreement

## Purpose

This document defines the new project space for a standalone Windows desktop
application focused on:

1. Douyin link import and local video import
2. batch material extraction
3. editable material packs
4. generated-video review
5. competitor analysis

This is a new project, not a destructive rewrite of the existing `nanocodex`
workspace.

## Project identity

- Project name: `MicrocodeX Short Video Material Workbench`
- Product type: `Windows desktop application`
- Primary platform: `Windows 10/11 x64`
- Delivery mode: `installer deployment for end customers`

## Mission

Turn short videos into structured, editable content assets and post-generation
review reports, without directly generating videos inside the desktop product.

## Non-goals

The following are explicitly out of scope for v1:

- direct text-to-video generation
- direct image-to-video generation
- exposing generic coding-agent controls to end customers
- turning the product into a chat-first assistant shell

## Product boundaries

The product stops at:

1. ingest
2. extraction
3. understanding
4. editing
5. review
6. competitor comparison

The actual video generation step is intentionally externalized to the customer's
preferred video app.

## Tech baseline

The new project should reuse the existing Rust desktop direction:

- Tauri 2
- Rust backend
- Svelte frontend
- ffmpeg sidecar
- optional Python sidecar only when a downloader cannot be replaced safely

## Model baseline

Only two primary model lanes are allowed in v1:

- Vision lane: `Qwen VL`
- Text lane: `DeepSeek`

Default routing:

- visual analysis -> `qwen3-vl-plus`
- text understanding -> `deepseek-v4-flash` by default
- quality override -> `deepseek-v4-pro` when the operator switches tier

## Repo separation rule

The new project must live in its own repository and workspace. Suggested path:

- `D:\agent_prac\microcodex-short-video-workbench`

The current repo remains a reference source, not the deployment target.

## Source-of-truth documents

The following files are mandatory before scaffold:

- `BOUNDARY_VERSIONS.md`
- `CODE_STYLE.md`
- `TESTING.md`
- `UI_GUIDELINES.md`
- `HANDOFF.md`

No feature branch should start until these files exist in the new repo.

## Architecture shape

The v1 product is task-first, not chat-first.

Required top-level flows:

1. `extract`
2. `review`
3. `competitor`

Required top-level UI surfaces:

1. dashboard
2. new job
3. queue
4. material pack editor
5. review
6. competitor
7. settings

## Governance

Any change to scope, model routing, supported OS versions, release channel, or
customer-facing data contract must update this space pack first.
