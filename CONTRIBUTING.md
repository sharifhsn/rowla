# Contributing to Rowla

Thank you for your contribution to Rowla. New contributors and AI-assisted contributions are welcome.
Documentation, reproduction steps, device results, and design feedback are useful contributions too.
Please read [the community conduct policy](CODE_OF_CONDUCT.md).

## Choose a first change

Use [the roadmap](docs/ROADMAP.md) and [good first issues](https://github.com/sharifhsn/rowla/labels/good%20first%20issue) to find a small task.
Ask questions in [Discussions](https://github.com/sharifhsn/rowla/discussions). For a larger feature, discuss the user problem before implementation.
A draft pull request is a useful place to ask for help with a specific change.

1. Fork the repository.
2. Create a branch from current `main`.
3. Make one focused change.
4. Complete the checks relevant to that change.
5. Open a pull request and describe the evidence.

AI tools can help. Review their output and take responsibility for the submitted code.
Use [AGENTS.md](AGENTS.md) for the shared agent instructions. Do not present generated benchmark values as measured results.


## Build

Use macOS 15.2+, Xcode Command Line Tools, Python 3, and Rust through rustup.
The repository pins Rust 1.96.0 in `rust-toolchain.toml`. Keep `Cargo.lock` in source control.

```sh
scripts/build.sh
open dist/Rowla.app
```

The build script creates `dist/Rowla.app`. It checks the dependency notices and downloads a pinned Sparkle archive.
It uses two Cargo jobs by default. Set `TASKBAR_BUILD_JOBS` to change this limit.
Python supports the build and test tools. The app needs no Python runtime.

The build uses Rust and the macOS SDK. No Swift helper or Node runtime is necessary.

Use `scripts/build.sh --universal` to build for Apple Silicon and Intel.
The public beta download supports Apple Silicon only.

## Checks

```sh
cargo fmt --all --check
cargo test --locked --all-targets -j 2
cargo clippy --locked --all-targets -j 2 -- -D warnings
for script in scripts/*.sh; do bash -n "$script"; done
scripts/build.sh
python3 scripts/check-compatibility.py dist/Rowla.app
dist/Rowla.app/Contents/MacOS/taskbar-rs --check-compatibility
TASKBAR_TEST_BUNDLE="$PWD/dist/Rowla.app" python3 -m unittest discover -s tests -v
```

Rust tests include opt-in sort and native dictionary microbenchmarks. The default checks skip these microbenchmarks.
Python tests use temporary app bundles and a local HTTP server. They do not install Rowla or request screen permissions.
The stable-certificate test needs a certificate identity through `TASKBAR_TEST_LOCAL_IDENTITY`. The default checks skip this test.

Linux supports the portable library checks:

```sh
cargo test --locked --lib -j 2
cargo clippy --locked --lib -j 2 -- -D warnings
```

For documentation-only changes, examine local links, anchors, YAML forms, and the updated GitHub page. No native build is necessary.

GitHub Actions runs these checks on Linux and builds the app on Apple Silicon and Intel macOS runners.
CI does not grant Accessibility or Screen Recording access. It does not prove controls work with every application.

## Code structure

Use the [project guide](AGENTS.md#code) for the complete code map and native invariants.
The guide also maps the user docs, API research, QA results, release procedure, and repository policies.

## Native checks

Use a fixed app bundle for native checks. Do not rebuild or sign that bundle during the checks.
The build script refuses to replace a bundle that runs.

```sh
dist/Rowla.app/Contents/MacOS/taskbar-rs --benchmark-ui 20
dist/Rowla.app/Contents/MacOS/taskbar-rs --gui-smoke 50
```

Run these commands in a local macOS desktop session. They create temporary test windows.

For window-control or capture changes, also do checks of permission denial, window close, minimized windows, sleep, and display changes.
Use `scripts/live-qa-fixture.py` for disposable fixture windows. Capture and Accessibility checks can need macOS permissions.
Use the [first-click regression procedure](docs/QA_FIRST_CLICK.md) for late native replies, restore, and rapid target changes.
Remove personal window titles and screenshots from bug reports.
Finite memory profiles do not prove that all native paths are free of leaks.

## Pull requests and licenses

Describe the changed behavior, the checks you completed, and the macOS versions you used.
State any native behavior that you did not exercise. Keep each change focused.

Keep `LICENSE`, `THIRD_PARTY_NOTICES.md`, and `licenses/` in distributions.
When `Cargo.lock` changes, examine each dependency's license and update the notices.
Preserve the additional Unicode notice and upstream objc2 license terms.
When the Rust version changes, include its standard-library copyright file.
`scripts/check-notices.py` rejects stale notice inventories before a build.

See [the release procedure](docs/RELEASE.md) for manual beta packages and the optional notarized release tools.
