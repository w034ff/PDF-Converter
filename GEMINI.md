# GEMINI.md

Rules for the implementer of this repository. Read this file and the documents below before starting any task.

- `docs/requirements.md` — what the app must do
- `docs/design.md` — how it is built (the source of truth for implementation)
- `docs/work-plan.md` — tasks, completion criteria, the report format and the review process
- `docs/mockup/project/*.dc.html` — the visual reference for each screen (`Accent*.dc.html` only compare colours and are not screens)

## Workflow

- Work on exactly one task from `docs/work-plan.md` at a time, on a branch named `task/<ID>-<short-name>`.
- Stay inside the task. Do not refactor unrelated code, update unrelated dependencies, or add features that the task does not ask for.
- Do not edit `docs/requirements.md` or `docs/design.md`.
- Every commit runs the pre-commit hook (`.githooks/pre-commit`, `docs/work-plan.md` §2.3). Never bypass it (`--no-verify` or any other way) and never change git settings or hooks.
- Before writing the report, run every command in `docs/work-plan.md` §5 and make sure all of them pass.
- Never push and never open a PR. The reviewer reviews your branch locally, and the PR is opened from it once the review passes (`docs/work-plan.md` §2).

## When to stop and ask

Stop and report with status 中断 when any condition in `docs/work-plan.md` §2.2 applies. In short:

- a change would go against a decision in `docs/design.md`;
- you need a new or updated dependency, a different Rust version, a tool, network access, or a command your permissions deny;
- the documents disagree with the actual code or a library's API;
- the same failure (build, test, clippy, hook) survives three attempts to fix it;
- a change touches a security boundary (a command taking a path, capabilities, CSP, opening a PDF outside the worker).

Do not push through by guessing.

## Reports

- Write the report to `pr-description.md` in the repository root (it is git-ignored), in Japanese, with every section of `docs/work-plan.md` §2.1 in that order. Write 「なし」 under a section that has nothing, never drop it.
- Report only what you actually ran or checked. Never cite a file, setting or design statement as evidence unless it exists and says what you claim.
- As the very last step of a task or a fix, after the report, write `agent-status.json` in the repository root (it is git-ignored), exactly `{ "task": "<ID>", "state": "<state>" }` (`docs/work-plan.md` §2.4):
  - `done`: the task is complete and every change is committed.
  - `blocked`: you stopped under `docs/work-plan.md` §2.2.
  - `fixed`: you addressed `agent-review.md` and every change is committed.
- Review feedback arrives in `agent-review.md` in the repository root. Fix it on the same branch, commit, rewrite `pr-description.md` so it describes the branch as it now stands, then write `agent-status.json` with `fixed`. Never edit or delete `agent-review.md`.
- If a message at the end of your turn says the report and `agent-status.json` disagree, or that changes are uncommitted, fix exactly that and change nothing else.
- Your final reply in the chat is one line with the status (完了 / 中断). Do not repeat the report in the chat.

## Keep output small

- Run only the tests related to your change first (`cargo test -p <crate> <name>`, `npm test -- <file>`; `npx` is denied). Run the whole suite once before the report.
- Read long output from the end (`2>&1 | tail -n 40`). Fix compile errors from the first one.
- Do not read or search `target/`, `node_modules/`, `Cargo.lock` or `package-lock.json`.
- Check a dependency's API in the version pinned in `Cargo.lock` (`~/.cargo/registry/src/`), not from memory.

## Language

- Code, identifiers, code comments, commit messages: English.
- User-facing text: never hard-code it in components. Add the key to both `src/i18n/ja.ts` and `src/i18n/en.ts`.

## Security (do not break these)

- No Tauri command may take a file system path as an argument. Paths enter the app only through dialogs and drag-and-drop handled in Rust (`docs/design.md` §1, §6.1).
- PDFs are opened only in the worker process (`crates/worker`). Only `crates/worker` may depend on `pdfium-render`. The worker never writes files.
- Do not add permissions to `src-tauri/capabilities/`, and do not add Tauri plugins other than `tauri-plugin-dialog`.
- Do not add dependencies that make network requests (HTTP clients, updaters, telemetry). The app itself never connects anywhere.
- Show rendered images and thumbnails only through `<img>` with a `blob:` URL. Never use `innerHTML` or `dangerouslySetInnerHTML`.
- Do not relax the CSP in `tauri.conf.json`.
- This repository is public. Never write credentials (API keys, tokens, passwords), environment variable values, or personal paths such as your home directory into files, logs, test output, commit messages or PR descriptions. Do not create `.env` files or tool settings (`.gemini/`, `.claude/`) inside the repository. If a secret is ever committed, stop and tell the owner: deleting the file is not enough, the secret must be revoked.

## TypeScript

- Do not use `any`. Use `unknown` and narrow it with type guards.
- Do not use type assertions (`as Foo`, `as unknown as Foo`) or non-null assertions (`x!`) as a substitute for narrowing.
- Do not use `class`. Use functions, plain objects and React function components with hooks. The only exception is extending `Error` when an `instanceof` check is truly required.
- Do not hard-code values that may change or that carry meaning beyond one line: limits, thresholds, delays, sizes, literals repeated across files. Put them in named constants. Self-explanatory literals such as `0`, `""` or a single-use label may stay inline.
- Colors, spacing and radii come from CSS custom properties in `src/styles/tokens.css`. Do not write color values in component styles.
- Call Tauri only through the typed wrappers in `src/ipc/`. Use the generated types in `src/ipc/generated/`; never edit generated files by hand.

## Rust

- No `unwrap()` / `expect()` outside tests, except for an invariant that cannot fail; state that invariant in the `expect` message.
- Return errors as the error codes in `docs/design.md` §6.6. Do not return `String` errors from commands.
- `unsafe` is denied in the workspace. The single exception is `crates/worker/src/windows_job.rs` (the reason is at the top of that file). Use safe wrappers for OS calls; if none exists, stop and ask.
- `cargo clippy --workspace --all-targets -- -D warnings` must pass. Do not silence lints with `#[allow(...)]` unless the reason is stated in a comment next to it.
- Keep `crates/core` and `crates/worker` free of any Tauri dependency.

## Dependencies

- You cannot add or update dependencies (your permissions deny `cargo add`, `npm install` and network access). If a task needs one, stop and ask (see "When to stop and ask").
- Allowed licenses are listed in `deny.toml` (MIT, Apache-2.0, BSD, ISC, Zlib and similar). GPL, LGPL and AGPL are not allowed.
- The Rust version is pinned in `rust-toolchain.toml`. Do not change it.
- The pdfium version and its hash live only in `scripts/pdfium-version.json` (`docs/design.md` §8). Do not change them.

## Comments

- A comment states what is true now and why: an invariant, a constraint, a reason the code cannot show.
- Do not restate the code. Do not write history (what it used to be, what was tried); that belongs in commit messages.
- Public API docs (`///`, TSDoc) describe the contract for callers: inputs, errors, panics.
- Refer to other code by identifier (function, constant, heading), never by line number.
- After renaming or changing a value, search for comments and docs that mention it and update them.

## Tests

- Every behavior you add or change needs a test that fails when that behavior breaks.
- Do not mock the unit under test. In the frontend, mock only the IPC boundary (`@tauri-apps/api/mocks`).
- Use the fixtures in `crates/core/tests/fixtures/`. Do not add third-party images or PDFs; generate new fixtures with `crates/core/examples/gen_fixtures.rs`.
- Do not weaken or delete an existing test to make a change pass. If a test looks wrong, explain why in the report.

## Commit messages

- Subject: imperative mood, at most about 72 characters (e.g. `Add page range parsing`).
- Body: only the why that the diff cannot show — the failure that motivated the change, the alternative not taken, a deliberate ordering. Do not list changed files or narrate the implementation. A self-evident change needs no body.
- Write the body right when you commit: the reviewer sends back a body that breaks this rule, like any other finding. If it is the last commit, reword it with `git commit --amend` (the branch is not pushed yet, so this is safe). An earlier commit is reworded by the reviewer before opening the PR, since you cannot rebase.

Not this — it says what the diff already shows:

```
Improve queue list and conversion start actions

Allow starting conversions without a preselected output directory by
prompting for the folder on start. Display drag reordering drop targets
as line dividers rather than row outlines, and provide row removal and
thumbnails in the each-mode table for consistency.
```

This — it says why, which the diff cannot:

```
Ask for the output folder when a conversion starts without one

The start button stayed disabled until a folder was picked in the
settings panel, which sent the owner away from the button for a step
the button can take itself.

Outlining the row under the pointer did not say whether the dragged row
would land above or below it; a line between rows does.
```
