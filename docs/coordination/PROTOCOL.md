# Accounts, pairing and activity ownership integration

Updated November 18, 2026. Integration constraints for the isolated accounts branch `codex/accounts-pairing-ownership`, based on server commit `a8188a3030eb176ec4c3b7156f93410da5bb6f09`. This is a proposed v2 contract and review gate, not an already shipped authentication protocol. Keep the accounts PR unmerged until matching client migrations and cross-feature tests pass.

## Shared-file ownership

Accounts owns new identity/pairing/lease modules and its isolated `game.rs`/`service.rs` hooks. AFK training and competitions should supply new modules plus small integration patches against those agreed hooks. Integration owns the final combination and live rollout. Do not edit another chat's checkout, live saves or preview.

The main server checkout currently also has an independent static-asset patch: exact `chao-locomotion.glb` allowlist/MIME/route tests in `src/service.rs`, `index.js`, `tests/protocol.rs` and `tests/reference/assets.test.js`. Retain that patch when rebasing or merging accounts; it makes no gameplay changes. The release containing it has SHA-256 `51A07F9C382ECB70785654297180349DD2E34F677D5921818EA2B9A6ECEC3100`.

## Required boundaries

1. **One canonical Chao.** Preserve profile ID, owner role, stats, gear, rings, records and resident opt-in. Existing `resumeOnly` saves must never silently create replacement characters when credentials expire or migrate. Account/device metadata must not leak into public player snapshots.
2. **One durable transaction.** Current `AppState::persist` exports all profiles, fsyncs a temporary file, atomically replaces profiles.json, then commits pending Tiny rewards. Account ownership, revocations, revisions, leases and future training receipts must join the authoritative persisted document/transaction. Account-only changes must trigger persistence despite the current dirty-flag early return. `Player::profile` is a whitelist: unmodeled identity fields would otherwise disappear. Preserve unknown fields and support the existing raw profile-array format through an explicit versioned migration with backup and failure tests. The Node reference must round-trip the new schema safely or fail closed instead of silently overwriting it.
3. **Preserve result durability.** `phone::project_profile` projects the pending ring credit and receipt into the same exported profile; `commit_rewards` updates the live wallet and emits `tiny-game-result` only after the file replacement succeeds. Any save may commit pending rewards across profiles. Pending `rings_after` is precomputed: a handoff must settle or block conflicting wallet mutations, never overwrite it from an earlier clone. Watch start/stop/care already has rollback on save failure. New identity/lease mutations need equivalent commit/rollback guarantees. A snapshot or accepted request is not a durable-save acknowledgment.
4. **Browser authentication.** Coordinate a versioned first-auth-frame browser WS path with all three current clients (`src/main.js`, `src/tiny.js`, `src/widget.js`). Before authentication, send no profile snapshot/events and apply a bounded auth deadline/frame size. Preserve Origin checks, rate bounds, heartbeat/backpressure and explicit auth failure. The current clients use `/ws?token=`; retire that route through an explicit tested migration rather than breaking saved clients silently. HTTP clients can use the agreed authenticated header scheme. No permanent bearer token in URLs or QR payloads.
5. **Observe and control separately.** A widget or stats viewer may subscribe without acquiring/stealing the interactive activity lease. Identify every socket by validated device and profile; do not rely on a global connection count to decide which device controls gameplay. A passive widget must not keep an abandoned interactive Tiny round alive. Require authority for care commands too; observation alone does not permit mutation.
6. **Central dispatch guard.** Apply ownership/revocation/lease checks under the same authoritative mutex before every mutation path: ordinary input, tiny-input, actions, explicit Save, Watch start/stop/care and future training/competition routes. Current `service.rs` has separate save and watch dispatch paths outside `Game::action`; guarding only that method is insufficient. Revocation/takeover must invalidate already connected sockets, not only future connections. Credential/lease expiry uses trusted real time, not a skipped fixed-step simulation clock.
7. **Separate lease epoch from durable revision.** Use an activity lease ID/epoch plus a bounded per-device input sequence for interactive streams. Use expected durable profile revision and an operation ID for transactional mutations, with an explicit conflict/retry result. Do not advance the durable revision on every simulation tick and then require each movement frame to match it: normal latency would reject legitimate input. Define exactly which events advance each value.
8. **Keep transport metadata outside strict intents.** Current Watch validates exact top-level keys; the Tiny game input enum rejects surplus fields. A proposed v2 envelope should carry lease/revision/operation metadata outside the existing gameplay payload. Validate it centrally and pass only the original bounded payload to existing handlers. Preserve Tiny round IDs and Watch session IDs: the activity lease supplements those checks.
9. **Resident and AFK lifecycles.** Disconnecting/revoking a controlling device stops its input without deleting the canonical Chao, resident opt-in or already committed receipts. A resident continues its server-owned routine while opted in, independent of an owner device's interactive lease. Training start must acquire the agreed activity authority; server settlement and recovery must remain possible after device disconnect/revocation without duplicate awards. Keep away-time AI race/Karate entry as its own explicit opt-in system.
10. **Atomic rollout.** Keep accounts unmerged until client session/slot migration, WS authentication, observation, explicit lease takeover/release and error handling are tested together. Publish request/response/error schemas before UI integration. Never deploy or migrate the live owner profile as part of tests; use copied/synthetic data and versioned runtime copies.

## Suggested v2 message shape

This shape is a proposal for agreement before implementation, not a mandate to rename existing fields without coordinated changes:

```json
{
  "protocolVersion": 2,
  "leaseId": "server-issued-activity-lease",
  "inputSeq": 42,
  "expectedRevision": 7,
  "operationId": "client-retry-id-for-durable-command",
  "payload": { "type": "action", "action": "watch-stop", "sessionId": "current-watch-id" }
}
```

Input frames need the lease/sequence fields; durable commands need revision/operation fields as defined in the published schemas. Avoid accepting every optional combination indiscriminately. Idempotent retries return the original committed result. Conflict responses should include enough current self-state to recover, without exposing credentials or other profiles' private identity data.

## Acceptance tests

- Legacy raw-array migration preserves all known and unknown profile fields, owner role, every Tiny receipt and resident identity; failed migration leaves the original file recoverable.
- Desktop controls while Android/widget observes; observer cannot move, care, spend, claim a reward or take over implicitly.
- Explicit takeover/revocation rejects the old socket's next input/care/save/claim without duplicate actors; the new controlling device receives the current canonical state.
- Auth timeout, malformed first frame, wrong Origin, expired/replayed pairing and stale revisions/leases fail without disclosing profile data.
- Complete Tiny result during lease release/disconnect/save failure/restart: wallet and receipt commit once, and success is emitted only after durable storage.
- Watch start/stop/care failure rolls back identity/lease/profile changes consistently; last owner disconnect preserves resident visibility and restart-safe restoration.
- Future AFK start/cancel/settle uses the same ownership/revision/receipt boundaries; no client clocks or claimed gains.
- Both current protocol and v2 transition behavior are deliberate, versioned and tested across desktop, Tiny Garden and widget clients.
