# Fixed-Network Conflict Lineage Report

This rerun updates the fixed-network conflict study after correcting the family
measurement.

The important instrumentation change is:

- when checking how a conflict family is distributed across the network, a
  coverer now counts for candidate `B` if `node.committed_block(B).is_some()`
- this measures whether that node has actually committed that contender
- it does not collapse a committed winner to "not visible" just because that
  node later advanced to a descendant

That gives a much more faithful picture of conflict outcomes than the earlier
exact-current-head view.

## Scenario

Shared settings:

- `2000` peers
- `1000` transaction rounds
- fixed network, no churn, no elections
- dense linear topology:
  - `ring_linear_probability`
  - `center_prob = 1.0`
  - `far_prob = 0.2`
  - `guaranteed_neighbors = 10`
- network profile: `same_dc`
- vote targets: `2`
- immediate `InitialVote` targets: `6`
- no scheduled `InitialVote` retries
- commit-chain sync: off
- `30%` conflict family fraction
- `2` contenders per conflict family
- connected-only random entry peers
- pruning frozen for the whole run:
  - `EC_STEADY_STATE_PRUNE_PROTECTION_TIME=100000`

For this study I also forced single-token transactions:

- `EC_STEADY_STATE_BLOCK_SIZE_MIN=1`
- `EC_STEADY_STATE_BLOCK_SIZE_MAX=1`

That keeps lineage accounting crisp and avoids mixing multi-token effects into
the family totals.

## Sequence Types

The simulator tags each submitted block by lineage:

- `fresh non-conflicted`
  a one-token block creating a new token head
- `extends committed clean chain`
  a non-conflicting block whose parent was a normal committed block
- `extends committed conflict winner`
  a non-conflicting block whose parent was a committed winner from an earlier
  conflict family
- `conflict candidate on clean parent`
  a contender in a family whose shared parent was a normal committed block
- `conflict candidate on conflict winner`
  a contender in a family whose shared parent was itself a committed conflict
  winner

Conflict families are also split by lineage:

- `clean-parent conflicts`
- `post-conflict conflicts`

## Command

```bash
EC_STEADY_STATE_ROUNDS=1000 \
EC_STEADY_STATE_INITIAL_PEERS=2000 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=1 \
EC_STEADY_STATE_BLOCK_SIZE_MIN=1 \
EC_STEADY_STATE_BLOCK_SIZE_MAX=1 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=0.5 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.3 \
EC_STEADY_STATE_CONFLICT_CONTENDERS=2 \
EC_STEADY_STATE_NETWORK_PROFILE=same_dc \
EC_STEADY_STATE_TOPOLOGY=ring_linear_probability \
EC_STEADY_STATE_LINEAR_CENTER_PROB=1.0 \
EC_STEADY_STATE_LINEAR_FAR_PROB=0.2 \
EC_STEADY_STATE_LINEAR_GUARANTEED_NEIGHBORS=10 \
EC_STEADY_STATE_VOTE_TARGETS=2 \
EC_STEADY_STATE_FIRST_VOTE_TARGETS=6 \
EC_STEADY_STATE_COMMIT_CHAIN_SYNC=false \
EC_STEADY_STATE_PRUNE_PROTECTION_TIME=100000 \
cargo run --release --quiet --example integrated_steady_state
```

## Overall Result

- actual existing-token share: `64.5%` (`831 existing / 457 new`)
- conflict families created: `288`
- candidate blocks submitted: `576`
- total submitted blocks: `1288`
- committed blocks: `970`
- pending blocks: `318`
- overall commit latency: avg `4.3`, p50 `3`, p95 `5`
- settled peer spread: avg `37.1`, p50 `31`, p95 `78`
- block-related messages to settle: avg `634.7`, p50 `425`, p95 `1422`

The graph stayed fixed throughout:

- final avg known peers `1199.5`
- final avg connected peers `1199.5`

So these results are not confounded by late peer-graph collapse.

## Conflict Outcomes

These outcomes are now measured using committed candidate presence among
covering peers.

| Metric | Value |
| --- | ---: |
| families | `288` |
| no-committed | `12` |
| single-committed | `276` |
| split-committed | `0` |
| unanimous-highest | `205` |
| highest-majority | `268` |
| lower-majority | `0` |
| any-majority | `268` |
| stalled-no-majority | `20` |
| any-lower-committed | `0` |
| lower-owner-commits | `0` |
| multi-owner-commits | `0` |

This is the main corrected read:

- `268 / 288` families reached majority on the highest contender
- only `20 / 288` remained without a majority under the committed-candidate view
- there were `0` lower-majority families
- there were `0` lower-owner commits
- there were `0` multi-owner commits

So in this steady fixed-network workload, minority contenders are being
suppressed, and most conflict families do converge on the highest candidate.

## Winner Distribution On Coverers

Because we now count a coverer if it has committed the candidate block at all,
we can ask directly how broadly the winner reached the covering set.

| Metric | Value |
| --- | ---: |
| committed candidates among coverers/family avg | `0.96` |
| committed candidates among coverers/family p95 | `1` |
| coverers with any candidate committed/family avg | `11.7` |
| coverers with any candidate committed/family p50 | `13` |
| coverers with any candidate committed/family p95 | `13` |
| highest-candidate coverer share avg | `0.90` |
| highest-candidate coverer share p50 | `1.00` |
| highest-candidate coverer share p95 | `1.00` |

So the typical family now looks like:

- exactly one contender ends up committed among coverers
- that committed contender is the highest contender
- and it usually reaches nearly all coverers, often all `13`

The earlier exact-head metric was undercounting this because descendants of the
winner no longer showed the original winner as the current token head.

## Commit Spread By Transaction Type

| Type | Submitted | Committed | Pending | Avg Latency | p95 Latency | Avg Settled Spread | p95 Spread | Avg Block Msgs | p95 Block Msgs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fresh non-conflicted | `457` | `454` | `3` | `4.0` | `5` | `36.0` | `77` | `635.7` | `1319` |
| extends committed clean chain | `177` | `169` | `8` | `2.5` | `4` | `28.3` | `51` | `263.0` | `623` |
| extends committed conflict winner | `78` | `76` | `2` | `2.7` | `4` | `28.3` | `51` | `291.5` | `773` |
| conflict candidate on clean parent | `420` | `198` | `222` | `7.6` | `14` | `47.2` | `95` | `1061.2` | `2524` |
| conflict candidate on conflict winner | `156` | `73` | `83` | `3.7` | `6` | `46.1` | `81` | `690.1` | `1387` |

The non-conflict story is still strong:

- fresh blocks commit reliably
- clean chain extension stays fast and local
- extending a prior conflict winner is also fast

Conflict candidates remain the expensive class and the main source of pending
work, especially when the shared parent is a normal clean-chain block.

## Conflict Families By Lineage

| Family Type | Families | Owner Commits | No Committed | Single Committed | Highest Majority | Stalled | Lower Owner | Multi Owner |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| clean-parent conflicts | `210` | `198` | `8` | `202` | `195` | `15` | `0` | `0` |
| post-conflict conflicts | `78` | `73` | `4` | `74` | `73` | `5` | `0` | `0` |

Both lineage classes look healthy under the corrected metric:

- clean-parent conflicts: `195 / 210` highest-majority
- post-conflict conflicts: `73 / 78` highest-majority
- neither class produced lower-owner commits

## Message Distribution

Delivered logical messages:

- total `4.31M`
- `InitialVote` `838,382`
- `Vote` `1.58M`
- `QueryBlock` `1.18M`
- `Block` `284,846`
- `Referral` `432,554`

Vote ingress / repair:

- trusted votes recorded `1,270,906`
- untrusted votes received `0`
- block fetches triggered by votes `7,576`
- missing-parent fetches `431,128`

So the remaining repair pressure is still dominated by missing-parent repair,
not by lower contenders committing.

## Interpretation

### 1. The safety story is strong in this workload

- `0` lower-majority families
- `0` lower-owner commits
- `0` multi-owner commits

The current reactive path is suppressing minority contenders in this fixed
network scenario.

### 2. Most conflict families do converge

Under the committed-candidate view:

- `268 / 288` families reached majority on the highest contender
- only `20 / 288` remained stalled

That is a much more positive result than the earlier exact-head-based report.

### 3. The winner usually reaches almost the full covering set

- average highest-candidate coverer share is `0.90`
- p50 and p95 are both `1.00`
- average coverers with any committed candidate is `11.7 / 13`

So once a family resolves, the committed winner is usually present on nearly all
coverers.

### 4. The remaining problem is efficiency, not minority commits

The open work from this run is mostly:

- higher latency and message cost for clean-parent conflict candidates
- substantial `QueryBlock` traffic
- substantial missing-parent repair

That points much more toward propagation and repair efficiency than toward a
basic conflict-safety failure.
