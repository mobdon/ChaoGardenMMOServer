# Authoritative care contract

This is the server-side handoff for the Tiny Garden Items, Garden and Stats
screens. Care uses the same canonical profile as desktop play. The server owns
pickup identities and locations, inventory, needs, signed effects, species
history and progression. A client animation or cancelled visual never grants
or reverses an item effect.

## Evidence boundary

The local Steam SA2 audit establishes animal IDs 0–20, drive IDs 21–24 and
signed item-table fields. Reserved animal-table row 25 is unavailable. A drive
does not add an animal-history bit. Inventory possession, species ever used,
current appearance parts and learned behavior are different state.

The original ten-short row order is Mood, Belly, Swim, Fly, Run, Power,
Stamina, Luck, Intelligence and one unresolved short. Those fields are raw
native item-table units. The retail application path, multipliers, experience
overflow and clamps are unresolved. Mapping them to this server's needs or
skill points is versioned MMO tuning. Skill points, levels and grades remain
separate; no item grants an S grade or owner privilege.

Animal-to-habit acquisition, body-part replacement, rare-animal special
effects and the full lifecycle/Chaos evolution conditions remain unknown.
Species history alone never establishes earned Chaos eligibility. Existing
saved forms and owner privileges must survive migration. Desktop prototype
evolution requirements are an adaptation, not a retail-rule claim.

Original tables, game files, native assets and saves stay local and ignored.
Public tests use synthetic signed rows; source provenance identifies the
local audited input without redistributing the decoded table.

## Local native catalog

Set both `CHAO_CARE_AUDIT_DIR` (the locally produced audit directory) and
`CHAO_CARE_EXECUTABLE` (the audited Steam executable) to enable native
animal/drive rows. The server checks local audit provenance and signed rows
against the pinned executable. Supplying neither keeps the native catalog
disabled and empty. Partial configuration or invalid configured input fails
startup; it never substitutes guessed rows. Do not copy these files into the
repository, client bundle or public asset routes.

`CHAO_CARE_AUDIT_DIR` must contain `animals.json` and
`verification-report.json`; the loader also reads
`../extracted/care/manifest.json` relative to that directory. The usual client
layout is `.local/care/` alongside `.local/extracted/care/manifest.json`.
The executable must match audited SHA-256
`4b1e09d782a2d919cbed2324ba2c0c78ef5fbe1677d400edab39608191f12c16`.
Other revisions are rejected rather than treated as verified native parity.

Native stage drop tables and carry limits are unknown. Any enabled server
pickup placement and inventory limit are explicit MMO rules. The client may
request only pickup IDs advertised by the current canonical snapshot.

The current adapted mission drop ID is `missionId:enemyId`; its native item
selection is the first SHA-256 byte of that source ID modulo 25. Defeating a
robot can create one private persistent drop for its canonical recipient.
There are at most 100 outstanding drops per profile and 99 owned units per
item. Claimed source IDs stay protected by durable receipts. This selection,
private ownership and capacity policy are MMO adaptations, not retail spawn,
drop or carry rules.

## Wire contract

The authenticated WebSocket accepts a strict `care` envelope. Example:

```json
{
  "type": "care",
  "requestId": "00000000-0000-4000-8000-000000000001",
  "expectedRevision": 0,
  "intent": { "kind": "consume", "itemId": "garden-fruit" }
}
```

`requestId` is a client-created UUID for one operation. `expectedRevision` is
the unsigned care revision from the latest self snapshot. The accepted intents
are:

| Intent | Additional fields | Meaning |
| --- | --- | --- |
| `consume` | `itemId` string | Consume one already owned item and apply its server effect. |
| `purchase` | `itemId` string | Buy one server-catalog item at the authoritative price. |
| `tiny_purchase` | `itemId` string | Buy one Tiny shop fruit in an eligible safe garden without the desktop shop/grove proximity requirement. |
| `tiny_enter` | none | Durably opt into automatic Tiny residency; existing resident Watch may convert without a Take control step. |
| `training_start` | `stat`, `durationMs` | Start one explicitly adapted server-timed Tiny AFK job. See TRAINING.md. |
| `training_cancel` | `jobId` | Close an unfinished AFK job with zero gains. |
| `training_settle` | `jobId` | Settle a ready job using trusted time and actual bounded points. |
| `tiny_egg_purchase` | `eggId`, `stockEpoch` | Buy the server's current native egg offer into the owner's single held slot. See TINY_EGGS.md. |
| `tiny_egg_hatch` | `ownedEggId` | Explicitly hatch the owned egg into an additional inactive owned Chao, preserving the active canonical Chao. |
| `pet` | none | Request a bounded server-validated pet interaction. |
| `pickup` | `pickupId` string | Claim one available server-created nearby pickup. |

Unknown envelope or intent fields are rejected. Never add a player/target ID,
price, stat values, needs, timestamp or quantity. Use item IDs from the server
catalog; native species numeric IDs are evidence identifiers, not client
authority to create inventory.

Canonical inventory item IDs are `garden-fruit`, `tiny-fruit:N` (0–6), and `sa2:N`, where `N` is a
decimal native ID 0–24 without a leading zero. Native catalog row `id` maps
to that `sa2:N` inventory ID. Animal IDs 0–20 and drive IDs 21–24 retain their
native identity; `sa2:25` and noncanonical aliases are unavailable.

The owner-only top-level `snapshot.care` contains:

| Field | UI use |
| --- | --- |
| `revision` | Bind the next new intent to current canonical care state. |
| `inventory` | Render owned quantities; possession alone grants no history or skill. |
| `needs` | Render server-confirmed energy, satiety and happiness. |
| `evidence` | Show species use and unknown/disabled behavior, appearance and native evolution capabilities. |
| `rawTotals` | Audit signed native item-table units separately from applied gameplay deltas. |
| `pickups` | Select only server-advertised pickup IDs; the server rechecks proximity and availability. |
| `catalog` | Render enabled item identities, effect/version labels and availability. |
| `shop` | Render server prices and the explicit per-item `enabled` state. |
| `tinyShop` | Render the actual Tiny catalog, source availability, prices and `eligibility.canPurchase`. See TINY_SHOP.md. |
| `tinyResident` | Render the durable automatic-resident choice and current server activity mode. |
| `training` | Render the frozen policy, server-timed job, actual closing result and per-action eligibility. See TRAINING.md. |
| `history` | Render at most 64 actual durable care transactions, including buy/feed and AFK results, with trusted Unix timestamps. Legacy history starts empty. |
| `tinyEggShop` | Render source-backed hourly egg stock, prices, server epoch and purchase eligibility. See TINY_EGGS.md. |
| `tinyCollection` | Render the original active Chao, real additional owned hatchlings and the one persistent held egg. Active selection remains disabled. |
| `eligibility` | Enable only server-permitted care actions and display the conflict reason. |

Do not read inventory or request receipts from the public `players` list.
The existing public `player.stats` remains the seven-skill display source.

A durable response has `type:"event"`, `kind:"care-result"`, `requestId`,
`accepted`, `replayed`, `code`, `revision` and `effects`. `effects` contains
actual signed applied deltas after bounds; raw native units are separate.
Treat a negative delta as a loss, a zero delta as no applied change, and
`accepted:false` as no committed mutation. `replayed:true` reports the already
committed outcome and must not animate or add the effect a second time. Use
the returned `code` to explain a rejection and refresh canonical state before
constructing a different new request.

## Published adapted effects

Native skill fields map one-for-one to signed server skill-point deltas,
clamped to 0–9999. Items never change levels or grades. Mood and Belly map
one-for-one to happiness and satiety, each clamped to 0–100. The unresolved
final short, `unknown18`, is retained as raw evidence and grants no gameplay
effect. These mappings and clamps are MMO adaptations; they do not reproduce
an established retail consumption formula.

The separately authorized owner perfect-stat privilege is preserved. Its
skill points receive zero actual item delta, including negative native raw
effects; raw evidence and item consumption are still recorded. The returned
actual `effects.points` values reflect this exception. Item use never grants
that privilege to another profile.

`garden-fruit` is the adapted food item. A `purchase` costs three rings and
requires the server-approved hub shop or garden grove interaction. The explicit
`tiny_purchase` buys the same inventory item for three rings in a safe garden
without that proximity check; ordinary `purchase` behavior is unchanged. Purchasing
adds inventory; consuming it separately applies up to +35 energy, +30 satiety,
+8 happiness and +25 Stamina points, bounded by the corresponding caps. It
does not automatically grant a Stamina level. The UI must show these as
adapted effects rather than a decoded native fruit rule.

Consumption records species use only for native IDs 0–20. Drives 21–24 add
drive-use evidence and never species history, animal parts or learned habits.
The lifetime history policy is an MMO adaptation; native history reset timing
is unknown. `evidence.nativeEvolution`, `evidence.learnedBehavior` and
`evidence.animalAppearance` remain explicitly disabled/unknown until their
native rules and visual compatibility are verified.

Verified native data is required for native consumption, native shop offers
and pickup claims. An otherwise valid operation reports
`native-data-unavailable` when that data is disabled; an already owned native
item remains in inventory and is not substituted or discarded. Adapted Garden
Fruit remains available. The UI must use each `shop` entry's `enabled` state
instead of assuming that a listed native offer is usable.

## Client behavior

Render all seven canonical skills: `swim`, `fly`, `run`, `power`, `stamina`,
`luck`, `intelligence`. Keep each server-provided `points`, `level` and `grade`
value distinct. Negative effects are valid and must remain visible. Never
calculate authoritative gains from a native species name or item description.

Send intents only. Never send prices, quantities to award, stat deltas, needs,
history bits, learned flags, appearance, clocks or evolution results. The
authenticated connection selects the target canonical Chao; a supplied target
ID must not permit caring for another profile.

Use the server's current revision and eligibility. Disable mutations when
disconnected, while awaiting a response, or when another activity owns the
Chao. Cached snapshots are view-only. On a lost acknowledgement, retry the
identical request identity and payload; do not generate a second operation.
The payload includes `expectedRevision`: preserve its original value on a
retry even if a newer snapshot has arrived. The server resolves committed
retries before checking the current revision or current activity conflict.
The revision in a replay is the original commit revision; it must not rewind
a newer snapshot's revision.
Treat only a durable acknowledgement as saved, then reconcile the canonical
snapshot. Do not replay arbitrary offline item actions.

## Atomicity and save compatibility

A successful item use durably commits the item decrement, all signed effects,
needs, history/learning changes, revision and retry receipt together. A failed
save retains the prior inventory, progression, cooldown and world claim state
and emits no successful receipt. A duplicate committed operation cannot spend
or grant twice, including after reconnect or service restart. Reusing an
operation identity for a different payload is rejected.

Each profile permanently retains up to 4096 accepted care request receipts.
Receipts are never evicted. Exhaustion rejects new requests without consuming
items or granting effects; it does not reopen an old request ID. This is an
explicit bounded prototype limit requiring a versioned migration before it
can be expanded or replaced.

Legacy profiles without care state initialize an empty inventory and history.
They retain existing stats, rings, alignment, forms, equipment and privileges.
Unknown save object fields are retained with canonical known values taking
precedence. Strict care and result ledgers are replaced as complete documents
so consuming a loaded stack cannot resurrect the old saved quantity. The Node
reference refuses versioned Rust care profiles before opening a writer.
New canonical needs must persist rather than resetting on load. Present but
invalid versioned care data must not silently become an empty inventory.

Care revisions scope the care transaction. They do not by themselves implement
the separate account/device lease contract. Until that integration is present,
multiple connections with one bearer credential share one canonical profile.

New care requests are exclusive with manual Watch, active or saving Tiny minigames,
missions, races, Karate and mounted vehicles. The pickup intent alone may
operate during its eligible mission; it still validates canonical world,
position and once-only ownership. A new consume/purchase/pet request cannot
bypass the activity conflict. Automatic Tiny residents may purchase, feed and
receive care while their routine is active. `tiny_enter` may convert an existing
manual Watch into that persisted mode, but cannot bypass a game, reward, mech,
race, Karate or training conflict. Returning the durable result of an old request
does not mutate the current activity.

AFK training and exclusive device leases are separate workstreams. Their
host integration must extend the same authoritative care eligibility guard
before concurrent jobs or stale-device input can be enabled. This contract
does not claim an AFK host or exclusive device lease already exists.

## Acceptance evidence

The server tests must cover signed effects and clamping, all seven skill
channels, zero species-history gain from drives, duplicate requests,
same-ID/different-payload rejection, concurrent connections, stale revisions,
insufficient inventory/currency, remote and duplicate pickup claims,
save-failure rollback, restart retries and legacy profile preservation.

Test care against active Tiny minigames (including saving results), Watch,
missions, races, Karate and mounted vehicles. A new request cannot bypass an
activity lock. A retry of an already committed operation returns its prior
outcome without mutating a newer activity. Future AFK jobs and device leases
must extend that conflict check when their server modules are integrated.

The server-owned `Player.care.activity_lock` is a persisted integration hook
for those modules: a nonempty lock blocks new care, ordinary gameplay actions
and movement input. Only the owning server activity may clear it. Clients
cannot set it. A future job/lease host must reserve and release this lock in
its own durable transaction, clear stale input and reconcile restored jobs;
this field alone does not authenticate or revoke devices.

Browser acceptance separately requires two-client collection → inventory →
care → signed result → save/reload and real native visual inspection before
enabling body parts or learned animation grants.

Validation recorded November 18, 2026:

- `cargo test --locked --all-targets`: full server regression suite, including
  strict intents, all seven signed channels, save rollback, restart, activity
  conflicts and real duplicate-socket transport tests: 114 passed, with the
  two local-source tests separately opted in below.
- `cargo clippy --locked --all-targets -- -D warnings` and `cargo fmt --check`.
- `cargo test --locked --manifest-path crates/tiny-garden-games/Cargo.toml`:
  all 22 independently packaged minigame tests passed.
- `cargo test --locked --manifest-path crates/tiny-garden-core/Cargo.toml`:
  21 passed; the unrelated ROM-only reference test remained ignored.
- `node --test tests/reference/*.test.js`: 51 passed, including rejection
  of Rust care saves before the retained reference opens a writer.
- With both local care environment variables configured,
  `cargo test --locked --test care_native -- --ignored`: two opt-in tests
  passed against the pinned installed executable. They validate signed
  consumption and actual melee kill → advertised pickup → inventory → care
  → durable reload/replay using temporary saves. No native table is embedded
  in the tests and no live profile is written.

The shared host patch adds only care state load/export, needs persistence,
private snapshots, a durable WebSocket dispatch, activity guards and combat
drop hooks. The Tiny Garden implementation chat received this contract.
Clients using old consumable actions require the coordinated UI update before
deployment. This PR does not restart the preview or migrate live saves in place.

This worktree starts at the verified Rust base
`a8188a3030eb176ec4c3b7156f93410da5bb6f09`. During implementation, upstream
`main` advanced to `325db09` with documented unaccepted generated changes and
unrelated build blockers. The evidence above applies to this isolated care
branch; upstream merge/rebase and the accounts/lease adapter remain integration
gates. No competing TypeScript authority is adopted.

## Tiny shop and automatic residency follow-up

The isolated `codex/tiny-chao-shop` slice builds on care commit
`02a2572950a65229579817b4e334e8a635bc4c16`; fetched care publication
`22add7030e2e8003a6ac05c202aff58864c9fcbf` has the identical Git tree. The same
care transaction now handles Tiny purchases, native-fruit consumption and the
durable `tiny_enter` preference. Exact protocol, verified source requirements,
native/application distinctions and the new acceptance record are in
[TINY_SHOP.md](TINY_SHOP.md). This is isolated tested source, not a runtime rollout
or a completed native Tiny Garden engine.
