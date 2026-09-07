# Architecture

BitBurn uses a React/TypeScript UI and Tauri 2 Rust commands. Wipe workers run on the blocking pool. `safety.rs` owns filesystem access, process-wide exclusion and short-lived, single-use confirmation receipts. Confirmation binds the window, operation, exact paths, algorithm and pass count.

Unix file opening traverses directory descriptors with no-follow flags; Windows holds parent handles and refuses reparse points and multiply-linked files. Windows deletion uses the open file handle. Files are deleted only after the overwrite completes. Directory cleanup removes empty directories only. Free-space wiping uses one anonymous temporary-file handle through filling and overwriting.

`main.rs` retains the established overwrite patterns and command orchestration, including existing regression tests; further mechanical module splitting is outside the security patch. `platform/` contains Windows registry integration. `ui.rs` wires tray and window lifecycle.

React tests cover UI state. Rust tests cover overwrite and safety rules. `e2e/desktop.mjs` drives the real webview and native confirmation with disposable files and isolated browser state. No test-only wipe implementation is compiled into the app.
