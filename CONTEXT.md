# Project context

- Work directly on `main`; release tags use `v<version>`.
- Existing published target is Windows x64. Linux is used for local development and real-webview verification.
- File overwrites are destructive but are not guaranteed media sanitization, especially on SSDs and copy-on-write storage. Preserve the internal algorithm enum names for compatibility while showing accurate overwrite labels.
- Full Linux verification: `bash scripts/verify.sh`. Windows CI independently checks the Rust/backend and real desktop workflow. Native confirmations are exercised by the harness; no production files are used.
- WebdriverIO uses the classic protocol. Its browser-downloader dependency is overridden to the maintained 3.x release to avoid a vulnerable ZIP extraction dependency; the suite uses installed native drivers, not automatic browser downloads.
- The current release workflow historically publishes a raw Windows executable requiring WebView2. A release must not claim this is self-contained without bundled-runtime verification. No signing-secret names were configured when this security patch was prepared.
- RustSec reports upstream GTK3 maintenance warnings and GLib VariantStrIter soundness advisory RUSTSEC-2024-0429 in Tauri’s Linux dependency stack. No compatible patched GLib 0.18 release exists; upgrading GLib alone is incompatible with the current GTK3 bindings. Do not describe the audit as warning-free.
