# Current Progress

## Completed
- Explorer context-menu registration/unregistration with CLI `--context-wipe`, path sanitization, and frontend context modal (multi-select, invalid-path feedback).
- NSIS installer checkbox and CLI support to opt into Explorer context menu; Windows CI covers registry helpers via BITBURN_CONTEXT_ROOT.
- Frontend now guards platform detection (Tauri IPC check) and falls back to UA; Windows-only UI still gated on backend platform info.
- Vitest + cargo test suites pass; manual progress/resolution callbacks wrapped via waitFor to eliminate act warnings.

## In Progress
- Full code review against architectural, security, and testing principles.

## Blockers
- None at this time.
