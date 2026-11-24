# Durable adapted Tiny Garden AFK training

November 18, 2026. The Rust host now connects the existing
`tiny-garden-core::training::TrainingLedger` to the canonical profile store.
This is an explicitly adapted MMO system, `adapted-afk-training-v1`, not a
claim that the original GBA Tiny Garden contains these timed training jobs.
The pure ledger remains the only training job engine.

## Wire contract

`GET /api/status` advertises `tinyGarden.training`:

```json
{
  "enabled": true,
  "rulesVersion": "adapted-afk-training-v1",
  "policy": {
    "tickMs": 60000,
    "pointsPerTick": 1,
    "maxDurationMs": 28800000,
    "maxPoints": 480,
    "costRings": 0
  },
  "stats": ["swim", "fly", "run", "power", "stamina"]
}
```

The initial policy is free, one canonical point per completed minute, up to
eight hours / 480 points per job. The policy is frozen when a job starts.
Durations are whole-minute multiples from 60000 through 28800000 ms. This
policy changes points only; it never raises levels or grades or changes hidden
stats. Points clamp to 9999. The admin Chao's stats are its
stored ones and take training points like any Chao's.

Use the existing authenticated `care` envelope, UUID and current care revision:

```json
{
  "type": "care",
  "requestId": "00000000-0000-4000-8000-000000000001",
  "expectedRevision": 1,
  "intent": { "kind": "training_start", "stat": "run", "durationMs": 60000 }
}
```

The other two strict intent shapes are:

```json
{ "kind": "training_cancel", "jobId": 1 }
{ "kind": "training_settle", "jobId": 1 }
```

No envelope or intent accepts client time, policy, target identity, award,
points, inventory or rings. A start returns an ordinary durable `care-result`
with `effects.training:{jobId,outcome:"started",costRings:0}`. It gives no
instant gain.

Closing returns `effects` containing:

```json
{
  "training": {
    "jobId": 1,
    "stat": "run",
    "outcome": "settled",
    "awardedPoints": 1,
    "appliedPoints": 1,
    "completedServerMs": 1800000060000,
    "receiptId": "00000000-0000-4000-8000-000000000002"
  },
  "points": { "run": 1 },
  "levelsChanged": false,
  "gradesChanged": false
}
```

An unfinished cancellation uses `outcome:"canceled"`, zero awarded/applied
points and the same receipt boundary. A ready job cannot be canceled; it must
settle. Early settlement is rejected. A long absence cannot exceed the saved
duration or its 480-point cap.

## Snapshot and routine behavior

The private self snapshot's `care.training` includes the capability above plus:

| Field | Meaning |
| --- | --- |
| `serverNowMs` | Trusted Unix wall time from the Rust host. |
| `job` | Existing core job view or null. |
| `lastResult` | Actual closing result for the currently stored closed job, or null. Earlier results remain in the activity history. |
| `eligibility` | `canStart`, `canCancel`, `canSettle`, and `reason`. |

The core job view has `jobId`, `stat`, `startedServerMs`, `deadlineServerMs`,
`phase`, `remainingMs`, and `projectedPoints`. Phases are `training`, `ready`,
`settled` or `canceled`. `projectedPoints` describes elapsed server time; it is
informational, never a client-submittable award. `eligibility.reason` explains
why starting is blocked, including while a job is active. Use each action's
explicit eligibility boolean when enabling its control.

Starting requires an authenticated controlling connection, explicit Tiny
residency, a safe hub/garden/Hero/Dark zone and no minigame, race, Karate,
vehicle or other activity lock. It reserves `care.activityLock` as
`training:afk:<jobId>`, clears movement input and pauses resident AI. Normal
game actions, Tiny games, shop, feeding, care and Watch changes cannot run
during the job. Cancellation or committed settlement clears only its own lock
and resumes the persisted Tiny routine automatically.

The background Rust service settles ready, safe jobs even when their owner is
offline. A pending/conflicting runtime activity postpones closure. Automatic
closure uses a server-generated UUID, the current care revision and the same
atomic transaction. After successful persistence it emits a private
`event` with `kind:"training-result"`, `revision`, the actual closing `result`
and `rulesVersion`. A failed write restores the job, stats, receipts, history,
dirty state and event queue; it emits no success result. A later attempt can
commit once.

Wall time comes from `SystemTime` independently of simulation tick skipping.
The host-only `care_action_at` / `settle_ready_training_at` functions allow
deterministic trusted test clocks; they have no HTTP or WebSocket clock-import
endpoint. Backward time produces no award. A restart preserves the frozen job
and deadline; the running service then safely closes it. This deadline-based
AFK award is distinct from the resident AI, which does not simulate invented
garden actions during server downtime.

Start reserves both a revision and receipt slot for its later closure. The
owned-egg integration additionally preserves the pending egg's final hatch
slot when starting and closing training; ordinary care cannot consume it.
The complete receipt ledger remains capped at 4096, with no receipt eviction. Exact
UUID/payload retries return the saved outcome before activity checks; changing
the payload under a UUID fails. The same mutex serializes multiple devices.
There is no separate training wallet, Chao record or profile writer.

## Actual activity history

The private `care.history` array contains at most 64 actual accepted care
transactions, oldest first:

```json
{
  "id": "00000000-0000-4000-8000-000000000002",
  "at": 1800000060000,
  "kind": "training_settle",
  "result": { "training": {}, "points": { "run": 1 }, "levelsChanged": false, "gradesChanged": false }
}
```

The example abbreviates `result.training`; real rows retain the full committed
effect shown above. `id` is the actual care request/receipt UUID. `at` is trusted
host Unix milliseconds. `kind` is `consume`, `purchase`, `tiny_purchase`,
`tiny_enter`, `pet`, `pickup`, `training_start`, `training_cancel` or
`training_settle`, plus `tiny_egg_purchase` and `tiny_egg_hatch` from the owned
collection integration. Buy/feed item rows additionally have `itemId`. `result` is exactly
that receipt's committed `effects`, including actual bounded deltas.

History appends in the same transaction as its receipt and canonical state.
Replays and rejected intents append nothing. Legacy profiles start with an
empty array; old receipt activity is never invented retroactively. Display
trimming does not remove once-only receipts. Restoring checks each row against
its real receipt and increasing care revisions. Other players' public entries
do not include this private history.

This slice does not persist minigame start/stop/result rows, garden AI routine
changes, or legacy Watch care as history entries. A UI may describe currently
observed routine changes as current-visit observations, clearly separated from
this durable log; it must not present boilerplate as saved past activity.

## Evidence and remaining boundaries

Executed in the isolated shop worktree against disposable canonical stores:

- `cargo test --locked`: 153 passed, 3 owned-native-source tests ignored.
- `cargo fmt --check` and `cargo clippy --locked --all-targets -- -D warnings`
  passed.
- Fourteen new host tests cover five-stat/duration bounds, forged fields,
  clock/readiness, replay/revisions, multi-device serialization, all activity
  conflicts, save rollback, offline/restart once-only settlement, applied caps,
  admin privileges, unknown metadata, corrupted stored state and bounded real
  history without fake legacy activity.
- Three added real WebSocket tests prove durable start/cancel and duplicate
  sockets, production-clock settlement/reconnect replay, private history,
  observer mutation rejection and the actual background service's post-commit
  automatic result event. The readiness fixtures use host clock injection;
  clients never supply a clock and no 60-second real wait is claimed.

No live save, running preview, release binary or credentials were replaced.
Source publication and these service tests are not an authorized runtime
rollout. Browser/Android training UI integration and its real browser/device
acceptance remain separate. The unmerged accounts/pairing/lease-v2 work is not
claimed here; passive observer sockets remain read-only.
