# Owned Tiny eggs and additive Chao collection

November 18, 2026. The Rust service now buys and explicitly hatches the native
Tiny Garden eggs into additional owned records. It preserves the existing
active Chao, account token hash, owner/admin role, stats, gear, needs and shared
wallet. A hatchling is not another credential identity or `Game.players` entry.

This first collection phase is real, durable ownership and native newborn
data. Selecting another active/playable Chao remains disabled until per-Chao
gameplay, care, gear and training targets are integrated safely. The original
canonical Chao remains active throughout this phase.

## Native source and explicit adaptations

The existing `CHAO_TINY_GARDEN_PAYLOAD` / `Config::tiny_garden_payload` path is
the only source configuration. Its bounded local read must pass the existing
exact A2NE payload gate (191340 bytes, SHA-1
`2c7da8e1c9af85986eecf4effae39197cdac9abf`) and the native egg-price,
name-table and body-template span hashes. Missing source disables stock and
hatching without discarding already owned eggs or Chao. An invalid configured
source fails startup. No price list, source template or ROM bytes substitute
for that gate in production.

The host uses the original Rust `NativeShopState::open_stock` weighting and all
twelve `NativeShopPrices` egg prices. The native source IDs remain 0..11; their
mapping to SA2 jewel colors, genetics or desktop appearances is unverified.
Display labels are therefore `Native Egg N`. Source palette/rendering work
must establish any richer label or appearance separately.

Hourly stock rotation is the agreed MMO adaptation, `adapted-tiny-owned-eggs-v1`.
The server derives the hour from trusted Unix milliseconds / 3600000. Each
owner has stable private server-derived stock entropy; the active hour seeds
the native weighted selection. Reading a snapshot, opening a panel,
reconnecting or restarting cannot reroll that hour. The adapted native caller
counter uses the bounded server epoch. Existing-Chao field 58 is set to zero
and the native jewel eligibility gate is enabled explicitly in this host
adapter; this does not claim those fields were imported from the SA2 owner.

Only **one purchased egg may be held per owner**. It persists unchanged when
the hour changes, the app closes or the service restarts. There is no automatic
offline hatch. An explicit authenticated online hatch consumes it and adds one
new owned Chao, freeing the slot. The same current-hour offer may then be
bought again. The adapted collection cap is eight total Chao, including the
original active Chao; a purchase reserves room for its eventual hatch.

Hatching calls the existing native `hatch_into_empty_native_slot` constructor
on a new, empty temporary record. It never passes the current owner's native
or canonical record. Native RNG state is server generated and durably retained
with the egg/collection. The original five grade branches/default fields,
name selection and body initialization come from the Rust source port. The
native caller's identifier counter is explicitly adapted to low 32 bits of
trusted host milliseconds. Original encoded names remain private encoded
bytes; `Tiny Chao N` is a display label, not a claimed native name decode.

## Real service contract

Status advertises `tinyGarden.ownedEggs` with `enabled`, `rulesVersion`,
`rotationMs:3600000`, `maxHeldEggs:1`, `maxChao:8`, `selectionEnabled:false`.

The private self snapshot contains `care.tinyEggShop`:

```json
{
  "enabled": true,
  "rulesVersion": "adapted-tiny-owned-eggs-v1",
  "epoch": 500000,
  "serverNowMs": 1800000000000,
  "rotatesAt": 1800003600000,
  "rotationMs": 3600000,
  "maxHeldEggs": 1,
  "offers": [{ "eggId": 0, "name": "Native Egg 0", "price": 0, "source": "native-a2ne" }],
  "eligibility": { "canPurchase": true, "reason": null }
}
```

The offer is an example, not a fixed item. Use its actual `eggId`, price and
epoch from the server. Purchase through the existing strict care transaction:

```json
{
  "type": "care",
  "requestId": "00000000-0000-4000-8000-000000000001",
  "expectedRevision": 1,
  "intent": { "kind": "tiny_egg_purchase", "eggId": 0, "stockEpoch": 500000 }
}
```

Its durable `care-result.effects` contains actual signed `rings`,
`egg:{id,nativeEggId,purchasedAt,stockEpoch,price}` and `rulesVersion`.
The egg's `id` is a new stable server UUID, distinct from the native egg ID.

Hatch that owned egg with:

```json
{
  "type": "care",
  "requestId": "00000000-0000-4000-8000-000000000002",
  "expectedRevision": 2,
  "intent": { "kind": "tiny_egg_hatch", "ownedEggId": "00000000-0000-4000-8000-000000000003" }
}
```

Its durable effects are `hatchedEggId`, the new public `chao` summary and
`rulesVersion`. The summary has a stable `id`, adapted display `name`,
`active:false`, `nativeEggId`, `hatchedAt`, five native `stats` entries
(`grade`, `level`, `points`, `experience`), `nativeNeeds:{mood,belly,units:
"native-0-to-200"}`, `source:"native-a2ne-initializer"`, `playable:false` and
`appearanceMapping:"unverified"`. Native mood/belly 100 is not silently
relabeled as the active server Chao's 0..100 needs. No hidden native skill is
invented where its mapping is unverified.

`care.tinyCollection` contains `enabled:true`, `rulesVersion`,
`activeChaoId` (the original profile ID), `pendingEgg` (that public egg or null),
`chao` (active canonical summary followed by owned hatchlings), `maxChao:8`,
`maxHeldEggs:1`, `eligibility:{canHatch,reason}` and
`selection:{enabled:false,reason:"per-chao-activity-integration-required"}`.

Only the current owner receives this collection. Public other-player records
do not expose it. Native body bytes, their integrity hashes, hatch/stock RNG
state, internal provenance receipts and credentials stay out of snapshots and
care-result effects. Local private profile storage retains a typed collection
with the complete 2136-byte native record per newborn and matching durable
purchase/hatch provenance.

## Authority, recovery and limits

Both operations require Tiny entry, an online controlling connection and a
safe ordinary garden with no minigame, race, Karate, mech, AFK job or activity
lock. A widget observer cannot mutate. The server rejects stale stock epochs,
unoffered source IDs, wrong owned UUIDs, a second held egg, full collections,
insufficient rings, backward trusted time and surplus client fields. Clients
cannot submit prices, RNG, native records, target owners, stats or rewards.

All changes join the existing care revision, UUID receipt and atomic profile
write. A failed write restores wallet, held egg, RNG, body, collection,
receipts, history, dirty flag and events. Exact retries return the committed
result; changed UUID payloads fail. Multiple sockets serialize under the same
mutex. Purchase reserves a final hatch receipt/revision slot, including when
AFK training later needs its own close slot. Accepted buy and hatch entries
also appear once in the bounded actual `care.history`.

Restore checks version, owner identity, capacity, unique IDs, native body
length/hash/default fields, egg provenance, actual purchase/hatch receipts and
reserved closing capacity. Malformed data fails without rewriting the file.
Unknown outer profile/stat/gear metadata remains intact; the typed collection
is a complete canonical replacement so a consumed egg cannot reappear through
an opaque JSON merge.

## Executed evidence

- Fifteen scoped host tests passed with the explicitly configured owned source:
  all twelve native prices/stock, stable hours/reconnect, single slot,
  offline/restart preservation, explicit hatch, repeated same-hour purchase,
  eight-Chao capacity, exact UUID/revision concurrency, save failure/RNG rollback,
  activity/time guards, receipt reservation, missing-source recovery, corrupt
  saved state and active admin/credential/gear/unknown-metadata preservation.
- A real configured-native WebSocket test passed with two owner sockets,
  durable purchase/hatch acknowledgements, matching disk and private snapshot,
  a guest's separate collection and restart replay without another Chao.
- Full server suite: 156 passed, 16 explicit owned-source cases ignored by
  default. The configured-source egg/transport/fruit run passed all 25 cases.
  Format and strict all-target Clippy checks passed.
- Default tests keep native positives opt-in rather than embedding original
  source tables or templates. The native fixtures used disposable stores; no
  live save, preview or release binary was changed.

This source delivery does not enable active selection, additional 3D residents,
cross-game transfer, decoded native names, inferred jewel genetics or a second
playable character. Browser acceptance and an explicit runtime rollout are
separate from the completed Rust transaction and collection evidence.
