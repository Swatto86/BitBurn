# Changelog

## 1.0.2

- Propagate cancellation into the active file writer and wait for backend completion in the UI.
- Refuse concurrent wipe operations and bind each execution to a single-use confirmation of its exact request.
- Reject symbolic links, reparse points, hard links and unsupported paths at file opening; use directory handles to limit path replacement races.
- Preserve files when an overwrite or directory traversal fails; remove only empty directories.
- Use an anonymous temporary file for free-space wiping, with automatic cleanup and a 64 MiB reserve.
- Enforce pass limits, normalize fixed-algorithm progress, handle empty files, and correct Gutmann patterns across buffer boundaries.
- Restore file-size warning commands, preserve filename delimiters in Explorer arguments, and stop autostart status checks from rewriting startup entries.
- Replace misleading NIST Purge and unrecoverability claims with accurate overwrite descriptions.
- Update vulnerable JavaScript dependencies and add real desktop acceptance coverage.

Distribution: Windows installers and a standalone executable. The standalone executable requires Microsoft WebView2 to be installed; it is not a self-contained runtime bundle.
