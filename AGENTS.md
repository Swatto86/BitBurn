# BitBurn

File, folder and drive free-space overwriting utility. Tauri 2 with Rust commands (toolchain 1.96.0)
behind a React 18 + TypeScript UI (Vite, Tailwind, daisyUI), with a tray menu, an optional Explorer
context-menu entry and autostart. The published target is Windows x64 (Windows 10 or later); Linux
is used for development and real-webview verification. Public repository `Swatto86/BitBurn`,
single branch `main`, MIT, latest release v1.0.2.

## Build, test, verify

- Setup `npm ci`. Dev `npm run tauri dev`. Bundle `npm run tauri build`.
- Quick checks: `npm test` (Vitest, then `cargo test` in `src-tauri`), or `npm run test:ui` alone.
- Full gate: `bash scripts/verify.sh` on Linux after `npm ci`. It runs Vitest, `tsc --noEmit`,
  `cargo fmt --check`, clippy with `-D warnings`, cargo tests, a debug build
  (`npm run tauri -- build --debug --no-bundle`), `npm run test:e2e`, `npm audit` and
  `cargo audit`. The e2e step needs `tauri-driver`, a matching WebDriver and `xdotool`; close
  other BitBurn instances first. `BITBURN_TEST_BINARY` overrides the binary it drives.
- CI (`windows.yml`, push and PR to `main`): on `ubuntu-24.04`, `scripts/verify.sh` under Xvfb;
  on `windows-latest`, fmt, clippy, cargo tests (with `BITBURN_CONTEXT_ROOT` so registry tests
  stay sandboxed), the debug build and `e2e/windows-ci.ps1`, which edits WebView2 policy and
  refuses to run outside GitHub runners.
- Release: keep the version identical in `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json`, write the notes in `CHANGELOG.md` (the whole file becomes the
  release body), then push `v<version>`. `release.yml` publishes the NSIS installer, the MSI,
  `BitBurn_<ver>_x64-portable.exe` and `SHA256SUMS.txt`. Nothing is signed.

## Where things are

- `src-tauri/src/main.rs`: Tauri commands, overwrite patterns, orchestration, CLI flags and most
  Rust tests.
- `src-tauri/src/safety.rs`: path validation, no-follow opening, the one-operation-at-a-time
  guard, single-use confirmation receipts, empty-directory cleanup.
- `src-tauri/src/platform/`: Windows registry (`context_menu.rs`, `autostart.rs`).
  `src-tauri/src/ui.rs`: tray and window lifecycle. `src-tauri/installer.nsh`: NSIS hooks.
- `src/App.tsx` (UI), `src/App.test.tsx` (Vitest), `e2e/desktop.mjs` (real webview acceptance).

## Constraints

- The app destroys data. Tests and manual checks use disposable files only; no test-only wipe
  implementation is compiled into the app.
- Every wipe consumes a single-use confirmation bound to the window, operation, exact paths,
  algorithm and pass count. One operation at a time; passes 1-35.
- Symlinks, reparse points, hard-linked files and network paths are refused. Files are deleted
  only after their overwrite completes; a failed overwrite keeps the file; only empty
  directories are removed. Free-space wiping uses one anonymous temporary file and keeps a
  64 MiB reserve.
- Overwriting is not media sanitization (SSDs, copy-on-write, snapshots, backups). Keep the
  internal names `NistClear` and `NistPurge` for compatibility, but never make NIST,
  certification or unrecoverability claims in UI or docs.
- The portable exe needs the WebView2 Evergreen Runtime; never call it self-contained.
- Windows registry: autostart is the `BitBurn` value under `HKCU\...\CurrentVersion\Run` with
  `--bitburn-autostart` (starts hidden); the context menu lives under
  `HKCU\Software\Classes\*\shell\BitBurn` and `Directory\shell\BitBurn` and launches
  `--context-wipe`. The installer asks before adding it and the uninstaller removes it. A second
  launch is forwarded to the running instance. The theme is kept in webview `localStorage`.
- The tray needs the bundled icons; missing icons fail start-up.
- WebdriverIO uses the classic protocol; keep the `@puppeteer/browsers` 3.x override (it avoids a
  vulnerable ZIP extraction dependency).
- `cargo audit` reports GTK3 maintenance warnings and RUSTSEC-2024-0429 (GLib) from Tauri's Linux
  stack with no compatible fix; do not describe the audit as clean.

## Open questions

- No code signing is configured.
- `main.rs` still holds the overwrite patterns and orchestration; splitting it was deferred.

## Where longer material lives

- `README.md`: features, algorithms, usage, security considerations, verification.
- `ARCHITECTURE.md`: components, file-opening and deletion model, test layers.
- `CONTEXT.md`: decisions in force with reasons. `CHANGELOG.md`: release notes.
- `TEST_RUNNER_README.md`: `src-tauri/test_runner.ps1`, a PowerShell wrapper for Vitest and
  cargo tests. `CurrentProgress.md`: an older progress note.
