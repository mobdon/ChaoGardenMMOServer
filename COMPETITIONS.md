# Opt-in away competitions, protocol v1

The Rust authority can enter the same canonical Chao in a bounded plan of AI
races or Karate bouts. Its owner may disconnect while the service continues
running. The plan, frozen match inputs, elapsed simulation time, saved results,
and ring receipts belong to that profile. A reconnect or a second socket does
not create another competitor or another clock.

Rules `native-gsne8p+mmo-v1`. Every outcome rule is a port of the GameCube
SA2B Chao modules (`src/competition_native.rs`; the same routines in the
client crate are checked bit for bit against the GameCube PowerPC):

- Race: the course table and class layout, the rival set, lane shuffle and
  rival skills, start stamina, the run/swim/fly/climb movement laws, the run
  trip roll and stamina drain, and the win rules for progress, medals,
  unlocks and emblems.
- Karate: the opponent table and row choice, fighter setup, the hit check,
  damage with its 5% floor, HP and zeal, the charge, attack outcome and
  give-up rules, the 90-second clock, the knock-out and HP-percentage
  judgement, five wins per class and the class emblem.

`src/competition_mmo.rs` supplies only what the server does not run
natively: the 3D course (abstract stretches per native section set), the
movement integrator, karate motion lengths, task order, the random seed,
replay sampling, optional player-Chao rivals and the ring payout. All of it
is in `MmoConfig`; the defaults are listed there and can be overridden in
`CHAO_DATA_DIR/activities/competition-mmo.json`. Saved results keep the
config they ran with.

The native tables are read from the owner's own install, never from git:

```
sa2-assets chao-competition-tables <Steam>/sonic2app.exe .local/chao-competition/competition-native.json
```

Copy that file to `CHAO_DATA_DIR/activities/competition-native.json`.
Without it no competition is offered. Karate opponent names come from the
existing local `activities/karate.json` `names` list when present.
Native race and karate do not change stats or needs, so this feature calls
no `life::` stat API. Emblems go through `emblems::award` only.
Results saved by `adapted-ai-competition-v1` still load.

## Eligibility, ownership, and budgets

Starting requires an authenticated connected owner socket, a hub or eligible
garden, no active Tiny game/manual race/Karate/mission, no mounted vehicle, and
no existing activity or training lease. An existing resident Watch routine may
remain enabled, but its movement pauses while the competition plan holds the
canonical Chao. There is one plan and at most one match per profile.

The owner chooses one through three matches of one kind. The first entry is
created by the server; later entries have a 60,000 ms simulation-time gap. No
plan automatically renews its budget. The exclusive `activityLease` persists
through the whole plan, including gaps, and is released after its final saved
result or a saved cancellation.

Training remains Tiny Garden's separate job system. A foreign `activityLease`
and non-null persisted `training`, `trainingJob`, or `trainingLedger` marker
block entry. Foreign leases and unknown profile fields round-trip unchanged.
This is an activity integration contract, not a claim that cross-device
pairing or a complete durable training host is implemented by this change.
Future training integration must acquire the same canonical activity lease
and refuse a competition reservation.
If another host integration replaces a live competition lease, competition
time pauses and persistence refuses to overwrite that foreign reservation.
The owner may cancel the competition safely; cancellation preserves the
foreign lease in memory and on disk, including across restart.

While reserved, normal movement, world ring pickups, manual race rewards,
Tiny games, and other gameplay/training actions cannot run concurrently.
Explicit Save and competition intents remain available.

## Owner intents

Read the current `competition.revision` from a snapshot. Start with exactly:

```json
{"type":"action","action":"competition-start","expectedRevision":0,"kind":"race","matches":1}
```

`kind` is `race` or `karate`; `matches` is an integer from 1 through 3.
There is no client seed, opponent, course, clock, placement, score, damage,
wallet credit, or result field. Unknown fields and unsupported actions are
rejected. Stale revisions and duplicate competing-device intents cannot
replace an entry or expand a plan's budget.

Cancel the active unpaid match and all remaining entries with the current
revision and the current plan ID:

```json
{"type":"action","action":"competition-stop","expectedRevision":1,"planId":"server-plan-uuid"}
```

A cancellation grants no reward. A result already waiting for its atomic
save cannot be cancelled, replaced, or claimed through another intent.
Previously committed results and ring credits remain intact.

State-changing intents pass through the authoritative save before returning:

```json
{"type":"event","kind":"competition-saved","accepted":true,"competition":{},"serverTime":123}
```

`accepted:false` means no requested state transition was committed. A disk
error restores the initiating profile's previous plan, lease, revision,
budget, currency, cooldowns, and queued events and returns an error notice.

## Simulation and rewards

At each entry the server freezes the canonical seven stat levels, native
rival/opponent reference, fresh server match ID, seed, and rule version.
Later client messages cannot change those inputs. Rival raw values are
normalized to canonical-equivalent levels using documented MMO scaling in
`src/competition_sim.rs`; native random-range interpretation remains unknown.

The deterministic race simulation records segment progress, stamina, actions,
finish times, and placements. The deterministic Karate simulation records
bounded movement/range, timed attacks and recovery, guard/dodge/focus,
HP/stamina, and KO/timeout/draw decisions. Seven canonical capabilities affect
performance. The generated replay is frozen with the entry, and live views
show only the frame reached by elapsed server simulation time.

Equal race finish times use stable server competitor-ID ordering. Karate ends
at KO or 60 seconds and compares remaining HP ratios; an exact tie places both
fighters second and grants no win reward. A simultaneous-KO test proves that
draw branch.

The adapted race payout is 25/15/5/0 rings for ranks 1/2/3/4. Karate pays
50 rings for a win and zero for a loss or draw. Wallet credit is capped at
999999; `earnedRings` preserves the pre-wallet-cap amount. These competitions
grant no stat levels, native medals, toys, unlock flags, or ordinary manual
race completions. Resident wandering and deliberate resident care never
produce these competition rewards.

## Durability, restart, and replay

Only trusted 50 ms service ticks advance elapsed match or gap time. Process
downtime grants no catch-up, synthetic matches, or rewards. Normal progress
uses the existing five-second periodic profile save; a crash may resume from
the last saved elapsed checkpoint. An accepted entry and its frozen simulation
remain stable across restart, independently of current catalog changes.

A terminal result stages a proposed ring credit and unique match receipt in
the same `profiles.json` transaction. The live wallet, saved result list, and
success event remain unchanged until fsync and atomic rename succeed. A save
failure leaves the earned result pending and the profile reserved, retries
without advancing another match, and allows explicit Save to retry. A restart
before that commit resumes the last durable state of the same deterministic
match; a restart after commit loads its receipt and credited wallet together.
There is no client claim path that can pay the result a second time.

After commit, the service broadcasts:

```json
{"type":"event","kind":"competition-result","profileId":"canonical-profile-id","result":{"id":"server-match-uuid","kind":"race","rank":1,"rings":25,"earnedRings":25,"saved":true,"replayAvailable":true}}
```

The owner's snapshot and public player data include `competition` with
`version`, `revision`, `plan`, `active`, `saving`, `completedTotal`, and
`results`. `active` contains match ID/kind, elapsed/duration, running or saving
phase, frozen competitors, rule version, and `currentFrame`. Future frames,
seed, and unsaved terminal results are not disclosed by that live view.
`results` contains up to ten committed summaries with placements, reason,
credited and earned rings, duration, and rule version. `completedTotal`
continues counting after older replay history is pruned.
Restore validates ledger relationships and reproduces frozen simulations from
their saved seeds and competitors; corrupted frames, conflicting kinds, and
duplicate receipts fail closed before scheduling or crediting.

Spectators receive the broadcast result and may request a retained saved
replay for an observed profile:

```json
{"type":"action","action":"competition-replay","profileId":"canonical-profile-id","matchId":"server-match-uuid"}
```

Omit `profileId` to request the caller's own result. The caller receives a
targeted `competition-replay` event containing the saved summary, complete
frozen simulation, seed, and reference provenance. It is read-only and does
not credit rings or change the target profile. Unknown, future, cancelled,
unsaved, or no-longer-retained match IDs produce no replay. Credentials and
private profile fields are never part of the response.

## Validation

`tests/competition.rs` uses synthetic native-shaped catalogs and temporary
profiles. It covers both away match slices, opt-in/budget bounds, forged and
replayed intents, duplicate sockets, profile isolation, durable cancellation,
foreign training leases, unknown profile field preservation, no world/manual
reward leakage, mid-match restart, pending save failure/retry, committed
restart, once-only receipts, spectator replay, and passive resident isolation.
`tests/competition_protocol.rs` exercises the authenticated WebSocket path;
embedded simulation tests cover determinism, scaling, and bounded frames.

Run the focused proof with:

```text
cargo test --locked --test competition --test competition_protocol
cargo test --locked competition_sim
```

An additional ignored proof uses the user's locally extracted catalogs without
opening the live service's profiles. Set `CHAO_COMPETITION_REFERENCE_DIR` to
the local `.local/activities` directory, then run:

```text
cargo test --locked --test competition locally_extracted_rosters -- --ignored --nocapture
```

The proof copies only `races.json` and `karate.json` into a temporary authority,
completes one race and one Karate bout with a temporary canonical owner offline,
restarts that temporary authority, and replays both saved receipts. It checks
that manual native course availability remains unchanged and reports only
kind, rank, credited rings, and durability. An explicit ignored-test run fails
if the environment variable or either local catalog is missing; CI uses the
synthetic fixtures and does not require native game content.

Native race obstacle/path fidelity, full tournament tiers, client replay
presentation, and device-pairing integration remain separate work. This PR
contains the first two server slices and the versioned client contract.
