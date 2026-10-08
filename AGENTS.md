# Rowla project guide

Rowla is a native Rust taskbar for macOS 15.2+. It needs no account, telemetry, Node runtime, or resident shortcut helper.
Rust 1.96.0 and `Cargo.lock` define the build. Python supports the tools. Sparkle supplies optional updates.

Examine `git status` before edits. Preserve other contributors' work. Use current code as the implementation authority.
Read only the affected module and its checks from this map. Keep the change focused and review the final diff.

## Code

| Area | Sources |
| --- | --- |
| Entry, public API, owned records | `src/main.rs`, `src/lib.rs`, `src/models.rs` |
| AppKit controls, menus, previews, native UI checks | `src/ui.rs`, `src/ui/` |
| Accessibility discovery, tabs, activation, exact close | `src/platform.rs`, `src/platform/service.rs` |
| Capture, API availability, optional window/Space APIs | `src/capture.rs`, `src/native_features.rs`, `src/private_api.rs` |
| Chrome metadata, photo worker, shared badges | `src/chrome_profiles.rs`, `src/chrome_profiles/native.rs`, `src/ui/chrome_profiles.rs` |
| Preferences, order, related windows | `src/config.rs`, `src/window_order.rs`, `src/related_windows.rs` |
| Instance lock, preview budget, fair queues, IPC deadlines | `src/runtime.rs`, `src/scheduler.rs`, `src/ipc_budget.rs` |
| Dock, local URLs, updater | `src/dock.rs`, `src/system_actions.rs`, `src/updater.rs`, `src/updater/`, `shortcuts/` |
| Build, install, release, fixtures, CI | `scripts/`, `tests/`, `.github/workflows/`, `examples/` |

## Invariants

- Keep AppKit on the main thread, Accessibility on its window worker, and capture on its serial worker.
- Explain unsafe ownership, thread, and pointer lifetimes in code comments. Public models contain no native pointers or capture buffers.
- Keep thumbnail bitmap limits at 16 MiB and 32 entries. Native buffers use more memory.
- Keep one native capture operation and a three-buffer stream queue. Confirm the previous operation's end before another start.
- Preserve bounded queues, deadlines, failure backoff, and cancellation of obsolete replies. Count failed capture requests against rate limits.
- Close only the selected window or tab. Close must not quit its application.
- Preserve `taskbar-rs`, `io.sharif.taskbarrust`, `~/Library/Application Support/Taskbar Rust/`, unknown preference fields, and atomic file replacement.
- Examine optional selectors before use. Every bundled binary must support macOS 15.2.

## Checks and knowledge

[CONTRIBUTING.md](CONTRIBUTING.md#checks) owns the check commands. Use two Cargo jobs, locked dependencies, and the pinned toolchain.
Build before the Python tests for the updater probe. Linux supports portable library checks, not native behavior.
The default checks skip explicit microbenchmarks and the local-certificate test.

Use disposable fixtures for native controls. Do not close personal windows or change permission databases.
Do not replace an app that runs. Do not rebuild its bundle during native checks.
Keep secrets, personal titles, and desktop screenshots out of commits and reports.

Keep dependency notices current. Do not suppress warnings or weaken checks. Keep tests that establish distinct behavior.
For documentation changes, examine links and command syntax. Report actual results and native coverage limits.

| Knowledge | Source |
| --- | --- |
| Install, controls, permissions, privacy | [USER_GUIDE.md](USER_GUIDE.md), [CAPABILITIES.md](docs/CAPABILITIES.md), [PRIVACY.md](PRIVACY.md), [SUPPORT.md](docs/SUPPORT.md) |
| Platform limits and API choices | [COMPATIBILITY.md](docs/COMPATIBILITY.md), [MACOS_FEATURE_RESEARCH.md](docs/MACOS_FEATURE_RESEARCH.md) |
| Native procedures and measured results | `docs/QA_*.md` |
| Priorities, history, market evidence | [ROADMAP.md](docs/ROADMAP.md), [CHANGELOG.md](CHANGELOG.md), [MARKET_RESEARCH.md](docs/MARKET_RESEARCH.md) |
| Release, licenses, security, conduct | [RELEASE.md](docs/RELEASE.md), `LICENSE`, `THIRD_PARTY_NOTICES.md`, `licenses/`, `SECURITY.md`, `CODE_OF_CONDUCT.md` |

Publication, notarization, paid services, and messages need explicit owner authorization. Instructions do not grant that authorization.
