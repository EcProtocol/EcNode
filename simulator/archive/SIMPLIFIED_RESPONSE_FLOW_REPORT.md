# Simplified Response-Flow Report

This report captures the steady-state simulator follow-up after simplifying the commit flow to:

- deterministic outward pair polling with a per-role sequence counter
- no pending-state fast replies
- no reply on first block arrival
- terminal-state pushes only (`Commit` or `Blocked`)
- bundled blocked replies that may include a better direct contender
- pause token-role polling once tally drops below `-2`
- when a blocked reply includes a better direct contender, send `0` if that contender is itself already `Blocked`

The corresponding design note is:

- [`Design/response_driven_commit_flow.md`](../Design/response_driven_commit_flow.md)

## Test Matrix

All runs in this round used:

- `vote targets = 2`
- batching enabled
- vote replies standalone
- `cross_dc_normal`

### Steady-state honest

Command:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state
```

Result:

- `1255` committed, `245` pending
- latency `avg 10.9`, `p50 10`, `p95 14`
- wire messages `6.40M`
- total block-message factor vs role-sum ideal: `4.83x`
- gradient locality `0.933`

### Steady-state with 25% conflict families

Command:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NEIGHBORHOOD_WIDTH=6 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

Result:

- `1235` committed, `652` pending
- latency `avg 12.2`, `p50 10`, `p95 14`
- wire messages `7.82M`
- `74` highest-majority families
- `268` stalled-no-majority families
- `107` lower-owner-commit families
- `0` multi-owner-commit families

### Corrected ring depth estimate

For the corrected steady-state ring:

- `192` peers
- guaranteed `±8` neighbors
- linear fade to zero by `±16`
- vote-eligible host width `±6`

a simple topology-only routing model gives:

- expected farthest connected peer on one side: about `12.8` rank steps
- average inward depth to host core for random origin / target pairs: `3.7`
- `p50` inward depth: `4`
- `p95` inward depth: `7`

That is a useful consistency check against the steady-state latency numbers:

- most honest traffic should still settle in a small number of inward and outward layers
- if latency grows a lot while depth stays low, the problem is probably not the gradient itself

## Readout

This follow-up keeps the simplified protocol viable, but sharpens the trade:

- on honest steady-state traffic it stays healthy and efficient
- under conflict it avoids multi-owner commits in this steady-state run, but still stalls too many families
- the new `-2` pause rule reduces unnecessary pumping in obviously losing directions
- the blocked-chain reply rule is cleaner semantically, but it does not by itself solve conflict convergence

The strongest current interpretation is:

- the simplified protocol is clean and stable enough to serve as a baseline
- poor performance is likely no longer coming from reply complexity alone
- remaining bottlenecks are more likely in:
  - how conflict knowledge turns into actual majority convergence
  - how live graph shape under churn deviates from the corrected ring
  - sync / election overhead outside this steady-state harness
  - conflict convergence itself
