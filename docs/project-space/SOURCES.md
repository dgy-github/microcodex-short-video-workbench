# GitHub Source Notes

I did not find one canonical GitHub-standard document named "SSBA" that
simultaneously covers scope, version boundaries, code style, testing, UI, and
handoff. This space pack is therefore a composite baseline assembled from strong
GitHub project-template patterns.

Key sources:

- `dx-tooling/planning-template`
  - planning, context, and project bootstrap structure
  - <https://github.com/dx-tooling/planning-template>

- `NLeSC/project-template`
  - repository bootstrap and governance skeleton
  - <https://github.com/NLeSC/project-template>

- `semver/semver`
  - version boundary discipline
  - <https://github.com/semver/semver>

- `go-gitea/gitea`
  - contribution, review, and code-quality expectations from a mature
    production repo
  - <https://github.com/go-gitea/gitea>

- `primer/react`
  - design-system and UI governance influence
  - <https://github.com/primer/react>

- `choughton/llm-handoff`
  - handoff artifact structure inspiration
  - <https://github.com/choughton/llm-handoff>

- `softaworks/ai-dev-tasks-and-handoffs`
  - practical transfer and continuation document patterns
  - <https://github.com/softaworks/ai-dev-tasks-and-handoffs>

How these sources were applied:

- versioning -> `BOUNDARY_VERSIONS.md`
- coding/contribution quality bar -> `CODE_STYLE.md`
- release and verification discipline -> `TESTING.md`
- UI system governance -> `UI_GUIDELINES.md`
- continuation / transfer shape -> `HANDOFF.md`
