# BitBurn Test Runner

PowerShell script for running comprehensive tests on the BitBurn application (frontend UI + backend Rust).

## Location

```
BitBurn/src-tauri/test_runner.ps1
```

## Quick Start

### Run All Tests (Frontend + Backend)

```powershell
.\src-tauri\test_runner.ps1
```

### Run Only Frontend UI Tests

```powershell
.\src-tauri\test_runner.ps1 -UIOnly
```

### Run Only Backend Rust Tests

```powershell
.\src-tauri\test_runner.ps1 -BackendOnly
```

## Options

| Parameter | Description | Example |
|-----------|-------------|---------|
| `-Verbose` | Show detailed test output including console logs | `.\test_runner.ps1 -Verbose` |
| `-Coverage` | Generate code coverage report for frontend tests | `.\test_runner.ps1 -Coverage` |
| `-Watch` | Run frontend tests in watch mode (re-run on file changes) | `.\test_runner.ps1 -Watch -UIOnly` |
| `-UIOnly` | Run only frontend UI tests (Vitest) | `.\test_runner.ps1 -UIOnly` |
| `-BackendOnly` | Run only backend Rust tests (Cargo) | `.\test_runner.ps1 -BackendOnly` |

## Combined Options

```powershell
# Verbose output for both frontend and backend
.\src-tauri\test_runner.ps1 -Verbose

# Frontend tests with coverage report
.\src-tauri\test_runner.ps1 -UIOnly -Coverage

# Backend tests with detailed output
.\src-tauri\test_runner.ps1 -BackendOnly -Verbose

# Watch mode for frontend development
.\src-tauri\test_runner.ps1 -UIOnly -Watch
```

## Output

The script provides:

### ✅ Environment Check
- Node.js version
- npm version  
- Cargo/Rust version (for backend tests)

### 📊 Test Results
- **Frontend**: Number of passed/failed UI tests
- **Backend**: Number of passed/failed Rust tests

### ⏱️ Summary
- Total tests passed/failed
- Execution duration
- Overall status (SUCCESS ✓ or FAILURE ✗)

## Example Output

```
============================================
  BitBurn Test Suite Runner
============================================

  Project Root: C:\Users\...\BitBurn
  Started: 2024-01-15 10:30:00

>>> Environment Check

✓ Node.js: v20.10.0
✓ npm: v10.2.3
✓ Cargo: cargo 1.75.0

============================================
  Frontend UI Tests (Vitest)
============================================

>>> Running UI Test Suite...

 ✓ src/App.test.tsx (45 tests) 4631ms
   ✓ Initial Render (8)
   ✓ Theme Toggle (2)
   ✓ Algorithm Selection (6)
   ...

 Test Files  1 passed (1)
      Tests  45 passed (45)

✓ Frontend tests completed successfully

============================================
  Backend Rust Tests (Cargo)
============================================

>>> Running Rust Test Suite...

running 6 tests
test tests::test_nonexistent_file ... ok
test tests::test_invalid_passes ... ok
test tests::test_nist_clear_wipe ... ok
test tests::test_nist_purge_wipe ... ok
test tests::test_random_wipe ... ok
test tests::test_gutmann_wipe ... ok

test result: ok. 6 passed; 0 failed

✓ Backend tests completed successfully

============================================
  Test Summary
============================================

  Frontend Tests:
  Passed: 45

  Backend Tests:
  Passed: 6

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Duration: 12.45s

Total: 51/51 tests passed

✓ ALL TESTS PASSED! ✓
```

## Exit Codes

- `0` - All tests passed ✅
- `1` - Some tests failed or environment check failed ❌

## Prerequisites

### For Frontend Tests
- Node.js (v18+)
- npm

### For Backend Tests
- Rust toolchain (rustc, cargo)
- Install from: https://rustup.rs/

## Troubleshooting

### "Cargo not found"

Install Rust:
```powershell
winget install Rustlang.Rustup
```

Or download from: https://rustup.rs/

### "npm not found"

Install Node.js:
```powershell
winget install OpenJS.NodeJS
```

Or download from: https://nodejs.org/

### Permission Denied

Enable script execution:
```powershell
Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope CurrentUser
```

### Script Not Running

Make sure you're in the project root or specify the full path:
```powershell
# From project root
.\src-tauri\test_runner.ps1

# Or use full path
C:\Path\To\BitBurn\src-tauri\test_runner.ps1
```

## Alternative: Using npm

You can also run tests using npm commands:

```bash
# All tests (same as test_runner.ps1)
npm test

# Frontend only
npm run test:ui

# Backend only
cd src-tauri && cargo test
```

## CI/CD Integration

The test runner can be integrated into CI/CD pipelines:

```yaml
# Example GitHub Actions workflow
- name: Run Tests
  run: pwsh -File src-tauri/test_runner.ps1
  shell: powershell
```

## Test Coverage

### Frontend Tests (45 tests)
- ✓ Initial Render & UI Elements
- ✓ Theme Toggle & Persistence
- ✓ Algorithm Selection (NIST Clear, NIST Purge, Gutmann, Random)
- ✓ Pass Count Input Validation
- ✓ File/Folder Selection
- ✓ Drag & Drop
- ✓ Wipe Operations
- ✓ Progress Display
- ✓ Result Messages
- ✓ Free Space Wiping
- ✓ Cancel Operations
- ✓ Accessibility
- ✓ Responsive UI

### Backend Tests (6 tests)
- ✓ NIST 800-88 Clear Algorithm
- ✓ NIST 800-88 Purge Algorithm
- ✓ Gutmann 35-Pass Algorithm
- ✓ Random Pass Algorithm
- ✓ Input Validation
- ✓ Error Handling

## License

This test runner is part of the BitBurn project.