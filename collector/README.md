# Chia DB collector

`chia-db-collector` is the long-running source of the graph data. It waits for a synced full node, records the physical SQLite database size, and obtains daily coin-transition counts through the full-node `get_additions_and_removals` RPC.

It writes two different kinds of state:

- `collector-state/collector.sqlite` is local, durable collector state. It holds the RPC cursor, canonical block hashes used for reorg detection, and per-block counts.
- `archive/metrics.ndjson` is the Git-friendly public source. It has one finalized UTC-day JSON record per line. The existing Node build converts this file into the CDB1 binary graph asset for Pages.

The collector stores the main SQLite file and the adjacent SQLite WAL file as separate exact byte counts. The graph can show the main file while still exposing the WAL measurement in the tooltip.

## Start locally with Docker Compose

From the repository root:

    docker compose up --build

The compose configuration starts an official Chia full-node image and this collector. It exposes only Chia P2P port 8444. RPC port 8555 stays inside the Compose network. The collector sees the Chia data volume read-only, except for its own `collector-state/` and the checked-out repository used for the archive.

The Chia RPC uses mutual TLS. The default Compose setup deliberately enables `CHIA_RPC_INSECURE_SKIP_VERIFY=true` because the Chia container generates a self-signed server certificate and the RPC is private to the Compose network. The client certificate and key are still required. For a remote or shared network, mount a trusted CA at `CHIA_RPC_CA_CERT` and set `CHIA_RPC_INSECURE_SKIP_VERIFY=false`; do not expose port 8555.

The collector will not record a database-size sample until the node reports that it has synced. It updates the current UTC-day sample on each poll, then freezes that sample after UTC midnight. A missed day remains explicitly unobserved instead of being fabricated.

## Backfill coin activity from block 1

The default starts activity collection at the current peak. To backfill every canonical block from height 0, start with an empty `collector-state/` directory and run:

    BACKFILL_START_HEIGHT=0 docker compose up --build

The environment variable is used only when the collector first creates its state database. After it reaches the tip, restart without `BACKFILL_START_HEIGHT`; it will continue from the persisted cursor. Do not run two collectors against the same `collector-state/` directory. The backfill uses `get_block_records` to obtain header hashes and calls `get_additions_and_removals` for every transaction block. `MAX_BLOCKS_PER_POLL` and `RPC_CONCURRENCY` control the load; the defaults intentionally favor the full node over speed.

While a historical backfill is incomplete, the collector keeps its intermediate data in local SQLite and does not rewrite or publish `archive/metrics.ndjson`. Once it reaches the tip, it writes the complete historical activity archive once. After that, it normally changes only after a UTC day is closed. A recent reorg can revise the small affected tail.

An existing `blockchain_v2_mainnet.sqlite` file can only show its size at the time it is observed. It cannot reconstruct its physical size on earlier dates. A block-1 backfill therefore provides complete additions/removals history, while exact database-size history begins with the first observed synchronized day. The CDB1 flags distinguish these facts.

## Build and view the live graph data

After the collector has produced at least two archive records:

    npm run build:data:live
    npm run dev

Open the local site with this query string:

    http://localhost:4173/?data=data/metrics.index.json

`metrics.cdb` and its index are deployment artifacts and remain ignored by Git. `archive/metrics.ndjson` is the source to track.

## Optional Git publishing

Publishing is disabled by default. The collector does not receive a token or deploy key through this project. To enable it, give the collector a dedicated clean checkout with preconfigured Git credentials, then set these environment variables:

    GIT_PUBLISH=true
    GIT_WORKTREE=/output
    GIT_REMOTE=origin
    GIT_BRANCH=main
    GIT_AUTHOR_NAME=chia-db-collector
    GIT_AUTHOR_EMAIL=chia-db-collector@localhost

When enabled, it stages only `archive/metrics.ndjson`, commits only when that file changed, and pushes after the node is caught up. Keep the worktree dedicated to this service so an operator can review its credentials and repository state independently.

## Useful one-shot checks

    cargo test --manifest-path collector/Cargo.toml
    docker compose run --rm collector --once

The one-shot collector command still requires an already-synced Chia node and its generated certificates.
