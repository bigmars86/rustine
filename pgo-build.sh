#!/usr/bin/env bash
# Profile-Guided Optimization (PGO) build for Rustine.
#
# 1. Instruments the binary with LLVM PGO instrumentation
# 2. Runs the complex benchmark to generate profile data
# 3. Merges profile data
# 4. Rebuilds with the profile data for optimized codegen
#
# Requires: rustup component add llvm-tools-preview
#
# Usage:
#   ./pgo-build.sh

set -euo pipefail

# Paths
WORKSPACE="$(cd "$(dirname "$0")" && pwd)"
PGO_DIR="$WORKSPACE/target/pgo-profiles"
MERGED="$WORKSPACE/target/pgo-merged.profdata"

echo "=== PGO Build for Rustine ==="
echo ""

# Step 0: Ensure llvm-tools are available
echo "[0/4] Checking llvm-tools..."
rustup component add llvm-tools-preview 2>/dev/null || true

SYSROOT="$(rustc --print sysroot)"
LLVM_PROFDATA="$(find "$SYSROOT/lib" -name 'llvm-profdata' -type f 2>/dev/null | head -1)"
if [ -z "$LLVM_PROFDATA" ]; then
    echo "ERROR: llvm-profdata not found. Run: rustup component add llvm-tools-preview" >&2
    exit 1
fi
echo "  Found: $LLVM_PROFDATA"

# Step 1: Clean previous PGO artifacts
echo "[1/4] Cleaning previous PGO data..."
rm -rf "$PGO_DIR"
mkdir -p "$PGO_DIR"
rm -f "$MERGED"

# Step 2: Build with instrumentation
echo "[2/4] Building with PGO instrumentation..."
RUSTFLAGS="-Cprofile-generate=$PGO_DIR" cargo build --release --example bench_complex

# Step 3: Run the training workload
echo "[3/4] Running training workload (complex benchmark)..."
cd "$WORKSPACE"
"$WORKSPACE/target/release/examples/bench_complex" || echo "  (training workload returned non-zero, continuing)"

# Run the test suite for extra profile coverage (non-ignored tests only)
RUSTFLAGS="-Cprofile-generate=$PGO_DIR" cargo test --release 2>/dev/null || true

# Step 4: Merge profile data
echo "[4/4] Merging profile data..."
PROF_COUNT="$(find "$PGO_DIR" -name '*.profraw' | wc -l)"
echo "  Found $PROF_COUNT profile files"
"$LLVM_PROFDATA" merge -o "$MERGED" "$PGO_DIR"

# Step 5: Rebuild with profile data
echo "[5/4] Rebuilding with PGO profile data..."
RUSTFLAGS="-Cprofile-use=$MERGED" cargo build --release
RUSTFLAGS="-Cprofile-use=$MERGED" cargo build --release --example bench_complex

echo ""
echo "=== PGO Build Complete ==="
echo "Optimized library at: target/release/librustine.so"
echo "Run benchmark: cargo run --release --example bench_complex"
echo ""
