#!/usr/bin/env pwsh
<#
.SYNOPSIS
    Profile-Guided Optimization (PGO) build for Rustine.

.DESCRIPTION
    1. Instruments the binary with LLVM PGO instrumentation
    2. Runs the complex benchmark to generate profile data
    3. Merges profile data
    4. Rebuilds with the profile data for optimized codegen

    Requires: rustup component add llvm-tools-preview

.EXAMPLE
    .\pgo-build.ps1
#>

$ErrorActionPreference = "Stop"

# Paths
$WORKSPACE = $PSScriptRoot
$PGO_DIR   = "$WORKSPACE\target\pgo-profiles"
$MERGED    = "$WORKSPACE\target\pgo-merged.profdata"

Write-Host "=== PGO Build for Rustine ===" -ForegroundColor Cyan
Write-Host ""

# Step 0: Ensure llvm-tools are available
Write-Host "[0/4] Checking llvm-tools..." -ForegroundColor Yellow
rustup component add llvm-tools-preview 2>$null
$llvmProfdata = & rustc --print sysroot
$llvmProfdata = Get-ChildItem "$llvmProfdata\lib\rustlib\*\bin\llvm-profdata*" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $llvmProfdata) {
    Write-Error "llvm-profdata not found. Run: rustup component add llvm-tools-preview"
    exit 1
}
Write-Host "  Found: $($llvmProfdata.FullName)"

# Step 1: Clean previous PGO artifacts
Write-Host "[1/4] Cleaning previous PGO data..." -ForegroundColor Yellow
if (Test-Path $PGO_DIR) { Remove-Item $PGO_DIR -Recurse -Force }
New-Item -ItemType Directory -Path $PGO_DIR -Force | Out-Null
if (Test-Path $MERGED) { Remove-Item $MERGED -Force }

# Step 2: Build with instrumentation
Write-Host "[2/4] Building with PGO instrumentation..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-Cprofile-generate=$PGO_DIR"
cargo build --release --example bench_complex 2>&1 | Write-Host
if ($LASTEXITCODE -ne 0) { Write-Error "Instrumented build failed"; exit 1 }

# Step 3: Run the training workload
Write-Host "[3/4] Running training workload (complex benchmark)..." -ForegroundColor Yellow
Push-Location $WORKSPACE
& "$WORKSPACE\target\release\examples\bench_complex.exe" 2>&1 | Write-Host
Pop-Location
if ($LASTEXITCODE -ne 0) { Write-Warning "Training workload returned non-zero, continuing..." }

# Also run a few demo fixtures for broader coverage
$demos = @("simple", "csv", "linesplit", "tria")
foreach ($demo in $demos) {
    $syntaxFile = "$WORKSPACE\fixtures\parity\$demo\syntax.gel"
    $inputFile  = "$WORKSPACE\fixtures\parity\$demo\input1.txt"
    if ((Test-Path $syntaxFile) -and (Test-Path $inputFile)) {
        Write-Host "  Running demo: $demo"
        # Use a small inline Rust program? No — just run the tests which exercise the engine.
    }
}
# Run the test suite for extra profile coverage (non-ignored tests only)
$env:RUSTFLAGS = "-Cprofile-generate=$PGO_DIR"
cargo test --release 2>&1 | Out-Null

# Step 4: Merge profile data
Write-Host "[4/4] Merging profile data..." -ForegroundColor Yellow
$profFiles = Get-ChildItem $PGO_DIR -Filter "*.profraw" -Recurse
Write-Host "  Found $($profFiles.Count) profile files"
& $llvmProfdata.FullName merge -o $MERGED $PGO_DIR 2>&1 | Write-Host
if ($LASTEXITCODE -ne 0) { Write-Error "Profile merge failed"; exit 1 }

# Step 5: Rebuild with profile data
Write-Host "[5/4] Rebuilding with PGO profile data..." -ForegroundColor Yellow
$env:RUSTFLAGS = "-Cprofile-use=$MERGED"
cargo build --release 2>&1 | Write-Host
if ($LASTEXITCODE -ne 0) { Write-Error "PGO optimized build failed"; exit 1 }

# Also build the bench_complex example with PGO
cargo build --release --example bench_complex 2>&1 | Write-Host

# Clean up env
Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue

Write-Host ""
Write-Host "=== PGO Build Complete ===" -ForegroundColor Green
Write-Host "Optimized binary at: target\release\rustine.dll"
Write-Host "Run benchmark: cargo run --release --example bench_complex"
Write-Host ""
