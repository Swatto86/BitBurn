# BitBurn - Secure File & Drive Wiping Utility

BitBurn is a modern, secure file and drive wiping utility built with Rust and Tauri. It provides multiple industry-standard data erasure algorithms and a user-friendly interface for securely wiping files, folders, and drive free space.

![BitBurn Logo](./src-tauri/icons/128x128.png)

## Features

- **Multiple Wiping Algorithms:**
  - Single-pass overwrite (1-pass)
  - Three-pass overwrite (3-pass)
  - Gutmann (35-pass)
  - Custom Random (1-35 passes)

- **Explorer Context Menu (Windows):**
  - Optional BitBurn entry on file/folder right-click that launches directly into shredding
  - Managed from the tray icon via a single toggle (Enable/Disable Explorer Context Menu)
  - Multi-selection support with path validation

- **Autostart (Windows):**
  - Tray toggle to enable/disable starting BitBurn with Windows

- **Flexible Wiping Options:**
  - Single file wiping
  - Multiple file selection
  - Folder/directory wiping
  - Drive free space wiping
  - Drag and drop support

- **Security Features:**
  - Secure random number generation
  - Proper file synchronization
  - Checked overwrite writes and synchronization
  - Errors reported rather than deleting failed files

- **User Interface:**
  - Modern, intuitive design
  - Dark/Light theme support
  - Real-time progress tracking
  - Detailed operation feedback
  - System tray integration (tray menu includes context menu toggle and autostart toggle on Windows)
  - Cancellable operations

  ## Quick Start

  - Install dependencies: `npm install`
  - Dev (frontend + Tauri): `npm run tauri dev`
  - Build production bundle: `npm run tauri build`
  - Backend/frontend tests: `npm test` (runs vitest and cargo tests)

  ## Platform Notes

  - System tray requires bundled icons; missing icons cause tray init to fail fast. Ensure `icons/` is present when packaging.

## Wiping Algorithms

### Single-pass overwrite
- Single pass with zeros
- Writes zeros to the selected logical file bytes. It cannot reach remapped storage, snapshots or backups.

### Three-pass overwrite
- Three-pass overwrite:
  1. All zeros
  2. All ones
  3. Random data
- This is a file overwrite, not a NIST Purge operation. Use a device-appropriate sanitization tool for purging media.

### Gutmann Method
- 35-pass overwrite with specific patterns
- Designed for magnetic media
- Includes:
  - 4 random passes
  - 27 specific patterns
  - 4 final random passes
- A legacy magnetic-media pattern; extra passes do not make SSD sanitization reliable.

### Random
- User-configurable number of passes (1-35)
- Cryptographically secure random data
- Customizable security level
- Suitable for modern storage devices

## Usage

### Windows Explorer Context Menu & Autostart
- Right-click the tray icon and use the toggles to enable/disable the Explorer context menu and Windows autostart.
- In Explorer, right-click files/folders and choose **BitBurn**; the app opens a modal where you can pick the wipe algorithm and passes before shredding.

1. **File/Folder Wiping:**
   - Click "Wipe Files/Folders" or drag files into the application
   - Select files/folders to wipe
   - Choose wiping algorithm
   - Confirm operation
   - Monitor progress

2. **Drive Free Space Wiping:**
   - Click "Wipe Drive Free Space"
   - Select target drive
   - Choose wiping algorithm
   - Confirm operation
   - Monitor progress

## Security Considerations

- Overwriting does not guarantee recovery is impossible: SSD wear levelling, copy-on-write filesystems, snapshots, cloud versions and backups can retain data.
- Operations cannot be undone
- Administrator privileges may be required for some operations
- Network paths are not supported for security reasons
- Symlinks are not followed to prevent security issues

## System Requirements

- Windows 10 or later
- Administrator privileges for system files/drives
- Sufficient system resources for large operations

## Technical Details

- Built with Rust and Tauri 2.0
- Uses cryptographically secure random number generation
- Implements proper file synchronization
- Includes comprehensive error handling
- Failed overwrites retain their files; cancellation can leave partially overwritten contents
- Includes extensive test coverage

## Development

Built using:
- Rust (Backend)
- Tauri 2.0 (Framework)
- React + TypeScript (Frontend)
- TailwindCSS (Styling)

## Warning

⚠️ BitBurn is designed to permanently destroy data. Always verify selected files/drives before confirming operations. Treat the selected data as lost after starting; copies elsewhere may survive.

## Author

BitBurn is developed by Steve Watson. Visit [swatto.co.uk](https://swatto.co.uk) for more projects and information.

## License

BitBurn is licensed under the [MIT License](LICENSE).

## Acknowledgments

- NIST 800-88 Guidelines for Media Sanitization
- Peter Gutmann's secure deletion paper
- The Rust and Tauri communities

## Verification

Run `bash scripts/verify.sh` on Linux after `npm ci`. The full gate includes strict Rust lint, Rust and React regressions, a debug desktop build, and real-webview acceptance with disposable files. The desktop suite requires `tauri-driver`, a matching platform WebDriver, and `xdotool` on Linux. Close other BitBurn instances before running it.

Free-space wiping keeps a 64 MiB reserve and cleans up its anonymous temporary file on ordinary success, failure and cancellation. Abrupt process termination or power loss can still interrupt writes.

The names `NistClear` and `NistPurge` remain internal compatibility identifiers; they are not certification claims. See [NIST SP 800-88 Rev. 2](https://csrc.nist.gov/pubs/sp/800/88/r2/final) for media-sanitization guidance.
