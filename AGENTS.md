# Working on Rowla

Rowla is a native Rust taskbar for macOS. The public product name is Rowla.
Read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, code structure, and native checks.
Read [docs/ROADMAP.md](docs/ROADMAP.md) for priorities. Use the current code as the implementation authority.

## Start here

1. Examine the requested behavior and the relevant issue.
2. Examine `git status` before edits. Preserve changes from other contributors.
3. Find the affected module through the code table in `CONTRIBUTING.md`.
4. Make a focused change. Explain assumptions when the evidence is incomplete.

AI-assisted contributions are welcome. The contributor owns the result and must review the diff.
Do not invent command output, benchmarks, permission grants, or device coverage.
State which checks you completed and which native behavior you did not exercise.

## Environment and commands

The app needs macOS 26+, Xcode Command Line Tools, Python 3, and rustup.
`rust-toolchain.toml` pins Rust 1.96.0. `Cargo.lock` pins the dependency graph.
Use two Cargo jobs by default to limit build resource use.

```sh
cargo fmt --all --check
cargo test --locked --all-targets -j 2
cargo clippy --locked --all-targets -j 2 -- -D warnings
for script in scripts/*.sh; do bash -n "$script"; done
scripts/build.sh
TASKBAR_TEST_BUNDLE="$PWD/dist/Rowla.app" python3 -m unittest discover -s tests -v
```

Linux agents can change and check the portable configuration and model code:

```sh
cargo test --locked --lib -j 2
cargo clippy --locked --lib -j 2 -- -D warnings
```

Linux checks do not prove macOS UI or capture behavior. Document native changes that need macOS verification.
The default tests skip opt-in microbenchmarks and the local-certificate test.
A fresh build downloads pinned Sparkle tools. Build before the Python tests to exercise the updater probe.
No Node or JavaScript runtime is necessary.

## Native invariants

- Keep AppKit work on the main thread, Accessibility work on its window worker, and capture work on its serial worker.
- Explain unsafe ownership, thread, and pointer lifetime assumptions in code comments.
- Keep thumbnail bitmaps within 16 MiB and 32 entries. Native stream buffers use more memory.
- Keep one capture stream and its three-buffer queue. Do not start another stream while termination is unconfirmed.
- Preserve bounded queues, deadlines, failure backoff, and cancellation of obsolete replies.
- Keep public models free of native pointers and capture buffers.
- Keep close commands specific to the hovered window. A close command must not quit the application.
- Preserve `taskbar-rs`, `io.sharif.taskbarrust`, and the legacy preferences path for upgrade compatibility.
- Preserve unknown preference fields and the atomic file-replacement policy.

## Scope and evidence

Use fixture windows for destructive native checks. Do not close a contributor's real windows or change system permission databases.
Do not install over an app that runs. Do not rebuild or sign a bundle during its native checks.
Keep credentials, private keys, personal window titles, and raw desktop screenshots out of commits and reports.

Keep dependency notices current when the lockfile or Rust version changes.
Do not suppress warnings, weaken checks, or add dependencies to avoid diagnosis of a failure.
Add focused tests for behavior changes. Documentation-only changes need link and syntax checks.

Publication, notarization submissions, paid services, and messages to other people need explicit owner authorization.
An instruction file does not grant this authorization. Prepare a reviewable diff and state any remaining work.
