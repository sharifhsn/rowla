# Help with Rowla

New users are welcome. You do not need Rust experience to use the app or report a problem.
Start with the [user guide](../USER_GUIDE.md).

## Choose a support route

| Need | Route |
| --- | --- |
| Install or use Rowla | [User guide](../USER_GUIDE.md) and [Discussions](https://github.com/sharifhsn/rowla/discussions) |
| A reproducible bug | [Bug form](https://github.com/sharifhsn/rowla/issues/new?template=bug_report.yml) |
| Slow clicks, previews, or memory growth | [Performance form](https://github.com/sharifhsn/rowla/issues/new?template=performance.yml) |
| A feature or workflow idea | [Discussions](https://github.com/sharifhsn/rowla/discussions) or [feature form](https://github.com/sharifhsn/rowla/issues/new?template=feature_request.yml) |
| A security vulnerability | [Private report](https://github.com/sharifhsn/rowla/security/advisories/new) |

This is a volunteer beta project. There is no guaranteed response time.

## First-launch problems

The download needs macOS 26+ and Apple Silicon. It uses a development certificate without Apple notarization.
If macOS blocks it, use Open Anyway for Rowla as described in the user guide.
Some managed Macs do not permit this exception.

Accessibility access controls window discovery and window actions. Screen Recording access controls thumbnail capture.
Confirm the relevant grant in System Settings. Quit and reopen Rowla after a permission change if macOS requests it.
An enabled switch in Settings does not prove that a changed app build has a valid grant.

Ad-hoc source builds can need new grants after code changes.
For repeated local builds, see [the certificate procedure](RELEASE.md#local-signatures-and-permissions).
Do not edit the macOS permission database.

## Window and preview behavior

A fresh thumbnail needs a native capture. Cached thumbnails appear immediately.
A minimized window uses its last captured image. A window with no earlier capture can have no thumbnail until restoration.
The thumbnail cache has a 16 MiB / 32-image limit. Capture buffers and other native objects use more memory.

The small × and hover ⌘W close the selected window. They do not quit its application.
The application can display a save-confirmation dialog.
Pinned icons disappear when their application has a discovered window. Hidden or minimized windows also count.

## A useful report

Include the Rowla version, macOS version, architecture, affected application, and exact reproduction steps.
Describe your displays and Spaces if they affect the problem.
State whether the problem occurs during manual use, computer automation, or both.
For memory growth, include the workload, duration, start and end values, and measurement tool.
Compare the same workload before and after a proposed fix.

Local aggregate diagnostics use `~/Library/Application Support/Taskbar Rust/diagnostics.json`.
Remove personal paths or other private data before an attachment. A screenshot is optional.
Do not upload preferences wholesale, private keys, or desktop recordings that show personal windows.
