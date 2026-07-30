# Windows CI validation gate — 2026-07-30

## Scope

This batch adds a Windows GitHub Actions workflow to catch repository-level regressions before they reach manual Windows testing.

Roadmap movement:

```text
REL-14  🟡  Windows CI build/test gate
```

## Workflow

File:

```text
.github/workflows/windows-ci.yml
```

Triggers:

```text
push to feature/windows-native-capture
push to main
pull requests
manual workflow_dispatch
```

Validation steps:

```text
bun install --frozen-lockfile
bun run lint
bun run build
cargo fmt --all -- --check
cargo test --no-default-features --locked
cargo check --no-default-features --locked
```

The job runs on `windows-latest`, has a 45-minute timeout, uses read-only repository permissions, caches Rust build outputs, and cancels obsolete runs for the same ref.

## Acceptance evidence

`REL-14` remains 🟡 until GitHub reports one successful workflow run on `feature/windows-native-capture`.

A successful run must confirm:

- the Bun lockfile installs without mutation;
- frontend linting passes;
- the Vite production build succeeds;
- Rust formatting is clean;
- Rust unit tests pass;
- Windows-only dependencies and generated bindings compile through `cargo check`;
- required Tauri resources such as icons are present.

Manual recording and playback tests remain required because CI does not have an interactive desktop capture session.

## Failure handling

The first run may expose existing repository drift. Those failures are evidence, not workflow failure. Fixes must be committed separately and the workflow rerun until green.

## Security impact

```text
Security impact:
Added automated read-only validation of repository source, dependencies, frontend output and Windows Rust compilation.

Data accessed:
Repository source files, Bun and Cargo lockfiles, dependency registries, compiler output and test output.

Data written:
Ephemeral GitHub Actions workspace files, dependency caches and workflow logs. No user recordings or local application data are accessed.

Network communication added:
GitHub Actions downloads pinned marketplace actions, Bun/Rust tooling and dependencies from configured registries during CI.

New permissions/capabilities:
Workflow permission is limited to repository contents: read. No write, issue, pull-request, package, deployment or secret permission is requested.

External processes:
Bun, Vite, ESLint, Cargo, rustfmt, rustc and repository tests run inside the GitHub-hosted Windows runner.

Untrusted inputs:
Repository content, dependency metadata, pull-request changes and compiler/test output.

Validation added:
Frozen Bun dependency installation, frontend lint/build, Rust formatting, locked Cargo tests, locked Windows cargo check, 45-minute job timeout and per-ref cancellation.

Secrets involved:
None. The workflow does not request or consume repository secrets.

Security tests completed:
Static workflow-permission, trigger, command and dependency-lock review. A successful hosted run remains pending.

Remaining risks:
Third-party GitHub Actions are referenced by version tags rather than immutable commit SHAs. Hosted CI does not validate interactive capture, camera, microphone, system audio, WebView playback or installer signing. Dependency downloads remain supply-chain trust boundaries.
```
