#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
npm run test:ui
npm exec tsc -- --project tsconfig.build.json --noEmit
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --workspace --all-targets
npm run tauri -- build --debug --no-bundle
npm run test:e2e
npm audit --audit-level=low
cargo audit --file src-tauri/Cargo.lock
