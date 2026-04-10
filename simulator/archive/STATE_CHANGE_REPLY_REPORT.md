# State-Change Reply Report

This report covers the latest vote-flow change set:

- store `reply` interest directly on the raw recorded vote entry
- only send delayed follow-up to peers that actually asked for it
- push state changes back to interested peers when a block becomes:
  - known and locally votable
  - `Commit`
  - `Blocked` by a higher-ranked direct sibling
- send conflict updates as paired messages:
  - `0` on the blocked contender
  - current vote on the higher contender

Code:
- `src/ec_mempool.rs`
- `src/ec_node.rs`

The goal was to move the protocol a bit away from pure tick-driven polling and toward a more explicit request / delayed-response pattern, then test that in the order:

1. sparse steady-state, no conflict
2. sparse steady-state, conflict workload
3. integrated churn lifecycle

## Implementation Notes

The important design choice here is that there is no separate shadow structure for “who wants replies”.

Instead:

- each stored raw vote now carries the latest `reply` interest bit from that peer
- delayed replies use that same vote map
- transition-time push uses the same interested-peer set

That keeps the latest semantic vote and the latest follow-up interest in one place.

## Commands

Sparse steady-state, no conflict:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
cargo run --release --quiet --example integrated_steady_state
```

Sparse steady-state, `25%` conflict families:

```bash
EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
cargo run --release --quiet --example integrated_steady_state

EC_STEADY_STATE_ROUNDS=500 \
EC_STEADY_STATE_INITIAL_PEERS=192 \
EC_STEADY_STATE_TOPOLOGY=ring \
EC_STEADY_STATE_RING_NEIGHBORS=8 \
EC_STEADY_STATE_NETWORK_PROFILE=cross_dc_normal \
EC_STEADY_STATE_BATCHING=true \
EC_STEADY_STATE_BATCH_VOTE_REPLIES=false \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_VOTE_BALANCE_THRESHOLD=2 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.25 \
cargo run --release --quiet --example integrated_steady_state
```

Integrated churn, no conflict and `25%` conflict families:

```bash
EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
cargo run --release --quiet --example integrated_long_run

EC_LONG_RUN_ROUNDS=600 \
EC_LONG_RUN_NETWORK_PROFILE=cross_dc_normal \
EC_LONG_RUN_EXISTING_TOKEN_FRACTION=0.5 \
EC_LONG_RUN_CONFLICT_FAMILY_FRACTION=0.25 \
EC_LONG_RUN_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_long_run
```

## 1. Sparse Steady-State, No Conflict

Compared against the earlier sparse steady-state baseline in `SPARSE_STEADY_STATE_REPORT.md`.

| Case | Committed / Pending | Wire Messages | Avg Latency | p95 | Total vs Role Ideal |
| --- | ---: | ---: | ---: | ---: | ---: |
| prior `+1` | `1254 / 246` | `9.25M` | `9.9` | `13` | `10.15x` |
| state-change push `+1` | `1183 / 317` | `9.30M` | `9.2` | `11` | `7.51x` |
| prior `+2` | `1197 / 303` | `9.31M` | `12.7` | `15` | `25.55x` |
| state-change push `+2` | `1202 / 298` | `8.99M` | `12.3` | `13` | `22.14x` |

### Reading The Honest Steady-State Result

`+2` is the cleaner win:

- slightly more commits
- fewer pending
- lower latency
- lower wire traffic
- lower protocol overhead

`+1` is more mixed:

- latency improved
- message factor improved a lot
- committed count dropped

So the new state-change path clearly helps the honest base case, but the `+1` variant still trades throughput against aggressiveness.

## 2. Sparse Steady-State, Conflict Workload

Compared against the earlier blocked-transition batch results in `ADVERSARIAL_CONFLICT_REPORT.md`.

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Stalled No Majority | Any Lower Visible | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| prior `+1` blocked-transition batch | `378` | `122` | `5` | `20` | `26` | `64` | `264` | `97` | `0.78` | `12.25M` | `9.5` | `12` |
| state-change push `+1` | `353` | `123` | `7` | `10` | `48` | `70` | `238` | `77` | `0.78` | `12.08M` | `8.3` | `10` |
| prior `+2` blocked-transition batch | `362` | `124` | `2` | `11` | `30` | `63` | `240` | `111` | `0.75` | `12.07M` | `10.9` | `13` |
| state-change push `+2` | `355` | `119` | `2` | `12` | `34` | `68` | `244` | `88` | `0.77` | `12.31M` | `9.9` | `12` |

### Reading The Conflict Result

This is where the new approach becomes genuinely interesting.

`+1` improved sharply without adding wire cost:

- split end states were cut in half: `20 -> 10`
- unanimous-highest improved strongly: `26 -> 48`
- highest-majority improved: `64 -> 70`
- stalled-no-majority improved: `264 -> 238`
- any-lower-visible improved: `97 -> 77`
- latency improved: `9.5 -> 8.3`

The weaker points are:

- lower-owner commits stayed basically flat
- multi-owner commits ticked up slightly

`+2` is more mixed but still net-positive on several meaningful signals:

- lower-owner commits improved: `124 -> 119`
- unanimous-highest improved: `30 -> 34`
- highest-majority improved: `63 -> 68`
- any-lower-visible improved a lot: `111 -> 88`
- latency improved: `10.9 -> 9.9`

The weaker points are:

- split families were essentially flat to slightly worse
- stalled-no-majority was slightly worse
- wire traffic rose a little: `12.07M -> 12.31M`

So the state-change push does not “solve” conflict convergence, but it does look like a better platform than the earlier blocked-transition-only path.

## 3. Integrated Churn Lifecycle

Compared against the earlier integrated churn comparison in `INTEGRATED_CHURN_CONFLICT_REPORT.md`.

### No-Conflict Churn

| Case | Submitted | Committed | Pending | Wire Messages | Peak In-Flight | Avg Latency | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| prior no-conflict churn | `1800` | `1226` | `574` | `8.18M` | `45,273` | `24.4` | `16` | `47` |
| state-change push no-conflict churn | `1800` | `1172` | `628` | `9.07M` | `53,282` | `21.2` | `14` | `42` |

Lifecycle timing:

- late-join to connected stayed flat: `26.9 -> 26.8`
- rejoin to connected improved slightly: `20.5 -> 19.6`
- recovery changed from `2 / 1` rounds to `1 / 7` rounds across the two crash events

### Conflict Churn

| Case | Submitted | Committed | Pending | Wire Messages | Peak In-Flight | Avg Latency | p50 | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| prior conflict churn | `2286` | `1284` | `1002` | `15.73M` | `99,730` | `22.6` | `14` | `47` |
| state-change push conflict churn | `2220` | `1263` | `957` | `15.85M` | `99,651` | `18.0` | `13` | `37` |

Conflict-family outcomes:

| Case | Families | Highest Majority | Stalled No Majority | Lower Owner Commit | Multi Owner Commits | Signal Coverage |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| prior conflict churn | `486` | `254` | `203` | `114` | `52` | `0.60` |
| state-change push conflict churn | `420` | `203` | `190` | `87` | `42` | `0.64` |

Lifecycle timing:

- late-join to connected: `26.5 -> 27.4`
- rejoin to connected: `16.2 -> 21.4`
- recovery improved dramatically:
  - prior: `14` rounds and `64` rounds
  - state-change push: `1` round and `1` round

### Reading The Churn Result

The churn result is the most encouraging part of this round.

Under heavy conflict load, the new path:

- kept wire traffic in the same band
- reduced pending work
- reduced latency materially
- improved lower-owner and multi-owner bad outcomes
- improved explicit conflict-signal reach
- restored very fast recovery after crash events

The important caution is that stronger recovery does **not** mean stronger convergence on the intended contender yet.

The conflict-family picture is still mixed:

- highest-majority is not clearly stronger
- stalled families are still common
- unanimous-highest is still effectively absent under churn

So the new state-change path is a real operational improvement, but not yet a full conflict-resolution answer.

## Overall Assessment

This change set looks worth keeping.

The strongest conclusions are:

1. Carrying `reply` interest directly on the stored vote works well.
   It avoided adding a second parallel structure, and the code paths stay fairly direct.

2. State-change push is helping more than pure polling.
   The gain is strongest on:
   - honest steady-state `+2`
   - conflict steady-state
   - conflict-heavy churn recovery

3. Conflict signaling is now reaching a better milestone.
   We are not yet getting consistent convergence on the highest contender, but more peers are being told that conflict exists, with lower cost than the broader piggyback ideas.

4. This still does not justify `+1` as a safe production threshold.
   `+1` remains interesting as a performance probe, not as a settled policy.

The current read is that this change improves the platform we build from next. It does not finish the conflict problem, but it gives us a better base for the next experiments around:

- dialing back tick-driven pumping
- testing whether `3` targets becomes more attractive with stronger delayed response
- improving how the highest contender itself is spread during conflict without going back to expensive broad piggybacking

## Follow-On Tuning: `3` Targets And Resend Cooldown

After the state-change push landed, I ran one follow-on sweep on the safer `+2` threshold:

- compare `2` vs `3` vote targets
- test a first resend throttle: `vote_request_resend_cooldown = 2`
- keep batching on and vote replies standalone

### Sparse Steady-State, No Conflict

Reference point from above:

- `2` targets, state-change push: `1202` committed, `298` pending, `8.99M` wire, avg latency `12.3`, p95 `13`

Follow-on results:

| Case | Committed / Pending | Wire Messages | Avg Latency | p95 | Total vs Role Ideal |
| --- | ---: | ---: | ---: | ---: | ---: |
| `3` targets | `1178 / 322` | `10.94M` | `9.7` | `11` | `13.09x` |
| `3` targets + cooldown `2` | `1154 / 346` | `8.04M` | `12.0` | `12` | `15.21x` |

Reading:

- plain `3` targets improved latency a lot, but paid with more wire traffic and slightly fewer commits
- adding the resend cooldown cut wire traffic sharply, but gave back too much on completed work

### Sparse Steady-State, Conflicts

Reference point from above:

- `2` targets, state-change push: `355` families, `119` lower-owner-commit, `12` split, `68` highest-majority, `244` stalled-no-majority, `12.31M` wire, avg latency `9.9`, p95 `12`

Follow-on results:

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Highest Majority | Stalled No Majority | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `3` targets | `343` | `107` | `0` | `6` | `52` | `247` | `0.75` | `14.76M` | `9.5` | `11` |
| `3` targets + cooldown `2` | `348` | `103` | `3` | `6` | `65` | `251` | `0.79` | `11.62M` | `10.1` | `12` |

Reading:

- `3` targets did improve some conflict-damping signals:
  - lower-owner commits improved
  - split families improved
  - multi-owner commits dropped to `0`
- but it did **not** improve highest-majority or stalled-majority behavior enough to call it a convergence win
- the cooldown version trimmed traffic back down, but again gave up too much in completed work and still did not clearly improve convergence

So in steady state:

- `3` targets is interesting for latency and some conflict-damping behavior
- the first resend-cooldown attempt is not strong enough to adopt

### Integrated Churn Lifecycle With `3` Targets

I carried only the plain `3`-target variant into churn, because the cooldown version already looked weak in steady state.

Reference point from above:

- no-conflict churn, `2` targets: `1172` committed, `628` pending, `9.07M` wire, avg latency `21.2`, p95 `42`
- conflict churn, `2` targets: `1263` committed, `957` pending, `15.85M` wire, avg latency `18.0`, p95 `37`

Follow-on results:

| Case | Submitted | Committed | Pending | Wire Messages | Avg Latency | p95 | Recovery |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| no-conflict churn, `3` targets | `1800` | `1215` | `585` | `10.91M` | `19.7` | `38` | crash recovery `16` rounds, then `1` |
| conflict churn, `3` targets | `2223` | `1268` | `955` | `18.16M` | `19.9` | `38` | first crash wave did not recover during run, second recovered in `1` |

Conflict-family outcomes under churn with `3` targets:

- `423` families
- `211` highest-majority
- `180` stalled-no-majority
- `92` lower-owner-commit
- `35` multi-owner-commits
- signal coverage `0.67`

Reading:

- `3` targets under churn helps early and mid-phase responsiveness
- it does not translate into a cleaner recovery story
- under conflict churn it is still mixed:
  - fewer stalled families and fewer multi-owner commits
  - but worse latency, more wire cost, worse lower-owner commits, and worse first-wave recovery

## Tuning Assessment

The current best read is:

1. keep the state-change reply path
2. keep `2` vote targets as the lifecycle default for now
3. keep `3` targets as a useful steady-state tuning point, not as the new integrated default
4. keep the resend cooldown as an experimental knob, but do not turn it on by default yet

So the next move should not be “just crank up fanout” or “just poll less.” The state-change path is helping, but the churn results say we still need a smarter scheduler if we want those two ideas to hold up across joins, crashes, returns, and sync.

## Follow-On Tuning: Default First-Wave `3`, Then Normal `2`

The next experiment was closer to the protocol shape we discussed:

- keep the normal vote target count at `2`
- widen only the first unresolved outbound send for a token or witness slot to `3`
- then compare:
  - normal tick pumping
  - resend every 2nd round via `vote_request_resend_cooldown = 2`

This keeps the “plant the seed wider” idea, but avoids the always-`3` fanout from the earlier sweep.

### Sparse Steady-State, No Conflict

Reference point:

- state-change push, `2` targets: `1202` committed, `298` pending, `8.99M` wire, avg latency `12.3`, p95 `13`, role-sum factor `22.14x`

Results:

| Case | Committed / Pending | Wire Messages | Avg Latency | p95 | Total vs Role Ideal |
| --- | ---: | ---: | ---: | ---: | ---: |
| first-wave `3`, then `2` | `1184 / 316` | `9.10M` | `9.2` | `11` | `5.90x` |
| first-wave `3`, then `2`, cooldown `2` | `1157 / 343` | `6.50M` | `11.3` | `14` | `8.91x` |

Reading:

- the first-wave widening is a very strong honest-load optimization:
  - much better latency
  - dramatically lower role-sum gap
  - only a small drop in committed count
- the every-2nd-round pump cuts wire traffic hard, but gives back too much latency and completion rate

### Sparse Steady-State, Conflicts

Reference point:

- state-change push, `2` targets: `355` families, `119` lower-owner-commit, `12` split, `68` highest-majority, `244` stalled-no-majority, `12.31M` wire, avg latency `9.9`, p95 `12`

Results:

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Highest Majority | Stalled No Majority | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| first-wave `3`, then `2` | `380` | `137` | `2` | `11` | `50` | `269` | `0.76` | `12.87M` | `10.2` | `11` |
| first-wave `3`, then `2`, cooldown `2` | `367` | `105` | `1` | `7` | `51` | `276` | `0.75` | `9.38M` | `10.7` | `14` |

Reading:

- widening only the first wave does **not** improve the conflict-convergence picture
- the honest-path win does not carry over here:
  - lower-owner commits got worse in the no-cooldown case
  - highest-majority got worse
  - stalled-no-majority got worse
- cooldown trims traffic, but still does not improve majority formation enough to justify becoming the new policy

### Integrated Churn Lifecycle

Reference points:

- no-conflict churn, state-change push with `2` targets: `1172` committed, `628` pending, `9.07M` wire, avg latency `21.2`, p95 `42`
- conflict churn, state-change push with `2` targets: `1263` committed, `957` pending, `15.85M` wire, avg latency `18.0`, p95 `37`

Results with first-wave `3`, then `2`:

| Case | Submitted | Committed | Pending | Wire Messages | Avg Latency | p95 | Recovery |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| no-conflict churn | `1800` | `1175` | `625` | `9.26M` | `22.0` | `49` | crash recovery `1` round, then `1` |
| conflict churn | `2248` | `1355` | `893` | `15.26M` | `18.9` | `38` | crash recovery `1` round, then `8` |

Conflict-family outcomes under churn:

- `448` families
- `230` highest-majority
- `182` stalled-no-majority
- `106` lower-owner-commit
- `45` multi-owner-commits
- signal coverage `0.67`

Reading:

- under no-conflict churn, first-wave `3` is basically flat to slightly worse:
  - commits nearly unchanged
  - latency worse
  - wire slightly higher
- under conflict churn, it is genuinely mixed:
  - more committed blocks and fewer pending
  - slightly lower wire cost
  - slightly better highest-majority / stalled counts
  - but worse lower-owner and multi-owner outcomes
  - recovery after the later crash wave got worse

## Current Assessment

The voter-window logic itself is aligned:

- if `neighborhood_width = 6`, the eligible voter set is `2*6+1 = 13`
- if `neighborhood_width = 4`, the eligible voter set is `2*4+1 = 9`

So the system is **not** silently dismissing in-range connected voters by using a larger or different eligibility window. The gap is between:

- who is eligible to count
- and how many of those eligible peers we proactively target on each outbound wave

The new first-wave default is therefore a real protocol choice, not a bug fix.

My read after this sweep is:

1. first-wave `3`, then `2` is a good honest-traffic optimization
2. it does **not** yet look like a clean default for conflict-heavy workloads
3. the every-2nd-round pump is still too blunt; the reply-push helps, but not enough to make that resend policy broadly safe yet

So the best current default is still arguable:

- if honest throughput/latency is the priority, first-wave `3` is attractive
- if conflict behavior is the priority, the older `2`-target baseline is still cleaner

That means the next improvement should probably be more selective:

- widen only the first wave for non-conflicted families
- keep conflict families on the more conservative path
