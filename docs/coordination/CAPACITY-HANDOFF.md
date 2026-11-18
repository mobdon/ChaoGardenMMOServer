# Capacity/deployment integration handoff — November 18, 2026

Branch `codex/measured-capacity`, measured base `282f6ab`. Isolated worktree:
`C:/Users/Admin/Documents/ChatGPT/mmo/.local/worktrees/server-capacity`.
This is a source/test proposal, not authority to merge, migrate or deploy.
The separate live preview, canonical save and other worktrees remain untouched.

## Shared hooks requiring integration review

| Path | Proposed change / coordination boundary |
| --- | --- |
| `src/game.rs` | Same-room snapshots/effects, diagnostic room IDs and non-persisted resident timing. One existing canonical player per credential; no room/shard ownership transfer. |
| `src/service.rs` | Upstream compilation repairs, config/admission bounds, common metrics, send deadlines, explicit reliable-event lag disconnect, readiness write probe, and encode outside the game mutex. Save/watch/Tiny authority and transaction ordering remain in Rust. |
| `src/metrics.rs` | Bounded process histograms/counters without identity labels; final coherent scrape fix and concurrent regression test. |
| `src/main.rs` | Rustfmt of an existing print statement only. The unsafe old normalization migration is not repaired, invoked or accepted here. |
| `tests/game.rs`, `tests/protocol.rs`, `tests/watch.rs` | Narrow assertions updated for same-room visibility and always-present self state. New interest/runtime tests cover boundaries. |
| `tools/`, `examples/`, `measurements/`, `deploy/`, capacity docs | Synthetic real-protocol load/negative controls, measured failures, and private review artifacts. No replacement JS/TS game authority. |

Preserve the separate `chao-locomotion.glb` allowlist/MIME/route patch when
combining `service.rs`/protocol tests. This branch does not import those live
uncommitted edits. The accounts branch's central revocation/lease guard must
remain before **all** input/action/Save/Watch mutation paths. New transport
bounds apply around its agreed authentication deadline/first-frame flow; do
not replace that contract with this v1 fixture or token query in a public rollout.

Keep the accounts transaction's canonical Chao, role, credentials, unknown
profile fields, revision/lease metadata, resident opt-in, gear and Tiny receipts.
The current load fixture is a raw-array v1 profile store accepted by measured
Rust source; adapting it to a later agreed migration requires fresh integration
tests. No database, profile migration or separate NestJS/Prisma service is used.

## Evidence and unresolved gates

[Capacity report](../CAPACITY.md) and [operations](../CAPACITY-OPERATIONS.md)
contain exact workload/hardware, raw artifacts, failure modes and commands.
The 100-client 30-second timing slice passed, but the 120-second repeat exceeded
the 250 ms lag budget (603 ms p95). Extra residents and 250/500 attempts failed.
Keep LOAD-100/250/500 open; configured caps are not supported-population claims.
The failed 500-case room transitions and rate-limit disconnects are preserved.

The final candidate fixes a concurrent histogram scrape race discovered while
adding strict fail-closed gates. The matrix binary/hash is preserved separately
from that candidate's small gated smoke; no repeated full-capacity claim.
Public/TLS/physical Android acceptance, account/device integration, transactional
store migration and representative activity/long-soak/restart load remain open.
Run exactly one writer per JSON directory. No shards or service purchases.
