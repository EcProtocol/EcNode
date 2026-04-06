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

## After Raising The Vote Threshold On Known Conflicts

I then tested the next idea:

- keep the single-signal + cooldown path
- but if a node knows a direct conflict exists for a token, raise the required token vote balance by `+1`

So:

- base `+1` becomes effective `+2` on the conflicted token
- base `+2` becomes effective `+3` on the conflicted token

This does **not** change witness thresholding. It only tightens the token-side settlement requirement when the node knows that token family is contested.

### Threshold-Bump Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Any Lower Visible | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, single signal + cooldown | `348` | `128` | `11` | `25` | `19` | `43` | `91` | `257` | `106` | `0.72` | `12.27M` | `10.5` | `13` |
| `+1`, cooldown + threshold bump | `383` | `147` | `11` | `7` | `27` | `45` | `109` | `274` | `105` | `0.72` | `11.86M` | `10.1` | `13` |
| `+2`, single signal + cooldown | `349` | `121` | `5` | `7` | `20` | `53` | `113` | `236` | `97` | `0.76` | `12.03M` | `11.2` | `14` |
| `+2`, cooldown + threshold bump | `341` | `117` | `5` | `9` | `13` | `53` | `100` | `241` | `80` | `0.76` | `11.73M` | `11.1` | `14` |

### Reading The Threshold-Bump Trial

This is not a strong win.

What it does seem to help:

- wire traffic drops a little:
  - `+1`: `12.27M -> 11.86M`
  - `+2`: `12.03M -> 11.73M`
- at `+2`, lower-candidate visibility improves a bit:
  - `any-lower-visible`: `97 -> 80`
  - `lower-owner-commit`: `121 -> 117`
- signal coverage is unchanged, so the warning behavior survives

What it does **not** clearly improve:

- `+1` still does not become safer:
  - lower-owner commits actually regress: `128 -> 147`
  - highest-majority is only marginally changed
- `+2` does not show a convincing settlement gain:
  - highest-majority stays flat: `53 -> 53`
  - any-majority gets worse: `113 -> 100`
  - stalled-no-majority gets slightly worse: `236 -> 241`
  - unanimous-highest drops: `20 -> 13`

So the bump is mostly acting like extra caution without a strong convergence benefit. It trims some activity, but it does not clearly move the system toward “majority forms on highest contender” often enough to stand out.

### Assessment Of This Idea

This experiment does **not** make `+1` look safe.

For the `+2 -> +3` path, the result is more neutral:

- it does no obvious harm to latency
- it slightly reduces lower-candidate persistence
- but it does not produce a cleaner majority outcome

So I would treat this as an interesting optional policy, not the next default direction. The more promising line still looks like:

- keep the single-signal + cooldown warning path
- improve how majority around the highest contender forms
- do that without simply demanding more votes and stalling longer

## After A Soft Family Vote Reset

I then tried a softer local reset mechanism:

- when a new highest direct sibling for a `(token, parent)` family is learned
- older recorded votes for that family are ignored by local tallying
- this does **not** clear blocks or wipe the whole token state; it only soft-expires older family votes

This version was kept purely as an experiment and later reverted.

### Soft-Reset Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Any Lower Visible | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, single signal + cooldown | `348` | `128` | `11` | `25` | `19` | `43` | `91` | `257` | `106` | `0.72` | `12.27M` | `10.5` | `13` |
| `+1`, soft family reset | `359` | `131` | `9` | `10` | `29` | `54` | `107` | `252` | `104` | `0.74` | `11.85M` | `10.6` | `13` |
| `+2`, single signal + cooldown | `349` | `121` | `5` | `7` | `20` | `53` | `113` | `236` | `97` | `0.76` | `12.03M` | `11.2` | `14` |
| `+2`, soft family reset | `383` | `128` | `7` | `13` | `23` | `57` | `104` | `279` | `96` | `0.73` | `12.47M` | `10.5` | `13` |

### Reading The Soft Reset

This one is mixed.

At `+1`, it helps the conflict-shape more than the baseline:

- fewer split families: `25 -> 10`
- more unanimous-highest: `19 -> 29`
- more highest-majority: `43 -> 54`
- slightly lower wire load

But it does **not** clearly improve the core safety metric:

- lower-owner commits got slightly worse: `128 -> 131`

At `+2`, it looks worse overall:

- lower-owner commits regressed: `121 -> 128`
- split outcomes worsened: `7 -> 13`
- stalled-no-majority worsened: `236 -> 279`

So the soft family reset is interesting as a local anti-stall idea, but not strong enough to keep in its current form.

## After Adding A Piggyback Highest-Contender Reply

I then tried the follow-up idea:

- keep the soft family reset above
- when replying to a vote request for a lower contender, also piggyback a vote for the highest known direct sibling
- the requested lower block still receives `vote = 0`
- the highest sibling travels as a second non-recursive vote reply

This was also reverted after the experiment.

### Piggyback Reply Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Any Lower Visible | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, soft family reset | `359` | `131` | `9` | `10` | `29` | `54` | `107` | `252` | `104` | `0.74` | `11.85M` | `10.6` | `13` |
| `+1`, soft reset + piggyback reply | `397` | `128` | `9` | `13` | `30` | `48` | `93` | `304` | `101` | `0.74` | `13.14M` | `9.5` | `12` |
| `+2`, soft family reset | `383` | `128` | `7` | `13` | `23` | `57` | `104` | `279` | `96` | `0.73` | `12.47M` | `10.5` | `13` |
| `+2`, soft reset + piggyback reply | `376` | `121` | `4` | `8` | `25` | `74` | `113` | `263` | `86` | `0.72` | `14.19M` | `11.5` | `14` |

### Reading The Piggyback Trial

This is more nuanced than the earlier rejected “replace the reply with the highest sibling” version.

What it improves:

- at `+2`, conflict outcomes get better across several measures:
  - lower-owner commits: `128 -> 121`
  - split: `13 -> 8`
  - highest-majority: `57 -> 74`
  - any-lower-visible: `96 -> 86`
- at `+1`, latency improves a little

What it costs:

- more wire traffic in both thresholds:
  - `+1`: `11.85M -> 13.14M`
  - `+2`: `12.47M -> 14.19M`
- more pending work / weaker throughput posture at the end of the run
- `+1` still does not look like a safer or cleaner settlement regime:
  - stalled-no-majority gets much worse: `252 -> 304`
  - highest-majority drops: `54 -> 48`

So this piggyback reply is not a bad idea, but it is not yet a clean enough win to justify keeping:

- `+1` remains unattractive
- `+2` gets a nicer conflict-majority picture, but pays for it with more traffic and a weaker steady-state throughput result

That makes it a plausible direction for future conflict-handling work, but not the next default behavior.

## After Sending A Transition-Time Conflict Update Batch

I then tried a narrower version of conflict propagation:

- keep the current single lower-sibling signal + cooldown path
- when a lower contender is newly moved to `Blocked` because a higher sibling is known
- send one explicit two-vote batch to the peers that had already voted on the blocked contender
- that batch contains:
  - `vote = 0` for the blocked contender
  - our current vote on the higher-ranked direct sibling

This is much narrower than the earlier piggyback-reply experiment:

- it only fires once per local transition to `Blocked`
- it only targets peers already registered as voters on that lower contender
- it does not depend on someone asking us again

### Transition-Time Batch Results

| Case | Families | Lower Owner Commit | Multi Owner Commits | Split | Unanimous Highest | Highest Majority | Any Majority | Stalled No Majority | Any Lower Visible | Signal Coverage | Wire Messages | Avg Latency | p95 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `+1`, single signal + cooldown | `348` | `128` | `11` | `25` | `19` | `43` | `91` | `257` | `106` | `0.72` | `12.27M` | `10.5` | `13` |
| `+1`, blocked-transition batch | `378` | `122` | `5` | `20` | `26` | `64` | `114` | `264` | `97` | `0.78` | `12.25M` | `9.5` | `12` |
| `+2`, single signal + cooldown | `349` | `121` | `5` | `7` | `20` | `53` | `113` | `236` | `97` | `0.76` | `12.03M` | `11.2` | `14` |
| `+2`, blocked-transition batch | `362` | `124` | `2` | `11` | `30` | `63` | `122` | `240` | `111` | `0.75` | `12.07M` | `10.9` | `13` |

### Reading The Transition-Time Batch

This is the first conflict-follow-up mechanism in this series that looks cheap enough to be genuinely interesting.

What stands out:

- wire traffic stayed essentially flat:
  - `+1`: `12.27M -> 12.25M`
  - `+2`: `12.03M -> 12.07M`
- queue pressure stayed in the same band or improved slightly
- latency improved in both thresholds:
  - `+1`: `10.5 -> 9.5`
  - `+2`: `11.2 -> 10.9`

At `+1`, the conflict picture improved across most useful metrics:

- lower-owner commits: `128 -> 122`
- multi-owner commits: `11 -> 5`
- split: `25 -> 20`
- unanimous-highest: `19 -> 26`
- highest-majority: `43 -> 64`
- any-lower-visible: `106 -> 97`
- signal coverage: `0.72 -> 0.78`

At `+2`, the picture is more mixed:

- better:
  - multi-owner commits: `5 -> 2`
  - unanimous-highest: `20 -> 30`
  - highest-majority: `53 -> 63`
  - any-majority: `113 -> 122`
  - latency improved
- worse:
  - lower-owner commits: `121 -> 124`
  - split: `7 -> 11`
  - any-lower-visible: `97 -> 111`

### Practical Reading

This mechanism is promising because it spends very little extra on the wire while still improving how fast prior voters hear “that contender lost; this is the higher one.”

It is **not** yet a complete win:

- `+1` still is not established as safe under adversarial conflict
- `+2` shows a genuine tradeoff rather than a clean improvement

But unlike the broader piggyback ideas, this one looks cheap enough that it may be worth keeping around while we continue testing conflict formation and majority behavior.

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
