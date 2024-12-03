# BitBurn

BitBurn is a secure file and free space wiping utility that implements multiple industry-standard data sanitization algorithms. It provides a user-friendly interface while ensuring thorough data destruction.

## Features

### Secure File Wiping
- Individual file and directory wiping
- Recursive directory handling
- Multiple wiping algorithms:
  - **Basic** (1 pass): Simple zero-fill
  - **DOD 5220.22-M** (3 passes): Meets US Department of Defense standards
  - **Gutmann** (35 passes): Peter Gutmann's secure deletion algorithm
  - **Random** (N passes): User-specified number of random data passes

### Free Space Wiping
- Securely wipes free space on selected drives
- Prevents recovery of previously deleted files
- Uses the same secure algorithms as file wiping
- Progress tracking and cancellation support

### Security Features
- Verification of write operations
- Protection against accidental system file deletion
- Clear warnings and confirmations before operations
- Safe handling of symbolic links
- Proper cleanup of temporary files

### User Interface
- Modern, intuitive graphical interface
- Real-time progress tracking
- System tray integration
- Operation cancellation support
- Detailed status reporting

## Usage

1. **File/Directory Wiping**:
   - Select files or directories to wipe
   - Choose your preferred wiping algorithm
   - Confirm the operation
   - Monitor progress and wait for completion

2. **Free Space Wiping**:
   - Select the target drive
   - Choose your preferred wiping algorithm
   - Confirm the operation
   - Monitor progress and wait for completion

## Security Considerations

- BitBurn is designed for secure data destruction on standard storage devices
- Some storage devices (SSDs, flash drives) may retain data due to wear leveling
- Administrative privileges may be required for certain operations
- Always verify critical files are backed up before wiping
- Consider physical destruction for highly sensitive data

## ⚠️ Warning

BitBurn permanently destroys data. Wiped files and free space contents CANNOT be recovered. Use with caution and always verify your selections before confirming operations.

## License

[MIT License](LICENSE)

## Acknowledgments

BitBurn implements several industry-standard data sanitization methods:
- DOD 5220.22-M (US Department of Defense)
- Gutmann method (Peter Gutmann)
