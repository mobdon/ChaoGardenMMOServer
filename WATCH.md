# Shared garden residents

Tiny Watch opts the canonical Chao into a small server-driven garden routine.
The Chao remains visible to other connected profiles after its owner closes
the browser. It idles, makes short terrain-checked walks, and rests. This is
an adapted resident routine; it does not execute the retail Chao AI engine.
The server must remain running. A stopped server does not simulate elapsed
wall-clock time, award offline rewards, or complete races or Karate.

## Protocol

Send `{"type":"action","action":"watch-start"}` from the hub or an eligible
garden. The hub transfers to Neutral Garden; a current garden is retained.
The server dismounts vehicles and rejects active Tiny games, missions, races,
and Karate. Manual movement, Tiny games, and other gameplay actions remain
blocked until Watch stops. Explicit Save remains available.

The self player and public player list include `watch`, null in direct mode.
An active value contains `id`, `mode:"watch"`, `activity` (`idle`, `wander`,
or `rest`), `startedAt`, `activityStartedAt`, `activityUntil`, optional `target`
and `lastCare`, `resident:true`, `ownerOnline`, and `rules:"adapted-resident"`.
Use canonical player coordinates for rendering.

`behaviour` carries the finer server-chosen behaviour from `src/behaviour.rs`:
`kind` (`idle`, `wander`, `sit`, `sleep`, `play`, `happy`, `startled`),
`variant` (a small server-chosen index the client maps onto its native clip
list), `phase` (`enter`, `loop`, `exit` for sit and sleep; `loop` otherwise),
`startedAt`, `until`, `phaseStartedAt`, `phaseUntil` and
`rules:"placeholder-tuning"`. `activity` stays the coarse legacy value: `sit` and
`sleep` report `rest`, `wander` reports `wander`, everything else reports
`idle`. Behaviour weights and durations are placeholders until the decoded native values replace them, because
the native behaviour controllers and timers are mapped but not decoded. The
picker reads needs through a read-only adapter (currently the prototype
`energy` and `happiness`) and never writes them. The server never names motion
clips; every observer sees the same behaviour. Public `connected` reports
whether any owner socket is connected; residency does not imply connectivity.

Stop with `{"type":"action","action":"watch-stop","sessionId":"current-id"}`.
Care uses `{"type":"action","action":"watch-care","sessionId":"current-id","kind":"pet"}`.
Allowed care kinds are `pet`, `rest`, and `wake`. Pet adds 2 happiness up to 100
and shows `happy`, or `startled` if the Chao was asleep. Rest starts `sleep`;
wake shows `startled` from sleep, otherwise `idle`;
all care shares a 1.5-second server cooldown. Rest and wake change the routine
state. No care awards rings or skill levels. Routine activity itself changes
no rings, stats, happiness, energy, or needs and collects no world pickups.
Extra fields, including client rewards, targets, and clocks, are rejected.
Stale or another profile's session ID cannot control the resident.

## Durability and reconnection

Start, stop, and care pass through the authoritative save transaction before
`{"type":"event","kind":"watch-saved","accepted":true,"watch":{},"serverTime":123}`.
`accepted:false` means the intent did not change state, including cooldown
or validation rejection. `watch` is null after an accepted stop. Disk failure
rolls back the initiating profile, mode, currency, cooldowns, and queued events;
the service sends an error notice and keeps the prior mode.

Profiles store only `resident:{enabled:true,zone,x,z}` or null. Server restart
validates eligibility and safe ground, restores one canonical resident, and
creates fresh routine timers and a fresh watch ID. It applies no catchup.
Resident position checkpoints become dirty every 15 simulation seconds and use
the existing periodic save path. Reconnecting reuses the same profile/entity;
the current watch ID from its snapshot is required to stop. Multiple sockets
for one credential share the same resident and can issue competing intents.

Rust and Node tests cover disconnected visibility/movement, rest, no passive
rewards, strict care intents, profile isolation, stale IDs, restoration and
direct-control return. Rust additionally covers holes and atomic disk-failure
rollback; the real WebSocket test verifies spectator visibility, owner
disconnect/reconnect, durable start/stop, and one canonical entity.
