# Fixed-Network Steady Extension Report

This note measures a long fixed-network no-conflict workload where transactions
continuously extend previously committed token chains.

The intent is to isolate ordinary chained updates without the conflict-family
path and without the late graph-collapse seen in earlier runs.

## Scenario

Shared settings:

- `2000` peers
- `1000` transaction rounds
- fixed dense-linear topology:
  - `ring_linear_probability`
  - `center_prob = 1.0`
  - `far_prob = 0.2`
  - `guaranteed_neighbors = 10`
- network profile: `same_dc`
- vote targets: `2`
- immediate `InitialVote` targets: `6`
- no scheduled `InitialVote` retries
- commit-chain sync: off
- no conflicts
- connected-only random entry peers

To keep the network truly steady during the full window:

- `EC_STEADY_STATE_PRUNE_PROTECTION_TIME=100000`

To force multi-token chained transactions:

- `EC_STEADY_STATE_BLOCK_SIZE_MIN=3`
- `EC_STEADY_STATE_BLOCK_SIZE_MAX=3`
- `EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=1.0`

## Command

```bash
EC_STEADY_STATE_ROUNDS=1000 \
EC_STEADY_STATE_INITIAL_PEERS=2000 \
EC_STEADY_STATE_BLOCKS_PER_ROUND=1 \
EC_STEADY_STATE_BLOCK_SIZE_MIN=3 \
EC_STEADY_STATE_BLOCK_SIZE_MAX=3 \
EC_STEADY_STATE_EXISTING_TOKEN_FRACTION=1.0 \
EC_STEADY_STATE_CONFLICT_FAMILY_FRACTION=0.0 \
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

## Result

Overall:

- submitted `1000`
- committed `857`
- pending `143`
- commit latency avg `2.7`, p50 `2`, p95 `5`
- settled peer spread avg `58.9`, p50 `47`, p95 `133`
- block-related messages to settle avg `1172.0`, p50 `452`, p95 `4441`

Important steady-network confirmation:

- final avg known peers `1199.5`
- final avg connected peers `1199.5`

So this run did **not** suffer the late peer-graph collapse from the earlier
conflict experiments.

Workload realized:

- actual existing-token parts `73.2%` (`2196 existing / 804 new`)
- blocks touching existing state: `896`

Even with `existing_token_fraction = 1.0`, the run still creates some new-token
parts early because there are not yet enough committed mappings to extend at
every entry peer.

## Commit Spread By Type

| Type | Submitted | Committed | Pending | Avg Latency | p95 Latency | Avg Settled Spread | p95 Spread | Avg Block Msgs | p95 Block Msgs |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| fresh non-conflicted | `104` | `104` | `0` | `4.6` | `5` | `107.8` | `155` | `3895.8` | `5664` |
| extends committed clean chain | `896` | `753` | `143` | `2.5` | `4` | `52.2` | `117` | `795.8` | `3282` |

## Message Distribution

Delivered logical messages:

- total `63.32M`
- `InitialVote` `4.13M`
- `Vote` `26.83M`
- `QueryBlock` `20.60M`
- `Block` `3.24M`
- `Referral` `8.53M`

Vote ingress / repair:

- trusted votes recorded `25.97M`
- untrusted votes received `0`
- block fetches triggered by votes `57,840`
- missing-parent fetches `6.19M`

## Reading The Result

### 1. Continuous clean extensions are fast when the graph is kept steady

The dominant class in the run was `extends committed clean chain`, and it
behaved well:

- `753 / 896` committed
- avg latency `2.5`
- p95 latency `4`

That is much healthier than the earlier conflict-heavy late-run behavior.

### 2. Clean extensions are cheaper than fresh creation here

Compared with fresh transactions:

- lower latency
- much smaller spread
- much lower block-message cost

So on this steady dense graph, once a token chain exists, continuing it is the
cheap path.

### 3. The expensive part is still block repair

Even without conflicts:

- `QueryBlock` is `20.60M`
- vote-triggered block fetches are `57,840`
- missing-parent fetches are `6.19M`

So the ordinary chained-update path is live and fast, but still block-repair
heavy at this scale.
