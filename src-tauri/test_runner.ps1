# BitBurn Comprehensive Test Runner
# Runs both frontend (UI) and backend (Rust) tests with detailed reporting

param(
    [switch]$Verbose,
    [switch]$Coverage,
    [switch]$Watch,
    [switch]$UIOnly,
    [switch]$BackendOnly
)

# Colors for output
$script:Colors = @{
    Header = "Cyan"
    Success = "Green"
    Error = "Red"
    Warning = "Yellow"
    Info = "White"
    Section = "Magenta"
}

# Test results tracking
$script:TestResults = @{
    FrontendPassed = 0
    FrontendFailed = 0
    BackendPassed = 0
    BackendFailed = 0
    StartTime = Get-Date
}

# Helper Functions
function Write-Header {
    param([string]$Text)
    Write-Host ""
    Write-Host "============================================" -ForegroundColor $Colors.Header
    Write-Host "  $Text" -ForegroundColor $Colors.Header
    Write-Host "============================================" -ForegroundColor $Colors.Header
    Write-Host ""
}

function Write-Section {
    param([string]$Text)
    Write-Host ""
    Write-Host ">>> $Text" -ForegroundColor $Colors.Section
    Write-Host ""
}

function Write-Success {
    param([string]$Text)
    Write-Host "✓ $Text" -ForegroundColor $Colors.Success
}

function Write-Failure {
    param([string]$Text)
    Write-Host "✗ $Text" -ForegroundColor $Colors.Error
}

function Write-InfoLine {
    param([string]$Text)
    Write-Host "  $Text" -ForegroundColor $Colors.Info
}

function Test-Command {
    param([string]$Command)
    try {
        Get-Command $Command -ErrorAction Stop | Out-Null
        return $true
    } catch {
        return $false
    }
}

# Main Script
Clear-Host
Write-Header "BitBurn Test Suite Runner"

# Get project root (parent of src-tauri)
$scriptPath = Split-Path -Parent $MyInvocation.MyCommand.Path
$projectRoot = Split-Path -Parent $scriptPath

Write-InfoLine "Project Root: $projectRoot"
Write-InfoLine "Started: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"

# Environment Check
Write-Section "Environment Check"

# Check Node.js
if (Test-Command "node") {
    $nodeVersion = node --version
    Write-Success "Node.js: $nodeVersion"
} else {
    Write-Failure "Node.js not found. Please install Node.js from https://nodejs.org/"
    exit 1
}

# Check npm
if (Test-Command "npm") {
    $npmVersion = npm --version
    Write-Success "npm: v$npmVersion"
} else {
    Write-Failure "npm not found"
    exit 1
}

# Check Rust/Cargo (only if running backend tests)
if (-not $UIOnly) {
    if (Test-Command "cargo") {
        $cargoVersion = cargo --version
        Write-Success "Cargo: $cargoVersion"
    } else {
        Write-Failure "Cargo not found. Please install Rust from https://rustup.rs/"
        if (-not $BackendOnly) {
            Write-Warning "Skipping backend tests..."
            $BackendOnly = $false
            $UIOnly = $true
        } else {
            exit 1
        }
    }
}

# Change to project root
Set-Location $projectRoot

# Run Frontend Tests
if (-not $BackendOnly) {
    Write-Header "Frontend UI Tests (Vitest)"

    Write-Section "Running UI Test Suite..."

    $testCommand = "npm"
    $testArgs = @("run", "test:ui")

    if ($Watch) {
        $testArgs = @("run", "test:ui", "--", "--watch")
    }

    if ($Coverage) {
        $testArgs += @("--", "--coverage")
    }

    if ($Verbose) {
        $testArgs += @("--", "--reporter=verbose")
    }

    # Run the test
    $output = & $testCommand $testArgs 2>&1 | Out-String
    Write-Host $output

    # Parse results - look for "Tests  45 passed (45)" format
    if ($output -match "Tests\s+(\d+)\s+passed") {
        $script:TestResults.FrontendPassed = [int]$Matches[1]
    }

    if ($output -match "Tests\s+(\d+)\s+failed") {
        $script:TestResults.FrontendFailed = [int]$Matches[1]
    }

    if ($LASTEXITCODE -eq 0) {
        Write-Success "Frontend tests completed successfully"
    } else {
        Write-Failure "Frontend tests failed"
    }

    Write-Host ""
}

# Run Backend Tests
if (-not $UIOnly) {
    Write-Header "Backend Rust Tests (Cargo)"

    # Change to Rust directory
    Set-Location $scriptPath

    Write-Section "Running Rust Test Suite..."

    $testCommand = "cargo"
    $testArgs = @("test")

    if ($Verbose) {
        $testArgs += @("--", "--nocapture", "--test-threads=1")
    }

    # Run the test
    $output = & $testCommand $testArgs 2>&1 | Out-String
    Write-Host $output

    # Parse results - look for "6 passed; 0 failed" format
    if ($output -match "(\d+)\s+passed;") {
        $script:TestResults.BackendPassed = [int]$Matches[1]
    }

    if ($output -match ";\s+(\d+)\s+failed") {
        $script:TestResults.BackendFailed = [int]$Matches[1]
    }

    if ($LASTEXITCODE -eq 0) {
        Write-Success "Backend tests completed successfully"
    } else {
        Write-Failure "Backend tests failed"
    }

    # Return to project root
    Set-Location $projectRoot
}

# Summary Report
Write-Header "Test Summary"

$totalPassed = $script:TestResults.FrontendPassed + $script:TestResults.BackendPassed
$totalFailed = $script:TestResults.FrontendFailed + $script:TestResults.BackendFailed
$totalTests = $totalPassed + $totalFailed

if (-not $BackendOnly) {
    Write-InfoLine "Frontend Tests:"
    if ($script:TestResults.FrontendPassed -gt 0) {
        Write-Host "  Passed: " -NoNewline
        Write-Host $script:TestResults.FrontendPassed -ForegroundColor $Colors.Success
    }
    if ($script:TestResults.FrontendFailed -gt 0) {
        Write-Host "  Failed: " -NoNewline
        Write-Host $script:TestResults.FrontendFailed -ForegroundColor $Colors.Error
    }
}

if (-not $UIOnly) {
    Write-Host ""
    Write-InfoLine "Backend Tests:"
    if ($script:TestResults.BackendPassed -gt 0) {
        Write-Host "  Passed: " -NoNewline
        Write-Host $script:TestResults.BackendPassed -ForegroundColor $Colors.Success
    }
    if ($script:TestResults.BackendFailed -gt 0) {
        Write-Host "  Failed: " -NoNewline
        Write-Host $script:TestResults.BackendFailed -ForegroundColor $Colors.Error
    }
}

Write-Host ""
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor $Colors.Header

$elapsed = (Get-Date) - $script:TestResults.StartTime
Write-InfoLine "Duration: $($elapsed.TotalSeconds.ToString('F2'))s"

Write-Host ""
Write-Host "Total: " -NoNewline
if ($totalFailed -eq 0) {
    Write-Host "$totalPassed/$totalTests tests passed" -ForegroundColor $Colors.Success
    Write-Host ""
    Write-Success "ALL TESTS PASSED! ✓"
    Write-Host ""
    exit 0
} else {
    Write-Host "$totalPassed/$totalTests tests passed, $totalFailed failed" -ForegroundColor $Colors.Warning
    Write-Host ""
    Write-Failure "SOME TESTS FAILED ✗"
    Write-Host ""
    exit 1
}
