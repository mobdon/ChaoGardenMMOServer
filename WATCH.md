# Shared garden residents

Tiny Watch opts the canonical Chao into a small server-driven garden routine.
The Chao remains visible to other connected profiles after its owner closes
the browser. It idles, makes short terrain-checked walks, and rests. This is
an adapted resident routine; it does not execute the retail Chao AI engine.
The server must remain running. A stopped server does not simulate elapsed
wall-clock time, award offline rewards, or complete races or Karate.

## Protocol

Authenticate and acquire a `watch` lease using [Accounts
v2](docs/ACCOUNTS-V2.md). Desktop and paired Android devices use separate
revocable credentials for the same Chao. Send
`{"type":"action","action":"watch-start","leaseId":"current-lease-id","expectedRevision":7,"sequence":1}`
from the hub or an eligible garden. The hub transfers to Neutral Garden; a current garden is retained.
The server dismounts vehicles and rejects active Tiny games, missions, races,
and Karate. Manual movement, Tiny games, and other gameplay actions remain
blocked until Watch stops. Explicit Save remains available.

The self player and public player list include `watch`, null in direct mode.
An active value contains `id`, `mode:"watch"`, `activity` (`idle`, `wander`,
or `rest`), `startedAt`, `activityStartedAt`, `activityUntil`, optional `target`
and `lastCare`, `resident:true`, `ownerOnline`, and `rules:"adapted-resident"`.
Use canonical player coordinates for rendering. Public `connected` reports
whether any owner socket is connected; residency does not imply connectivity.

Stop with `{"type":"action","action":"watch-stop","sessionId":"current-id","leaseId":"current-lease-id","expectedRevision":9,"sequence":3}`.
Care uses `{"type":"action","action":"watch-care","sessionId":"current-id","kind":"pet","leaseId":"current-lease-id","expectedRevision":8,"sequence":2}`.
Each request uses the latest revision and a sequence above the current lease's
`lastSequence`; the numbers here illustrate the shape. Native widgets can use
authenticated `GET /api/account` for snapshots and `POST /api/intent` for care,
with these gameplay fields inside `message` and fencing fields on the envelope.
Credentials never enter URLs. Widget refresh does not require a lease; care does.
Allowed care kinds are `pet`, `rest`, and `wake`. Pet adds 2 happiness up to 100;
all care shares a 1.5-second server cooldown. Rest and wake change the routine
state. No care awards rings or skill levels. Routine activity itself changes
no rings, stats, happiness, energy, or needs and collects no world pickups.
Extra fields, including client rewards, targets, and clocks, are rejected.
Stale or another profile's session ID cannot control the resident.

## Durability and reconnection

Start, stop, and care pass through the authoritative save transaction before
`{"type":"event","kind":"watch-saved","accepted":true,"watch":{},"serverTime":123,"sequence":2,"identity":{}}`.
`accepted:false` means the intent did not change state, including cooldown
or game validation rejection; its accepted fencing sequence is still persisted.
`watch` is null after an accepted stop. Ownership validation failures return a
v2 error instead of a success event. Disk failure rolls back profile, mode,
currency, cooldowns, sequence and queued events; the service returns
`persistence_failed` and keeps the prior mode.

The resident portion of profiles stores only
`resident:{enabled:true,zone,x,z}` or null, alongside the account identity,
revision and lease. Server restart
validates eligibility and safe ground, restores one canonical resident, and
creates fresh routine timers and a fresh watch ID. It applies no catchup.
Resident position checkpoints become dirty every 15 simulation seconds and use
the existing periodic save path. Reconnecting reuses the same profile/entity;
the current watch ID from its snapshot is required to stop. The current lease
and revision are also required. A 30-second device lease expires on the server's
wall clock; Watch continues until explicitly stopped. Reacquire a `watch` lease
after expiry to resume care. A conflicting direct/Tiny lease cannot bypass the
active resident. Release the Watch lease after stopping to hand control back.
Revoking a controlling device removes its access and held lease while retaining
the canonical resident. Shared-device sockets must coordinate the same monotonic
counter; stale, reordered or replayed care cannot repeat an effect.

Rust and Node tests cover disconnected visibility/movement, rest, no passive
rewards, strict care intents, profile isolation, stale IDs, restoration and
direct-control return. Rust additionally covers holes and atomic disk-failure
rollback; the real WebSocket test verifies spectator visibility, owner
disconnect/reconnect, durable start/stop, and one canonical entity.
