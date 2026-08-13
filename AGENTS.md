# Repository execution contract

## Instructions and scope

Follow instructions in this order: the latest explicit `HUMAN → EXECUTOR` message, the current `ORCHESTRATOR → EXECUTOR` message, the assigned issue and unresolved review feedback, nested `AGENTS.md`, this file, then surrounding code. Issue-specific requirements override repository rules.

A higher-priority current instruction overrides a lower-priority general rule only for the named action and current task. All unmentioned lower-priority rules still apply.

Build only the assigned issue. Apply YAGNI, preserve unrelated behavior, and do not add speculative options, abstractions, configuration, infrastructure, cleanup, formatting, or documentation. Match the surrounding language and framework style.

## Implementation quality

Use the configured formatter and lint rules. Keep source readable, explicit, and uncompressed. Use intent-revealing names, keep cohesive code together, and create helpers only when they remove concrete complexity. Remove directly superseded code and stale comments; do not leave dead or commented-out code.

Keep functions focused, make side effects visible, validate at boundaries, preserve useful error context, and never suppress errors to pass a gate. Comments must explain durable intent or constraints, not restate the code.

## Tests and validation

Tests belong with the behavior they verify and must exercise stable observable boundaries. Do not test generated assets, framework internals, or third-party behavior, and do not add a test framework unless the issue authorizes it.

The executor writes tests required by the issue or behavior change. Never weaken, skip, ignore, delete, or change a test only to make a gate pass. Do not mock the unit under test; mock external boundaries only when necessary.

Use repository-native commands. Run focused checks while working and relevant local gates before pushing; treat CI as the authoritative cross-platform result.

## Dependencies

Do not add or change a dependency without explicit approval. Stop and report the requirement before doing so.

## Build, installation, and machine safety

Default to headless validation. Do not locally bundle, package, or install unless a current explicit human or orchestrator instruction authorizes that named action for the current task. Build or installation authorization never includes launching or controlling the application.

For an authorized build or installation, validate first and wait for green CI when it exists. Use the normal non-interactive process on the exact validated commit, then report the installed commit or version. Stop on unexpected privilege, GUI, credential, security-bypass, destructive, or unsupported-environment requirements.

Do not use AppleScript, `osascript`, Accessibility APIs, simulated input, browser or Finder automation, notifications, GUI interaction, persistent services, watchers, or development servers unless the current task explicitly requires and authorizes them.

## Human verification

Distinguish verification that blocks completion from deferred low-risk visual verification, and report both accurately. Do not perform or claim GUI verification unless it is explicitly authorized.

## Pull requests and handoff

Use one branch and one pull request per issue. Each pull-request description must contain exactly one `Fixes #N` for its issue.

Begin routine executor status with `**EXECUTOR → HUMAN**`. Use `**EXECUTOR → HUMAN — ACTION REQUIRED**` only for one precise blocking question. On completion or blockage, follow the issue-specific protocol and end with exactly one fenced `EXECUTOR → ORCHESTRATOR` handoff. Include blocking-verification state, deferred visual items, installation result, installed commit or version when successful, installation blocker when failed, queue state, and the blocker when blocked, along with the repository, issue and pull request, branch, commit, CI, unresolved feedback, and uncovered requirements.
