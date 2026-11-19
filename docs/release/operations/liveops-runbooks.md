# Operations runbook — canonical Rust service

Updated November 18, 2026. This is a procedure and review gate, not evidence of
a public deployment, tested production restore or authorization to restart the
playable preview. The [PR integration record](../../coordination/VIBE-MERGES-2026-11-18.md)
records the repaired Rust build gate. The baseline `0226dc9` persistence,
migration and deployment findings remain in the [Vibe review](../../coordination/VIBE-REVIEW-2026-11-18.md).
The working preview is a separate previously tested runtime; identify its exact
binary and data directory before any later operational action.

## 1. Inventory the actual writer and committed state

Record the process/executable path and SHA-256, source commit, listener,
working directory, absolute `CHAO_DATA_DIR`, nonsecret configuration and the
last successful save evidence. Do not print bearer credentials or copy live
profile contents into an issue/log. A Git merge does not change the running
binary, and a green source-only check does not validate that binary.

The current authority is one Rust process with in-memory state and
`CHAO_DATA_DIR/profiles.json`, a raw JSON array. Terrain, activity catalogs,
inventory metadata and local imports also resolve under that directory.
PostgreSQL/Redis configuration is not the canonical gameplay store. Keep one
writer; the in-process mutex is not a filesystem/distributed writer lock. The
Node reference must never write the same directory concurrently.

Normal persistence is [AppState::persist](../../../src/service.rs): export
profiles, write `profiles.json.tmp`, sync that file, rename it over
`profiles.json`, clear dirty state, then commit pending Tiny rewards. This
ordering does not establish every filesystem/power-loss guarantee, nor make
the separate release-migration rewrite safe. The current migration uses a
non-atomic canonical-file rewrite and remains blocked.

## 2. Quiesced canonical backup

Use this procedure only during a separately scheduled operation. It requires
a known writer/runtime; there is no reviewed automated canonical backup script.

1. Stop admission of new mutating activity using the actual hosting/launcher
   controls. Do not invent a feature flag or assume blocking an HTTP catalog
   route stops WebSocket actions. Preserve access needed to finish saving.
2. Obtain the supported explicit Save result for controlled sessions where
   applicable: `kind:"saved"` is meaningful only after successful persistence.
   A snapshot, a pending Tiny `saving` screen or a timed-out request is not a
   save acknowledgement. Record outstanding/pending operations before stopping.
3. Request clean shutdown through the existing controlling launcher
   (`CHAO_SHUTDOWN_STDIN=1` and its `shutdown` line), Ctrl+C, or the implemented
   Unix SIGTERM path. Wait for the exact process to exit and confirm no other
   writer targets the directory. Inspect final-save errors; process exit alone
   does not prove a successful final flush.
4. Copy the quiesced data into a new private backup location outside the active
   directory. Retain the complete canonical profile file and required
   terrain/activity/catalog inputs; record optional/imported inputs and any
   `.schema-version` marker so the tested environment can be reproduced.
   Original native data remains private/local. Preserve temporary files as
   diagnostic artifacts separately; never promote `profiles.json.tmp` simply
   because its timestamp is newer.
5. Parse the copied profile file as an array and inventory expected IDs/count,
   token hashes, owner role, rings/stats, cosmetics/gear, Tiny records/receipts
   and resident opt-in in a private comparison. Record file sizes/SHA-256s and
   source/runtime/configuration metadata without publishing secrets.
6. Verify copied hashes and perform the isolated restore drill below. Keep the
   original backup immutable and document precisely which committed checkpoint
   it represents. A copied last-good save does not include uncommitted memory.

The generated [backup.sh](../../../scripts/backup.sh) dumps PostgreSQL, which
does not contain this canonical Rust profile state. Its success is not a backup
of the Chao. Retention, off-machine copies and access controls require a real
configured/tested policy; none is established by a generated cron/checklist.

## 3. Restore drill and rollback compatibility

1. Use an immutable verified backup and copy it into a new isolated recovery
   directory. Keep the active directory and current backup untouched. Never
   test a restore/migration against the live owner save.
2. Select an exact known-compatible binary/configuration. Do not assume that
   an older image preserves newer data: [Player::profile](../../../src/game.rs)
   writes a known-field projection, so ignored unknown fields can disappear on
   the next save. The loader can also filter invalid profile entries; JSON
   parsing/startup alone is insufficient preservation evidence.
3. Start only the recovery copy on loopback with an isolated allocated port,
   explicit recovery `CHAO_DATA_DIR`, owner bootstrap disabled and no public
   traffic. Load through the real Rust loader and verify the expected canonical
   IDs, owner role, credential continuity, rings/stats, cosmetics/gear, Tiny
   records/receipt IDs and resident opt-in. Do not allow a failed resume to
   silently create a new Chao; use the real `resumeOnly:true` behavior.
4. Exercise an explicit save and clean shutdown on the recovery copy. Compare
   the resulting profile fields with the backup, including any newly introduced
   and unknown data that must survive. Missing/mutated required fields block
   compatibility. Confirm there is one restored canonical entity, not a
   replacement actor.
5. Check documented restart semantics: direct-control sessions return to safe
   spawn, Watch gets a fresh routine/session ID with no downtime catchup, and
   unfinished Tiny hidden boards do not resume. Durable Tiny credit and its
   receipt must remain together. See [PHONE.md](../../../PHONE.md) and
   [WATCH.md](../../../WATCH.md).
6. Record every failure, runtime/source hash and comparison result. Only a
   later reviewed operation may switch the active directory/runtime, with all
   writers stopped and the original state retained for recovery.

Restoring an older checkpoint rewinds progression and receipts after that
checkpoint. It is a data rollback with reconciliation requirements, not an
automatic duplication-safe retry. Do not ask players to re-run all activities
or claim that redeploying a previous image recovers lost progression. A binary
rollback must be compatible with the current save or use a separately reviewed
data-rollback decision. Unknown-field preservation is a migration requirement,
not an implemented universal guarantee.

Existing validation entry points are [protocol/save tests](../../../tests/protocol.rs),
[Tiny receipt tests](../../../tests/phone.rs), [resident tests](../../../tests/watch.rs),
[profile compatibility tests](../../../tests/game.rs) and
[launcher shutdown test](../../../tests/process.rs). These files do not by
themselves prove that an operator's backup/restore has run successfully.

## 4. Persistence incident and probe failures

On a save error, stop new mutating traffic through the actual available
controls and preserve the process/state for diagnosis where safe. A pending
Tiny result remains uncommitted and blocks replacement; a forced stop can lose
memory that has not reached disk. Inspect storage capacity/permissions and the
actual temporary-write/sync/rename failure. Preserve the last committed file
and diagnostics; do not edit it by hand or treat a stale backup as fresh state.

| Observation | What it proves / required response |
| --- | --- |
| `/api/status` responds | Service/status path responds; inspect reported terrain/assets and verify the intended binary/profile separately |
| Added `/livez` responds | Liveness only; a live process can fail all saves |
| Current `/readyz` responds | Only its `create_dir_all` check passed; an existing read-only/full directory can still fail actual persistence |
| Missing added probe on older runtime | Record the actual version/route contract; do not infer that a Git-only route was deployed |
| Save timeout/error or Tiny phase `saving` | No durable success established; do not confirm payout/save or return traffic merely because probes are green |
| Metrics show no failures | Current instrumentation misses some persistence/drop paths; inspect actual acknowledgements/logs and verified state |

Reproduce storage failure and recovery with disposable data. A useful readiness
repair must test the actual durable-write boundary and fail closed on inability
to commit. Do not create a canary player, change volume permissions or fill the
live disk as a test. Source/metrics fixes and their tests remain separate work.

## 5. Candidate validation and held rollout

Use the candidate checkout and disposable data; preserve the preview and its
save. Run the host and independent crates explicitly:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --locked
cargo test --manifest-path crates/tiny-garden-games/Cargo.toml --locked
npm test
```

At the recorded baseline these are required checks, not a statement that the
broken source passes them. Main-package tests do not automatically test the
standalone training core. Run formatting and strict Clippy for each independent
Tiny manifest as well; root `--all` does not include those separate workspaces.
Include real supported session/socket/save round
trips, deliberate missing-ack/failed-save/replay controls, copied-data restart
and the restore drill. Never default a mutating harness to the live listener.

Keep rollout held until build, canonical storage/restore, protocol migration,
one-writer hosting and measured capacity gates pass. Pairing/lease changes must
migrate desktop, Tiny and widget consumers together. Record the candidate and
previous runtime hashes and actual data-compatibility evidence.

The generated [release.sh](../../../scripts/release.sh) exits under `set -e`
when its deploy command fails before reaching its advertised rollback branch.
The current [migration](../../../scripts/migrate.sh), infrastructure and backup
procedures have separate review findings. Do not present them as a proven
deployment/automatic rollback path. This runbook executes none of them.

## 6. Catalog, simulation and activity changes

Shop prices/actions are in [game.rs](../../../src/game.rs); check
[game tests](../../../tests/game.rs) and the retained reference suite.
Prepare a bounded source change and verify purchase/proximity/ownership and
rollback compatibility on isolated data. Existing owned cosmetics/gear and
earned records must survive. There is no implemented general refund/admin
correction API to substitute for a tested migration.

The standalone [TrainingLedger](../../../crates/tiny-garden-core/README.md)
has bounded frozen policy and once-only transitions, but live durable
start/cancel/settle, app controls and ownership integration are not connected.
Watch is an idle/wander/rest routine with no passive rewards, not AFK training.
Five Tiny games use [PHONE.md](../../../PHONE.md) and the Rust game crate.

Race/Karate authority is described in [ACTIVITIES.md](../../../ACTIVITIES.md).
All 62 native race configurations stay unavailable until genuine routes,
gates/spawn and terrain are verified. Scheduled away-time AI competitions,
operator event enablement and event reward/reconciliation tools need their own
Rust implementation and proof. Do not announce or enable nonexistent controls.

## 7. Emergency controls, corrections and notifications

There is no established remote feature-flag service or implemented
`CHAO_DISABLE_*` switches. Use only identified existing edge/process controls;
closing a catalog endpoint does not stop the WS mutation path. Narrowing Origin
settings does not revoke bearer credentials, and disabling owner bootstrap
does not revoke an already persisted owner's role.

Moderation, per-account freezes, balance correction/refund tools, account
deletion and push campaigns remain planned/reviewed separately. Preserve
evidence and correct through a tested Rust authority operation when one exists;
do not patch live JSON, promise automatic compensation or manufacture a store
transaction/notification result. Source policy drafts are not implemented
operator capabilities.

Maintain an incident/change record with exact version, affected checkpoint,
observed failures, corrective evidence and outstanding risk. Communicate through
the project's actual authorized channels; this document provisions or sends
nothing. Follow the [coordination board](../../coordination/BOARD.md) and
[protocol gate](../../coordination/PROTOCOL.md) for implementation ownership.
