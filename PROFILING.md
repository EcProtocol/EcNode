# Profiling ecRust Simulations

## Quick Performance Check (Works in Devcontainer)

Use the `profiling_runner` to get basic performance metrics:

```bash
cargo run --bin profiling_runner --release scenarios/bootstrap.yaml
```

This shows:
- Time breakdown (I/O, parsing, simulation)
- Performance metrics (rounds/sec, messages/sec)
- Time per round and per message

**Example output:**
```
Performance:
  Rounds/sec:      26.7
  Messages/sec:    37699
  Time/round:      37.441ms
  Time/message:    26526ns
```

## Detailed Profiling (macOS only)

For detailed function-level profiling showing where time is spent inside the simulation:

### On Your Mac

1. **One-time setup:**
   ```bash
   xcode-select --install  # If not already installed
   cargo install cargo-instruments
   ```

2. **Run profiling:**
   ```bash
   ./profile_on_mac.sh scenarios/bootstrap.yaml
   ```

This will:
- Build with debug symbols
- Profile the simulation using macOS Instruments
- Open a `.trace` file with detailed performance data

### What to Look For in Instruments

Key functions that will show up:
- `handle_answer`, `handle_query`, `handle_referral` - Message processing
- `tick_elections` - Election logic and proof-of-storage
- `update_peer_state` - Peer state management
- `deliver_message` - Message routing
- `calculate_*` - Statistics computation

The flame graph will show you the call hierarchy and where CPU time is actually spent.

## Why Can't We Profile in Devcontainer?

The devcontainer runs Linux on a Mac host, but profiling tools like `perf` need direct kernel access which isn't available in Docker containers. macOS has its own profiling infrastructure (Instruments) that works natively.

## Alternative: Add Internal Instrumentation

If you want profiling in the devcontainer without moving to Mac, we can add detailed timing points throughout the code to measure:
- Time in election processing
- Time in message delivery by type
- Time in peer state updates
- Time in statistics calculation

Let me know if you want this added!
