#!/bin/bash
# Profiling script for macOS
#
# This script profiles ecRust scenarios using cargo-instruments (macOS profiling tool)
#
# Prerequisites on Mac:
#   1. Install Xcode Command Line Tools: xcode-select --install
#   2. Install cargo-instruments: cargo install cargo-instruments
#
# Usage:
#   ./profile_on_mac.sh scenarios/bootstrap.yaml

set -e

SCENARIO="${1:-scenarios/bootstrap.yaml}"

if [ ! -f "$SCENARIO" ]; then
    echo "Error: Scenario file not found: $SCENARIO"
    echo "Usage: $0 <scenario.yaml>"
    exit 1
fi

echo "═══════════════════════════════════════════════════════"
echo "  ecRust Profiling Script for macOS"
echo "═══════════════════════════════════════════════════════"
echo ""
echo "Scenario: $SCENARIO"
echo ""

# Check if cargo-instruments is installed
if ! command -v cargo-instruments &> /dev/null; then
    echo "cargo-instruments not found. Installing..."
    cargo install cargo-instruments
fi

# Add debug symbols to release builds
export CARGO_PROFILE_RELEASE_DEBUG=true

echo "Building with debug symbols..."
cargo build --release --bin profiling_runner

echo ""
echo "Running profiling with Instruments..."
echo "This will generate a .trace file you can open in Xcode Instruments"
echo ""

# Run cargo-instruments with time profiler template
# The output will be a .trace file that can be opened in Instruments.app
cargo instruments --release --bin profiling_runner \
    --template time \
    --open \
    -- "$SCENARIO"

echo ""
echo "═══════════════════════════════════════════════════════"
echo "  Profiling complete!"
echo ""
echo "The profiling results (.trace file) should open automatically"
echo "in Xcode Instruments. Look for:"
echo "  - Time spent in different functions"
echo "  - Call stacks showing what calls what"
echo "  - Hot paths (functions consuming most CPU time)"
echo ""
echo "Key functions to look for:"
echo "  - handle_answer, handle_query, handle_referral (message processing)"
echo "  - tick_elections (election logic)"
echo "  - update_peer_state (peer state management)"
echo "  - deliver_message (message delivery)"
echo "═══════════════════════════════════════════════════════"
