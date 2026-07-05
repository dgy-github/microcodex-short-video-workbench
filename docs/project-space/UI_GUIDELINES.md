# UI Guidelines

## Product posture

This is an operator workbench, not a marketing site and not a developer console.

Design intent:

- quiet
- dense but readable
- workflow-first
- status-visible
- easy to scan repeatedly

## Layout rules

- use a permanent left navigation rail
- use a single primary work surface
- keep secondary details in tabs, drawers, or split panes
- do not lead with chat bubbles as the main experience
- do not nest cards inside cards

## Visual rules

- radius: `8px` or less
- use a restrained neutral palette with one accent
- use status colors only for state, not decoration
- prioritize tables, panels, tabs, and steppers over oversized hero layouts
- keep important counts visible: jobs, tokens, cost, status

## Control rules

- mode selection uses segmented controls
- binary settings use toggles or checkboxes
- numeric limits use inputs or steppers
- provider routes use explicit preset selectors
- destructive actions require clear confirmation

## Required primary surfaces

1. dashboard
2. new job
3. queue
4. material pack editor
5. review
6. competitor
7. settings

## New job page requirements

Must show, before start:

- source
- mode
- estimated frames
- estimated model calls
- estimated tokens
- estimated cost
- current budget policy

## Material pack editor requirements

Must keep raw and optimized content separate.

Suggested tabs:

- raw transcript
- OCR subtitles
- visual notes
- editable script
- title/copy candidates
- export

## Settings page requirements

The settings page must center the business workflow, not internal agent tuning.

Required groups:

1. text provider
2. vision provider
3. cost and budget
4. runtime limits

Explicitly hide from customer UI:

- sandbox mode
- approval policy
- generic coding model list
- MCP marketplace management
- git/session tooling

## Flash/Pro interaction rule

The UI must expose both:

1. a global default tier switch
2. a per-job override tier switch

Switching tier must update the effective model preset immediately in the UI
summary.

## Accessibility

- all key actions must be keyboard reachable
- status text must not rely on color alone
- buttons must have explicit labels
- long reports must support copy/export without layout breakage
