# BitBurn - Secure File & Drive Wiping Utility

BitBurn is a modern, secure file and drive wiping utility built with Rust and Tauri. It provides multiple industry-standard data erasure algorithms and a user-friendly interface for securely wiping files, folders, and drive free space.

![BitBurn Logo](./src-tauri/icons/128x128.png)

## Features

- **Multiple Wiping Algorithms:**
  - NIST 800-88 Clear (1-pass)
  - NIST 800-88 Purge (3-pass)
  - Gutmann (35-pass)
  - Custom Random (1-35 passes)

- **Flexible Wiping Options:**
  - Single file wiping
  - Multiple file selection
  - Folder/directory wiping
  - Drive free space wiping
  - Drag and drop support

- **Security Features:**
  - Secure random number generation
  - Proper file synchronization
  - Complete data overwriting
  - Verification of write operations

- **User Interface:**
  - Modern, intuitive design
  - Dark/Light theme support
  - Real-time progress tracking
  - Detailed operation feedback
  - System tray integration
  - Cancellable operations

## Wiping Algorithms

### NIST 800-88 Clear
- Single pass with zeros
- Quick and effective for most modern storage devices
- Suitable for non-sensitive data

### NIST 800-88 Purge
- Three-pass overwrite:
  1. All zeros
  2. All ones
  3. Random data
- Recommended for sensitive data
- Provides good balance of security and speed

### Gutmann Method
- 35-pass overwrite with specific patterns
- Designed for magnetic media
- Includes:
  - 4 random passes
  - 27 specific patterns
  - 4 final random passes
- Maximum security for legacy storage devices

### Random
- User-configurable number of passes (1-35)
- Cryptographically secure random data
- Customizable security level
- Suitable for modern storage devices

## Usage

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

- Files erased with BitBurn cannot be recovered
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
- Features atomic operations for data integrity
- Includes extensive test coverage

## Development

Built using:
- Rust (Backend)
- Tauri 2.0 (Framework)
- React + TypeScript (Frontend)
- TailwindCSS (Styling)

## Warning

⚠️ BitBurn is designed to permanently destroy data. Always verify selected files/drives before confirming operations. Wiped data cannot be recovered.

## Author

BitBurn is developed by Steve Watson. Visit [swatto.co.uk](https://swatto.co.uk) for more projects and information.

## License

BitBurn is licensed under the [MIT License](LICENSE).

## Acknowledgments

- NIST 800-88 Guidelines for Media Sanitization
- Peter Gutmann's secure deletion paper
- The Rust and Tauri communities
