# Repository execution contract

## Instructions and scope

Follow instructions in this order: the latest explicit `HUMAN → EXECUTOR` message, the current `ORCHESTRATOR → EXECUTOR` message, the assigned issue and unresolved review feedback, nested `AGENTS.md`, this file, then surrounding code. Issue-specific requirements override repository rules.

Build only the assigned issue. Apply YAGNI, preserve unrelated behavior, and do not add speculative options, abstractions, configuration, infrastructure, cleanup, formatting, or documentation. Match the surrounding language and framework style.

## Implementation quality

Use the configured formatter and lint rules. Keep source readable, explicit, and uncompressed. Use intent-revealing names, keep cohesive code together, and create helpers only when they remove concrete complexity. Remove directly superseded code and stale comments; do not leave dead or commented-out code.

Keep functions focused, make side effects visible, validate at boundaries, preserve useful error context, and never suppress errors to pass a gate. Comments must explain durable intent or constraints, not restate the code.

## Tests and validation

Tests belong with the behavior they verify and must exercise stable observable boundaries. Do not test generated assets, framework internals, or third-party behavior, and do not add a test framework unless the issue authorizes it.

Use repository-native commands. Run focused checks while working and relevant local gates before pushing; treat CI as the authoritative cross-platform result. Report deferred or uncovered human-verification items without claiming they passed.

## Dependencies

Do not add or change a dependency without explicit approval. Stop and report the requirement before doing so.

## Build, installation, and machine safety

Default to headless validation. Do not locally bundle or package, install, launch, or control the application unless a current explicit human or orchestrator instruction authorizes that named build or installation action. That authorization applies only to the named action and does not imply permission for related application or GUI interaction.

Do not use AppleScript, `osascript`, Accessibility APIs, simulated input, browser or Finder automation, notifications, GUI interaction, persistent services, watchers, or development servers unless the current task explicitly requires and authorizes them.

## Pull requests and handoff

Use one branch and one pull request per issue. Each pull-request description must contain exactly one `Fixes #N` for its issue.

Begin routine executor status with `**EXECUTOR → HUMAN**`. Use `**EXECUTOR → HUMAN — ACTION REQUIRED**` only for one precise blocking question. On completion or blockage, follow the issue-specific protocol and end with exactly one fenced `EXECUTOR → ORCHESTRATOR` handoff that reports repository, issue and pull request, branch, commit, CI, unresolved feedback, uncovered requirements, human verification, installation state, queue state, and any blocker.
