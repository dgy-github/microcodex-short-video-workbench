# Code Style

## Core principles

1. Prefer existing repo patterns over invention.
2. Keep business logic out of visual components.
3. Keep task orchestration deterministic where possible.
4. Separate raw extracted facts from rewritten content.
5. Every user-editable artifact must be version-safe.

## Repository structure

Recommended top-level structure for the new repo:

```text
apps/
  desktop/
crates/
  app-core/
  config/
  jobs/
  media/
  providers/
  review/
docs/
tests/
resources/
```

## Rust rules

- edition: `2021`
- toolchain: stable `1.96.x`
- formatting: `cargo fmt --check`
- linting: `cargo clippy --all-targets --all-features -D warnings`
- no `unwrap()` in business-path code unless failure is structurally impossible
- prefer typed structs over unstructured `serde_json::Value` in internal flows
- command handlers must stay thin; move workflow logic into crate modules

## Frontend rules

- framework: `Svelte 5`
- language: `TypeScript strict`
- keep `App.svelte` thin; split real pages into components
- no API/business logic in presentational leaf components
- no hidden side effects inside render blocks
- page state and job state should live in explicit stores or controller modules

## Config rules

- product config and developer config are separate concerns
- end-customer settings must not expose generic agent controls
- fixed model presets should be data-driven, not hand-scattered through the UI
- any new config key requires:
  - default value
  - migration behavior
  - UI label
  - validation rule

## Documentation rules

Any new feature must update, at minimum:

- feature contract
- test impact
- handoff impact when the work is incomplete

## Commit rules

Recommended commit shape:

1. `feat(desktop): ...`
2. `feat(jobs): ...`
3. `fix(settings): ...`
4. `docs(space): ...`
5. `test(review): ...`

Avoid mixing broad refactors with feature work unless the refactor is required
to ship safely.
