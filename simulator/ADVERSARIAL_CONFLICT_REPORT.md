# Adversarial Conflict Report

This report evaluates a first adversarial steady-state workload on the sparse fixed graph.

The aim is to test whether the current conflict-handling path can keep competing updates on the same token from producing divergent or minority outcomes, and to see how much of the `+1` threshold gain survives once conflicts are present.

## Scenario

Common settings:

- `192` peers
- sparse fixed ring topology, `±8` neighbors
- `500` rounds
- network profile: `cross_dc_normal`
- neighborhood width: `6`
- vote targets: `2`
- request batching: `phase1` (`batch_vote_replies = false`)
- existing-token workload target: `50%`

Adversarial workload:

- `25%` of transaction slots are replaced by conflict families
- each family injects `2` competing blocks
- both blocks update the same `(token, parent_block)` pair
- contenders are submitted from distinct entry peers

This means the conflict runs submit more blocks than the honest controls.

## Commands

Threshold `+1`, honest control:

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
```

Threshold `+1`, adversarial conflicts:

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
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

Threshold `+2`, honest control:

```bash
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

Threshold `+2`, adversarial conflicts:

```bash
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
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
cargo run --release --quiet --example integrated_steady_state
```

## Summary

| Case | Submitted | Committed | Pending | Wire Messages | Avg Latency | p95 Latency |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, honest | `1500` | `1280` | `220` | `9.10M` | `9.6` | `13` |
| `+1`, conflicts | `1883` | `1202` | `681` | `12.68M` | `10.4` | `12` |
| `+2`, honest | `1500` | `1191` | `309` | `9.22M` | `12.0` | `15` |
| `+2`, conflicts | `1857` | `1162` | `695` | `12.50M` | `10.3` | `14` |

## Conflict Outcomes

Conflict-family metrics focus on the covering peers for the conflicted token at the end of the run.

Definitions:

- `no-visible`: none of the covering peers ended on a candidate block from the family
- `single-visible`: exactly one candidate is visible among covering peers
- `split`: more than one candidate is visible among covering peers
- `unanimous-highest`: all covering peers ended on the highest candidate block ID
- `highest-majority`: more than half of the covering peers ended on the highest candidate
- `any-majority`: more than half of the covering peers ended on some single candidate
- `stalled-no-majority`: no single candidate reached a majority among covering peers by the end of the run
- `any-lower-visible`: at least one lower candidate remained visible among covering peers
- `lower-owner-commit`: at least one lower candidate committed at its submitting owner
- `multi-owner-commits`: more than one contender committed at submitting owners
- `participant peers`: peers that knew at least one candidate from the family during the run-end snapshot
- `signaled participants`: those participant peers that also received at least one explicit `vote = 0` conflict signal for a lower sibling
- `signal coverage among participants`: share of participant peers that received such a conflict signal

| Case | Families | No Visible | Single Visible | Split | Unanimous Highest | Any Lower Visible | Lower Owner Commit | Multi Owner Commits |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, conflicts | `383` | `186` | `176` | `21` | `26` | `102` | `147` | `21` |
| `+2`, conflicts | `357` | `164` | `177` | `16` | `15` | `110` | `140` | `23` |

Useful percentages:

- `+1`, conflicts:
  - `48.6%` no-visible
  - `5.5%` split
  - `6.8%` unanimous-highest
  - `26.6%` any-lower-visible
  - `38.4%` lower-owner-commit
- `+2`, conflicts:
  - `45.9%` no-visible
  - `4.5%` split
  - `4.2%` unanimous-highest
  - `30.8%` any-lower-visible
  - `39.2%` lower-owner-commit

## What This Says

### 1. The threshold is not the main conflict problem

`+1` still improves the honest steady-state path:

- more commits: `1280` vs `1191`
- lower latency: `9.6` vs `12.0`
- fewer pending blocks: `220` vs `309`

But once conflicting families are present, `+1` and `+2` look much closer:

- committed blocks: `1202` vs `1162`
- average latency: `10.4` vs `10.3`
- pending blocks: `681` vs `695`

So the adversarial workload largely cancels the clean steady-state advantage of `+1`.

### 2. Both thresholds still allow too many lower-candidate outcomes

This is the more important result.

Both runs show:

- a high count of lower candidates committing at their submitting owners
- too few families converging unanimously on the highest candidate
- a non-trivial number of final split outcomes

That points at conflict propagation and suppression as the main weakness, not threshold alone.

### 3. Many conflict families do not settle cleanly within the run

In both thresholds, close to half of the families ended with no candidate visible at the covering peers.

That means the current system often neither converges cleanly nor cleanly rejects the competing updates inside `500` rounds. It tends to accumulate unresolved conflict work in mempool state instead.

## After Adding A Persistent Conflict Gate

I then added a local rule in the mempool:

- before commit decisions, if a pending block shares `(token, parent)` with a known higher block ID, it is moved to `Blocked`

This does not yet add explicit blocker propagation. It only prevents local commit once the higher sibling is known.

### Post-Gate Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, conflicts, before gate | `383` | `147` | `21` | `21` | `26` | `12.68M` | `10.4` | `12` |
| `+1`, conflicts, after gate | `354` | `124` | `11` | `18` | `33` | `7.58M` | `10.1` | `12` |
| `+2`, conflicts, before gate | `357` | `140` | `23` | `16` | `15` | `12.50M` | `10.3` | `14` |
| `+2`, conflicts, after gate | `372` | `133` | `3` | `11` | `22` | `7.93M` | `10.6` | `13` |

### Updated Reading

The gate helps, but only part of the way:

- lower-candidate owner commits dropped in both thresholds
- multi-owner conflicting commits dropped sharply
- unanimous-highest outcomes improved
- wire traffic almost halved because blocked lower siblings stop generating repeated work

But the core safety issue is still not fully solved:

- lower candidates are still visible too often
- too many families still fail to converge cleanly
- throughput did not improve with the gate alone, especially at `+2`

That matches the protocol intuition: local commit blocking is necessary, but not sufficient. We still need active propagation of higher-conflict knowledge so other peers learn to suppress the lower sibling sooner instead of merely refusing to commit it locally once they happen to discover it.

## After Forced Blocker Propagation

I then tested the next step:

- when sending the highest known candidate for a token conflict, also send `0`-vote requests for known blocked direct siblings in the same `(token, parent)` family

This uses the current batch path, so the extra conflict votes can still travel in request batches.

### Post-Propagation Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, conflicts, post-gate | `354` | `124` | `11` | `18` | `33` | `7.58M` | `10.1` | `12` |
| `+1`, conflicts, forced blockers | `393` | `146` | `9` | `14` | `36` | `29.28M` | `9.6` | `13` |
| `+2`, conflicts, post-gate | `372` | `133` | `3` | `11` | `22` | `7.93M` | `10.6` | `13` |
| `+2`, conflicts, forced blockers | `348` | `107` | `11` | `17` | `28` | `33.27M` | `11.0` | `13` |

### Reading The Forced-Propagation Trial

This result is mixed.

What improved:

- `+2` saw a meaningful drop in lower-owner commits: `133 -> 107`
- both thresholds improved unanimous-highest outcomes a bit
- `+1` and `+2` both reduced final split outcomes relative to the original ungated baseline

What got worse:

- wire traffic exploded: about `4x` over the post-gate runs
- peak in-flight queues exploded for both thresholds
- `+1` lower-owner commits got worse again: `124 -> 146`
- `+2` multi-owner commits regressed: `3 -> 11`

So forced propagation is directionally useful as a way to spread conflict knowledge, but the naive “send all blocked direct siblings every time” policy is too blunt. It buys some convergence, but at too high a traffic cost and with unstable effects across thresholds.

## After Single Conflict Signal Propagation

I then narrowed the propagation rule to the minimum useful signal:

- when sending the highest known contender for a direct `(token, parent)` conflict family
- include one lower sibling with `vote = 0`
- do not send all blocked siblings

This is closer to the protocol intent: spread enough conflict knowledge that peers can warn clients and prefer majority/election-based reads, without turning every conflict family into a flood.

### Single-Signal Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Signaled Participants / Family | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, conflicts, post-gate | `354` | `124` | `11` | `18` | `33` | `50` | `94` | `260` | `0.0` | `0.00` | `7.58M` | `10.1` | `12` |
| `+1`, conflicts, single signal | `393` | `142` | `15` | `21` | `23` | `55` | `109` | `284` | `129.5` | `0.74` | `21.81M` | `10.3` | `13` |
| `+2`, conflicts, post-gate | `372` | `133` | `3` | `11` | `22` | `51` | `97` | `275` | `0.0` | `0.00` | `7.93M` | `10.6` | `13` |
| `+2`, conflicts, single signal | `368` | `114` | `2` | `6` | `22` | `63` | `114` | `254` | `132.1` | `0.74` | `23.32M` | `11.2` | `13` |

### Reading The Single-Signal Trial

This version is materially better than “send all blockers”, but it is not a universal win.

What it clearly does achieve:

- it spreads conflict knowledge widely among peers that touched a family:
  - both thresholds reached about `74%` average signal coverage among participant peers
- it supports the client-warning model much better than the post-gate baseline:
  - a large share of family-aware peers ended the run with explicit evidence that a conflict existed
- at `+2`, it improves conflict hygiene:
  - lower-owner commits dropped: `133 -> 114`
  - multi-owner commits stayed very low: `3 -> 2`
  - split outcomes dropped: `11 -> 6`
  - highest-majority and any-majority outcomes improved

Where it is still weak:

- it is still much more expensive than the post-gate baseline:
  - `7.93M -> 23.32M` wire messages at `+2`
  - `7.58M -> 21.81M` wire messages at `+1`
- at `+1`, the safety picture is mixed:
  - lower-owner commits regressed: `124 -> 142`
  - multi-owner commits regressed: `11 -> 15`
- stalled families still dominate the end state:
  - `284/393` at `+1`
  - `254/368` at `+2`

So this is a more reasonable version of conflict signaling than the naive all-blockers approach, and it aligns better with the operational idea that a peer can warn users when it sees a conflict. But it still does not produce the desired outcome on its own: a clear majority on the highest contender often enough, with stalled settlement as the next-best fallback.

### Practical Reading

At this point the design looks closer to:

- best case: majority forms on the highest contender
- next best: no stable majority forms, but many participants know there is a conflict and can warn clients
- still too common today: lower contenders can remain live for too long, and some commit at owners before the neighborhood converges

That means the current signal is useful as a warning mechanism, but not yet strong enough as a settlement mechanism.

## After Adding Resend Discipline To The Single Signal

I then kept the single-signal model, but stopped sending the same lower-sibling warning to the same peer every tick.

Current rule:

- still only one lower-sibling `vote = 0` signal per direct family
- but only resent to the same `(peer, lower_block)` after a cooldown window

This aims directly at the tradeoff we care about:

- keep conflict visibility high enough that participants can warn users
- cut the repeated traffic cost of “I already told this peer”

### Single-Signal + Cooldown Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Signaled Participants / Family | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, conflicts, single signal | `393` | `142` | `15` | `21` | `23` | `55` | `109` | `284` | `129.5` | `0.74` | `21.81M` | `10.3` | `13` |
| `+1`, conflicts, single signal + cooldown | `348` | `128` | `11` | `25` | `19` | `43` | `91` | `257` | `126.9` | `0.72` | `12.27M` | `10.5` | `13` |
| `+2`, conflicts, single signal | `368` | `114` | `2` | `6` | `22` | `63` | `114` | `254` | `132.1` | `0.74` | `23.32M` | `11.2` | `13` |
| `+2`, conflicts, single signal + cooldown | `349` | `121` | `5` | `7` | `20` | `53` | `113` | `236` | `131.2` | `0.76` | `12.03M` | `11.2` | `14` |

### Reading The Cooldown Version

This is the first version that looks practically usable as a warning mechanism.

What improved:

- wire traffic dropped sharply versus uncapped single-signal:
  - `+1`: `21.81M -> 12.27M`
  - `+2`: `23.32M -> 12.03M`
- peak queue pressure also dropped a lot:
  - `+1`: from about `169k` to `99k`
  - `+2`: from about `177k` to `99k`
- signal coverage among participant peers stayed high:
  - `0.72` at `+1`
  - `0.76` at `+2`

So the cooldown does preserve the main operational benefit: many peers that touched a conflict family still receive explicit evidence that a conflict exists.

What did not fully hold:

- the strongest `+2` safety outcomes of the uncapped single-signal version softened a bit:
  - lower-owner commits regressed slightly: `114 -> 121`
  - highest-majority families dropped: `63 -> 53`
- `+1` is still not persuasive on safety:
  - lower-owner commits remain high
  - split outcomes remain too frequent
  - majority on the highest contender is still too weak

### Practical Reading

The cooldown version looks like the best current balance if the goal is:

- spread conflict knowledge widely enough that clients/nodes can warn
- avoid the traffic explosion of the naive or uncapped versions
- keep `+2` as the more defensible settlement threshold

It does **not** yet fully solve settlement convergence. But it does move the mechanism closer to the protocol story you outlined:

- if conflict is present, many participant peers know that fact
- they can prompt a user or higher-level client to query multiple peers and elect the majority outcome
- if the system does not form a clear majority, the next acceptable result is stalled settlement rather than blind trust in a single peer

## Assessment

These adversarial results still do **not** support adopting `+1` as “safe enough” yet.

At the same time, they still do **not** support the simpler conclusion that “`+1` alone is the problem.” The threshold change is not what mainly breaks conflict handling here. The more serious issue is that the highest-ID conflict rule is not being propagated strongly enough for the neighborhood to converge quickly and consistently.

## Next Steps

The best follow-on experiments are:

1. Strengthen conflict propagation:
   - keep the single-signal idea rather than the all-blockers version
   - add resend discipline so the same signal is not sent every tick to the same peer
   - improve majority formation around the highest contender without flooding the graph
   - keep the blocker idea, but bound it more carefully than “all known blocked siblings every time”
2. Re-run the same adversarial matrix with `3` vote targets on the sparse graph
3. Add a round-by-round conflict-state tracker so temporary split commits can be measured, not only final state
