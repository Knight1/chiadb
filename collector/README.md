# Chia DB collector

`chia-db-collector` is the long-running source of the graph data. It records physical SQLite database size as soon as the file exists and begins coin-transition collection through `get_additions_and_removals` after the full node first contacts a full-node peer.

It writes two different kinds of state:

- `collector-state/collector.sqlite` is local, durable collector state. It holds the RPC cursor, canonical block hashes used for reorg detection, and per-block counts.
- `archive/metrics.ndjson` is the Git-friendly public source. It has one finalized UTC-day JSON record per line. The existing Node build converts this file into the CDB1 binary graph asset for Pages.

The collector stores the main SQLite file and the adjacent SQLite WAL file as separate exact byte counts. The graph can show the main file while still exposing the WAL measurement in the tooltip.

## Start locally with Docker Compose

From the repository root:

    docker compose up --build

The Compose configuration starts the official Chia image, the collector, and a small permissions helper. No ports are published to the host. The collector uses an internal network with Chia and has no Internet route by default; Chia also joins an outbound network for peer connections. RPC port 8555 stays inside the Compose networks.

The default collector image uses `scratch`: it contains the stripped, statically linked Rust binary and a CA certificate bundle. It has no shell, package manager, or Git executable. The collector runs as UID/GID `10001:10001`, drops all Linux capabilities, uses `no-new-privileges`, and has a read-only root filesystem. Chia data and the copied RPC credentials are mounted read-only. Only `collector-state/`, `archive/`, and a small temporary filesystem are writable; the repository and `.git` are not mounted.

Chia creates private TLS files owned by root. The permissions helper has no network, a read-only root filesystem, and only `CHOWN` and `FOWNER` capabilities. It prepares the state/archive directories, including existing root-owned collector SQLite files, and copies only the full-node RPC certificate and key into a separate volume. Those copies are readable by collector group `10001` with mode `0440`. The original Chia files are never changed. The helper marks the data directories ready before TLS exists, so size sampling can start immediately. It retries missing TLS files every 60 seconds. Restart the collector after rotating Chia's TLS credentials so its HTTP client reloads them.

This changes ownership only of the dedicated data directories and known collector files, not the entire checkout. If you inspect these directories directly on Linux, use UID/GID `10001` or administrator access. Existing collector state and the Chia volume are retained.

The Chia RPC uses mutual TLS. The default Compose setup deliberately enables `CHIA_RPC_INSECURE_SKIP_VERIFY=true` because the Chia container generates a self-signed server certificate and the RPC is private to the Compose network. The client certificate and key are still required. For a remote or shared network, mount a trusted CA at `CHIA_RPC_CA_CERT` and set `CHIA_RPC_INSECURE_SKIP_VERIFY=false`; do not expose port 8555.

On a first Compose start, the Chia container can take time to create its TLS files and start its RPC service. The collector stays running when those files are absent, waits for `POLL_SECONDS` (60 seconds by default), and retries. Once the files exist, it keeps polling while the node syncs.

Each RPC request has a 300-second timeout, independent of the 60-second polling interval. Increase it for a busy syncing node with `CHIA_RPC_TIMEOUT_SECONDS=600 docker compose up --build`, or set `CHIA_RPC_TIMEOUT_SECONDS` in `.env`. A timed-out batch keeps successfully collected blocks and retries missing responses from its saved cursor; gaps alone do not count as a reorg.

The collector records database and WAL size as soon as the SQLite file exists, including while the full node is syncing. Once that file exists and `get_connections` reports a full-node peer, it starts collecting block activity immediately, even during initial sync. It persists that first network-contact observation, so a later temporary peer disconnect does not stop collection from the node's local chain. It updates the current UTC-day size sample on each poll, then freezes that sample after UTC midnight. A missed day remains explicitly unobserved instead of being fabricated.

## Backfill coin activity from block 1

When the collector first sees an initially syncing node, it starts activity collection at block 0 automatically. If it first sees an already-synced node, the default starts at the current peak. To force a block-0 backfill in either case, start with an empty `collector-state/` directory and run:

    BACKFILL_START_HEIGHT=0 docker compose up --build

The environment variable is used only when the collector first creates its state database. After it reaches the tip, restart without `BACKFILL_START_HEIGHT`; it will continue from the persisted cursor. Do not run two collectors against the same `collector-state/` directory. The backfill uses `get_block_records` to obtain header hashes and calls `get_additions_and_removals` for every transaction block. `MAX_BLOCKS_PER_POLL` and `RPC_CONCURRENCY` control the load; the defaults intentionally favor the full node over speed.

While a historical backfill is incomplete, the collector keeps its intermediate data in local SQLite and does not rewrite or publish `archive/metrics.ndjson`. Once it reaches the tip, it writes the complete historical activity archive once. After that, it normally changes only after a UTC day is closed. A recent reorg can revise the small affected tail.

An existing `blockchain_v2_mainnet.sqlite` file can only show its size at the time it is observed. It cannot reconstruct its physical size on earlier dates. A block-1 backfill therefore provides complete additions/removals history, while exact database-size history begins with the first observed file sample. The CDB1 flags distinguish these facts.

## Build and view the live graph data

After the collector has produced at least two archive records:

    npm run build:data:live
    npm run dev

Open the local site with this query string:

    http://localhost:4173/?data=data/metrics.index.json

`metrics.cdb` and its index are deployment artifacts and remain ignored by Git. `archive/metrics.ndjson` is the source to track.

## Optional Git publishing

Publishing is disabled in the default Compose configuration and the minimal image does not include Git. To enable it, use the optional `runtime-git` target and publishing override with a dedicated clean checkout owned by UID/GID `10001:10001`:

    PUBLISH_WORKTREE=/absolute/path/to/dedicated-checkout \
      docker compose -f compose.yml -f compose.publish.yml up --build

The override mounts that checkout, enables publishing and outbound networking, and builds an Alpine runtime containing Git and an SSH client. The collector still runs without capabilities and with a read-only root filesystem. The helper only receives the checkout's `archive/` directory. Configure repository credentials separately, using read-only secret mounts and Git/SSH configuration; no token or deploy key is supplied by this project. SSH credentials can be mounted under `/run/secrets/` and selected with `GIT_SSH_COMMAND`, including an explicit known-hosts file. Do not disable host-key checking or mount your full home directory.

For a local binary with Git installed, the equivalent environment variables are:

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
    bash collector/tests/container-smoke.sh

The one-shot collector command records size when the database file exists. It needs generated TLS certificates, a local peak, and at least one observed full-node peer only when activity data is required; it does not wait for complete sync.

The container smoke test needs Docker Compose, Python 3, and OpenSSL on the host. It builds both runtime targets, exercises startup before TLS, checks state migration and file permissions, and verifies that a non-root collector can read the copied client identity. CI runs it before publishing Pages. All of its containers, volumes, and fixtures are temporary; it never connects to Chia or pushes Git data.

## Rust source layout

- `main.rs` parses configuration and configures logging; `runtime.rs` owns the retry loop.
- `collection/` coordinates each poll, fetches activity, reconciles reorgs, samples files, and finalizes the archive in separate modules.
- `rpc/` separates endpoints, TLS/HTTP transport, response types, and block-range validation.
- `store/` separates SQLite setup, metadata, block activity, snapshots, daily aggregation, and calendar gap handling.
- `tests/` holds the integration tests, separate reorg regression tests, and container smoke test.
