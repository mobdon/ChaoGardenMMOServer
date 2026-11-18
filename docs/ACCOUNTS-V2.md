# Accounts, Android pairing and activity ownership v2

This is the Rust service's account/device contract. Discover it at
`GET /api/status` under `accounts` or at authenticated `GET /api/account` under
`capabilities`. Require `version:2` before enabling controls. The capability
object advertises `authentication:["bearer-header","websocket-first-frame"]`,
pairing, revocation, revisions, leases, intent sequences and HTTP intents.
`durableTraining:false` means this release does not offer AFK training jobs.

The store is local JSON owned by one running Rust process. These accounts bind
devices to existing Chao; they do not implement account passwords, Steam/DLC
entitlement, a production database or public deployment. Existing server-assigned
owner roles remain authoritative. Pairing never grants or changes a role.

## Authentication and snapshots

Use `Authorization: Bearer <device credential>` for account HTTP requests and
native WebSocket upgrades. Browser WebSocket clients open `/ws` with no query
string and send exactly this first text frame within five seconds:

```json
{"type":"authenticate","token":"device credential"}
```

Authentication precedes the first snapshot. Invalid bearer upgrades return HTTP
401; failed/missing first-frame authentication closes the socket with policy
code 1008. `/ws` rejects every query string, including
legacy token queries. Never place credentials or pairing secrets in URLs, logs,
analytics, bundles or exported debug state. Use HTTPS/WSS outside loopback and
configure application origins. The application does not log request bodies or
credentials; any reverse proxy must also exclude authorization headers and bodies
from logs. Responses carry `Cache-Control:no-store`.

`POST /api/session` with JSON `{"name":"Chao name"}` creates a desktop device
and returns `{token,player,identity}` after persistence. Resume an existing local
save with `{"token":"existing credential","resumeOnly":true}`. Invalid
credentials fail without creating a replacement Chao. Local browser save slots
retain their existing credentials after migration. Keep this bootstrap endpoint
on the private development service until a public account policy is implemented.

`GET /api/account` requires the bearer header and returns:

```json
{
  "identity": {
    "schemaVersion": 2,
    "accountId": "stable-account-id",
    "chaoId": "existing-player-id",
    "revision": 7,
    "deviceId": "this-device-id",
    "devices": [{"id":"desktop-id","kind":"desktop","revoked":false}],
    "lease": {
      "id": "current-lease-id",
      "deviceId": "desktop-id",
      "activity": "watch",
      "expiresAt": 1794999300000,
      "lastSequence": 3
    }
  },
  "snapshot": {"type":"snapshot","protocolVersion":2},
  "capabilities": {"version":2,"durableTraining":false}
}
```

The example abbreviates the normal snapshot/capability fields. `lease` is null
when absent. The full snapshot also contains the authenticated `identity` and
canonical gameplay state. `snapshot.self` always contains the authenticated Chao,
including when disconnected without an active resident; `players` also includes
that Chao once for its own account view. This preserves widget access without
inventing an online connection. WebSocket snapshots contain those same private fields.
Public player lists expose Chao gameplay state, not account IDs, device hashes or
pairing state. Every paired device observes the same `chaoId`, wallet, seven
stats, inventory/records and active game; pairing creates no duplicate Chao.

## Desktop-approved Android pairing

1. Read the desktop account's current revision.
2. The authenticated desktop explicitly approves a new invitation with
   `POST /api/pairing`, bearer header and JSON `{"expectedRevision":7}`.
   The response is `{pairingId,pairingSecret,expiresAt,revision}`. Creation itself
   is desktop approval; Android devices cannot create invitations.
3. Transfer that short-lived JSON invitation to Android, for example as a QR
   payload. Do not encode the desktop credential or a lasting Android credential
   into the QR. The invitation expires after 120 seconds and replaces any prior
   unclaimed invitation for this account.
4. Android sends `POST /api/pairing/claim` with JSON
   `{"pairingId":"...","pairingSecret":"..."}`. No bearer is needed for
   this one request. After the atomic save succeeds, the response is
   `{token,deviceId,identity}`. Store the returned credential in protected native
   storage and discard the invitation. The server retains only SHA-256 hashes.

Wrong, expired, replaced and consumed invitations return `invalid_pairing`.
Claim consumes the invitation and registers the credential in the same profile
transaction. Failed persistence leaves the invitation redeemable; replay after a
successful commit cannot issue another device. Losing the response after a
successful claim requires a new desktop-approved invitation. There are at most
32 stored device records per account, including revoked records.

Revoke from an active desktop with `POST /api/devices/revoke` and JSON
`{"deviceId":"target-device-id","expectedRevision":9}`. The response contains
the updated `identity`. Revocation invalidates authentication, clears that
device's lease and held movement, and cancels invitations it approved. Existing
sockets lose access and close; reconnect cannot restore a revoked credential.
The autonomous resident/game state is not converted into a new Chao or rewarded
by revocation. Android cannot revoke devices. Desktop self-revocation is allowed:
revoking the last desktop removes the ability to pair or manage devices through
these endpoints. There is no password-reset or recovery-key API in this release;
retain an authorized recovery backup before doing this.

## Exclusive activity leases and revisions

All gameplay intents require an active device credential, the current profile
revision, a matching unexpired lease and a strictly increasing sequence number.
Reading snapshots and explicitly saving are allowed without a lease.

Send `POST /api/lease` with the bearer header. The operations are:

| Operation | JSON fields in addition to `operation` | Result |
| --- | --- | --- |
| `acquire` | `activity`, `expectedRevision` | New exclusive lease ID; `lastSequence:0` |
| `renew` | `leaseId`, `expectedRevision` | Same lease ID/counter; new expiry |
| `release` | `leaseId`, `expectedRevision` | Lease cleared and held movement reset |

Each successful operation returns `{identity}`. Activities are `direct`, `tiny`,
`watch`, `training` and `competition`. A lease lasts 30 seconds on the server's
wall clock, independently of gameplay simulation ticks. Renew before expiry;
after expiry acquire a new generation. A current lease cannot be replaced even
by the same device: explicitly release it before handing control to another
device/activity. Lease expiration or release does not cancel the server-owned
resident, pending result or other runtime activity. An active runtime activity
must permit the requested activity; a device cannot evade it by waiting for TTL.

Profile revisions increase with durable identity or profile changes, including
earned results and intent counters. No-op saves do not advance them. Lease
renewals, pairing and another device's actions can make a prior revision stale.
After any successful response, use its latest identity. On conflict, fetch a new
account snapshot and reconcile before sending another intent. Do not blindly
reapply purchases/care after an uncertain response: check the canonical result
and `lastSequence` first.

For a Tiny Garden handoff, finish/stop the old activity as appropriate, release
the old lease, acquire `tiny`, then start the server minigame. For Watch care,
acquire `watch` and use the current watch ID from the account snapshot as `sessionId`.
An expired Watch controller can reacquire `watch` to resume care; it cannot
acquire conflicting direct gameplay until the resident is stopped. Training and
competition are valid reservation names; this does not implement AFK training
handlers or autonomous competition scheduling.

## Intent envelopes and acknowledgements

WebSocket gameplay keeps the existing intent fields and adds fencing fields:

```json
{"type":"action","action":"watch-care","sessionId":"current-watch-id","kind":"pet","leaseId":"current-lease-id","expectedRevision":12,"sequence":4}
```

Native widgets can send the same intent using `POST /api/intent`, a bearer
header, `Content-Type:application/json` and the HTTP envelope:

```json
{"leaseId":"current-lease-id","expectedRevision":12,"sequence":4,"message":{"type":"action","action":"watch-care","sessionId":"current-watch-id","kind":"pet"}}
```

Use a positive `sequence` greater than `identity.lease.lastSequence`. The counter
survives save/restart and reconnect/renewal for that lease. Old revisions, lease
IDs, another device's lease, duplicate/reordered sequences, wrong activities and
revoked credentials cannot apply an intent. If multiple sockets share one
device, their writer must coordinate one counter; reconnect does not reset it.

An accepted transport intent returns
`{type:"event",kind:"intent-accepted",sequence,identity}` only after the atomic
profile save. This confirms validation/persistence of the intent, not a gameplay
reward or that an in-world precondition succeeded. Watch intents return
`{type:"event",kind:"watch-saved",sequence,identity,accepted,watch,serverTime}`;
`accepted:false` means no Watch/care change, such as cooldown rejection. The
monotonic intent counter is still committed. Game result receipts and success
events remain separately authoritative.

`{"type":"action","action":"save"}` is exempt from lease/revision/sequence
fencing after authentication. Over HTTP put this message inside `message`. It
uploads no profile state and returns the existing `saved` event with the latest
`identity` only after successful persistence.

WebSocket rejections are
`{type:"error",error:"stale_revision",protocolVersion:2}`. HTTP rejections use
`{error:"stale_revision",protocolVersion:2}` with an appropriate status:

| HTTP status | Representative errors |
| --- | --- |
| 400 | `invalid_request`, `invalid_activity` |
| 401 | `unauthorized`, `device_revoked`, `invalid_pairing` |
| 403 | `desktop_approval_required` |
| 409 | `stale_revision`, `activity_conflict`, `lease_required`, `stale_lease`, `lease_expired`, `replayed_intent`, `device_limit` |
| 500 | `persistence_failed` |

The new account endpoints reject unexpected top-level fields. Existing game intent
validators continue to reject forged stats, rewards, scores and clocks. Gameplay
payloads remain bounded to 2 KiB and 80 WebSocket messages per second. Clients
must coordinate transport authentication **and** fencing before integration;
changing only the WebSocket URL leaves old controls incompatible with v2.

## Migration, atomic persistence and recovery

On startup, each legacy profile without `identity` gains a schema-v2 identity.
Its existing player ID becomes `chaoId`; one stable account ID is assigned and
the existing token hash becomes an active desktop device. Legitimate stats,
currency, ownership, equipment, resident opt-in and earned record/receipt fields
remain server data. No client profile upload or entitlement claim is accepted.
The retained top-level `tokenHash` is not an authentication bypass: revoking its
migrated desktop device disables that credential after restart as well.

Before replacing a legacy save, the service writes and fsyncs an exact original
`profiles.v1.backup.json`. An existing backup must match the migration source;
conflicts fail startup for review. Invalid/future identity schemas, missing or
malformed legacy hashes and duplicate Chao/account/device identities fail closed
without overwriting the original. Migration retries safely after a blocked temp
file or other recoverable storage failure.

Identity, lease/counter, profile revision, progression and reward receipt share
one `profiles.json` transaction. The service writes `profiles.json.tmp`, fsyncs
it and atomically replaces the destination before publishing success. Failed
device/intent transactions restore runtime state, queued events and ownership;
the previous durable file remains authoritative. Revision publication waits for
the same commit. Tests exercise write/rename failure and restart; this is not a
claim of database replication or protection against every hardware/power fault.
Run only one authority against the data directory.

The writer also takes an OS lock on `profiles.lock` and compares the current
document with its last committed bytes before replacement. A competing writer
or stale loaded authority fails without overwriting newer progression; reload
the authoritative state before retrying. Rust 1.89 or later is required for the
file-lock API. The release `migrate` command uses this same atomic path and does
not trust an old `.schema-version` marker to skip validation.

Authenticated sockets are counted per device as well as per Chao. An observer
does not keep another device's abandoned interactive Tiny round alive. A
controlling device's final disconnect or revocation stops held input and cancels
unfinished interactive activity while preserving opted-in Watch and pending
terminal rewards. An authenticated observer can flush a pending reward through
Save; the receipt and credit still commit once.

For recovery, stop the authority, archive the entire current data directory and
restore a complete known-good **v2** backup into an isolated directory. Point a
test instance at it and verify canonical IDs, wallet/records, revisions and
revocation before switching the configured data directory. Prefer a backup made
after the latest earned result. The original v1 migration backup is an emergency
source for an isolated migration rehearsal; restoring it as live data discards
all post-migration gains and revocations. Do not overwrite a healthy v2 save with
that older file. The retained Node reference is v1 and refuses account/device
profiles: its constructor and direct profile creation reject any `identity`
field or explicit non-v1 `schemaVersion` before normalization, and export repeats
the guard before writing. Startup fails before listening or touching a migrated
save. This guard does not implement v2 authority in Node; use Rust for v2 data.

## Integration handoff and evidence

The pairing, Tiny Garden and Android widget clients must consume this same
contract. Widgets can read `/api/account` without taking control and use bounded
HTTP Watch intents for care. The full Tiny Garden app owns minigames and future
AFK controls. Cached widget state must retain its timestamp/stale indication and
must never fabricate needs, timer completion or stat gains.

AFK integration must add the persistent job to `accounts::runtime_activity` and
commit job/cost/result/receipt with identity, sequence and revision. Device lease
expiry must never release or settle a durable training job by itself. Until that
integration is delivered and tested, `durableTraining:false` is the gate and
clients must not present functioning AFK controls.

`tests/identity.rs` covers pairing expiry/replay, revocation, strict migration,
lease expiry/generations, reconnect and durable sequence fencing.
`tests/identity_game.rs` covers canonical identity and progression/owner-role
preservation. `tests/accounts.rs` exercises exact migration backup/recovery,
HTTP/native/browser authentication, concurrent devices, revocation of connected
sockets, stale/replayed intents and failed-write rollback in isolated stores.
The existing Tiny Garden/Watch suites remain the regression evidence for earned
receipts and resident gameplay. This work does not restart or edit the live
preview/profile; client integration and actual Android device QA are separate.

The current correctness-first transport fsyncs the full JSON profile store for
each accepted gameplay frame, including movement, to keep the intent counter and
profile transaction together. Its latency and player capacity are unmeasured;
it does not establish the planned 100–500-player target. A measured persistence
and stream design is a separate integration gate before public deployment.
`tests/reference/identity-compat.test.js` proves the Node guard rejects v2 before
runtime construction, export or process startup without changing save bytes.

Validation on November 18, 2026: `cargo test --locked` passed 128 host tests;
`cargo fmt --all -- --check` and strict all-target Clippy passed. The separate
Tiny core passed 21 tests and Tiny games passed 22, with formatting and strict
Clippy; one ROM-dependent reference test was intentionally ignored because no
reference ROM was configured. All 51 isolated Node reference tests passed.
Recovery checks include exact v1 backup, opaque nested field preservation,
latest-v2 restore with revoked devices and earned receipts, failed atomic
creation/care/revocation, stale-writer/OS-lock exclusion and pending terminal
reward settlement by an observer after the controller disconnects.

The implementation started from `a8188a3` and was rebased onto later upstream
history. It retains the independent exact native Run asset route and repairs
the upstream Rust metrics borrow/Config initialization blockers required to
test the combined source. This does not accept the generated mobile/API or
deployment stacks. Tiny UI and Android widget chats received the v2 contract;
no active AFK training chat was available during coordination, so its consumer
handoff is recorded here and `durableTraining:false` remains explicit.
