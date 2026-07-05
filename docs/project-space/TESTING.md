# Testing Standard

## Objective

The product is customer-deployed on Windows, so the test strategy must prove:

1. core extraction flows work
2. settings are validated and persisted safely
3. token/cost accounting is trustworthy
4. the desktop UI does not break common operator workflows

## Test pyramid

### 1. Unit tests

Required for:

- config parsing and migration
- budget estimation
- token estimation
- report scoring
- job state transitions
- schema validation helpers

### 2. Integration tests

Required for:

- create job -> process -> artifact output
- review job -> report output
- competitor job -> comparison output
- settings save/load with fixed model presets
- Flash/Pro tier switching
- over-budget stop behavior

### 3. UI tests

Required for:

- import link workflow
- local upload workflow
- queue visibility
- material pack editing
- settings save flow
- cost warning banner rendering

Recommended stack:

- component tests: Vitest
- end-to-end desktop checks: Playwright against Tauri build or a web-shell mode

## Release gates

No Windows installer build is shippable unless all of the following pass:

1. `cargo fmt --check`
2. `cargo clippy --all-targets --all-features -D warnings`
3. Rust unit and integration tests
4. frontend typecheck
5. frontend build
6. key UI flow tests
7. installer smoke test on a clean Windows machine

## Cost-control tests

This product has a hard business requirement around token/cost control. Add
dedicated tests for:

- Flash preset maps to the expected model and URL
- Pro preset maps to the expected model and URL
- custom endpoint mode unlocks URL editing
- official endpoint mode re-locks URL editing
- cost estimate blocks job start when budget policy says so
- actual usage accumulation never goes negative

## Fixture policy

- store redacted sample videos or short synthetic fixtures only
- do not commit customer secrets
- do not commit live API keys
- any saved usage payloads must be redacted

## Manual acceptance checklist

Before first customer deployment, manually verify:

1. fresh install can open
2. settings can be saved with user-provided keys
3. Douyin import works or fails clearly
4. extraction job completes and writes artifacts
5. material pack can be edited and exported
6. review flow works with uploaded result video
7. cost panel shows both estimate and actual usage
