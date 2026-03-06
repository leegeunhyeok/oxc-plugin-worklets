#!/usr/bin/env bash

set -euo pipefail

N=100
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

echo "=== Benchmark: Babel vs oxc ($N iterations) ==="
echo ""

# Check babel deps
if [ ! -d "$SCRIPT_DIR/babel/node_modules" ]; then
  echo "Error: bench/babel/node_modules not found. Run 'just setup' first."
  exit 1
fi

# Build oxc bench binary
echo "Building oxc bench binary (release)..."
cargo build --release --manifest-path "$SCRIPT_DIR/oxc/Cargo.toml" 2>&1 | tail -1
echo ""

# Run Babel benchmark
echo "--- Babel ---"
node "$SCRIPT_DIR/babel/run.mjs" "$N"
echo ""

# Run oxc benchmark
echo "--- oxc ---"
"$SCRIPT_DIR/oxc/target/release/bench_oxc" "$N"
echo ""
