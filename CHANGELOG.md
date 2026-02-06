# Changelog

## v0.4.8
- Removed bundled Agent Browser from app packaging and CI signing/notarization paths to stabilize cross-platform release builds.
- Kept Ensemble mode and switched to runtime detection of external `agent-browser` (`PATH` / `npx --no-install`).
- Enabled `SearchWeb` / `FetchURL` fallback when `agent-browser` is unavailable, with optional `curl`/`wget` shell fallback.
- Added README guidance for optional Agent Browser installation: https://agent-browser.dev/installation

## v0.4.7
- Fixed Windows packaging step to tolerate missing MSI output when NSIS-only build is selected in CI.
- Added explicit artifact-existence guard so CI fails only when no Windows installer is produced.

## v0.4.6
- Fixed Windows CI bundling failures by switching release build output to NSIS only (skip flaky WiX/MSI stage in GitHub runners).
- Aligned default Tauri bundle targets with CI (`app`, `deb`, `rpm`, `nsis`).

## v0.4.5
- Fixed Windows CI panic in `build.rs` caused by canonical path prefix mismatch.
- Restricted CI bundle outputs to stable targets (`deb/rpm`, `msi/nsis`, `app`) to avoid flaky AppImage/DMG tooling failures.
- Hardened macOS release signing by signing all embedded Mach-O binaries before app re-sign and notarization.

## v0.4.2
- Fixed release workflow to use global Tauri CLI with platform-native bindings.
- Removed CI dependency churn that could break Tauri native binding resolution.
- Ensured macOS signing/notarization happens after signing embedded Agent Browser binaries.

## v0.4.1
- Fixed release workflow for cross-platform Tauri native bindings in CI.
- Signed embedded Agent Browser binaries in macOS release notarization flow.

## v0.4.0
- Added Ensemble (Co-Work) mode.
- Fixed multiple UI issues and interaction inconsistencies.
- Integrated Agent Browser for internet-required requests.
