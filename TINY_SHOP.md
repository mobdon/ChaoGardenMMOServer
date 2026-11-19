# Rust Tiny shop and automatic residency

November 18, 2026. This isolated `codex/tiny-chao-shop` slice uses the existing
canonical Rust care transaction, shared wallet/inventory, revisions and UUID
receipts. It does not replace the live server, migrate its saves or implement
all original Tiny Garden systems.

The next isolated Rust slice connects durable AFK jobs and actual bounded care
history through this same canonical transaction; see [TRAINING.md](TRAINING.md)
for its exact wire, trusted time, atomic closure and transport evidence.
The native egg shop, single durable held egg and additive owned collection are
specified in [TINY_EGGS.md](TINY_EGGS.md), with explicit hourly rotation and
online hatching through this same care transaction.

## Actual service contract

`GET /api/status` advertises `capabilities.careMutations:true`,
`tinyGarden.autoResident:true`, `tinyGarden.observerSockets:true` and
`tinyGarden.shop`. The shop contains `rulesVersion:"adapted-tiny-shop-v1"`,
`enabled:true`, `items` and `nativeCatalog`. The default offer is adapted Garden
Fruit (`garden-fruit`, three rings). Without verified local source,
`nativeCatalog` is `{enabled:false,reason:"native-data-unavailable"}`; original
fruit rows are not fabricated.

The authenticated self snapshot exposes `care.tinyShop` with that catalog and
`eligibility:{canPurchase,reason}`. Reasons are `null`, `activity-conflict`,
`receipt-capacity`, `insufficient-rings` or `inventory-full`. Eligibility means
at least one currently affordable offer fits its stack; the server rechecks the
selected item, its price and capacity on every new request. Catalog item fields
are `itemId`, `name`, `price`, `enabled`, `maxQuantity`. Verified original fruit
also includes `source:"native-a2ne"`, `effectRules` and signed `rawEffects`.
Clients must render the supplied list rather than assume a fixed item count.

Buy exactly one item using the existing care envelope:

```json
{"type":"care","requestId":"00000000-0000-4000-8000-000000000001","expectedRevision":0,"intent":{"kind":"tiny_purchase","itemId":"garden-fruit"}}
```

Feed exactly one owned item with `intent:{kind:"consume",itemId:"…"}`. All
prices, needs, effects, quantities, IDs, target identity and trusted time are
server decisions; surplus fields are rejected. Both use the existing durable
`care-result` acknowledgement. Retry the exact UUID, revision and payload after
a lost acknowledgement. A retry never charges or feeds twice. A changed
payload under the same UUID is rejected. Stacks are bounded at 99. Failed saves
restore the entire canonical player, wallet, inventory, revision, receipt,
dirty flag and events. Unknown profile/stat/gear metadata remains preserved.

Only `tiny_purchase` omits desktop proximity. The ordinary `purchase` still
requires the existing hub shop or garden grove. New Tiny purchases and care
require hub/garden/Hero/Dark safety and reject active minigames, pending results,
races, Karate, vehicles and the server training/activity lock. Manual Watch
still conflicts; automatic Tiny residency explicitly allows care and shopping.

## Automatic play and passive widgets

On the full Tiny app's first authenticated snapshot, send this once if
`tinyGarden.autoResident` is advertised and `care.tinyResident.enabled` is
false:

```json
{"type":"care","requestId":"00000000-0000-4000-8000-000000000002","expectedRevision":0,"intent":{"kind":"tiny_enter"}}
```

The existing atomic care save commits this explicit opt-in. Do not repeatedly
toggle Watch or optimistically mark the choice saved. The snapshot's
`care.tinyResident` has `enabled`, `active` and a `mode` of `manual`, `resident`,
`minigame`, `saving` or `paused`. The persisted care field is a boolean with a
default of false for legacy profiles. Merely connecting never opts in.

The opted-in Chao follows the server's existing custom garden routine online
and offline while the service runs. A valid `tiny-start` suspends that routine;
`tiny-stop`, a durably committed terminal result or the last controlling socket
disconnect automatically resumes it. A pending reward or training/activity
lock keeps it paused. There is no concurrent routine/minigame authority and no
fabricated progress during server downtime. The existing session-checked
desktop `watch-stop` explicitly returns to manual control and clears the Tiny
preference. Tiny UI does not require that step to play its minigames.

An authenticated widget uses `/ws?token=…&observe=1`. It receives private self
snapshots/events tagged `connectionMode:"observe"`, never counts as a
controlling connection, and cannot send any gameplay/care/save mutation. Such
messages receive `code:"observer-read-only"`. This prevents an open widget
from keeping a closed phone's unfinished game active. Full Tiny retains the
existing control socket (`connectionMode:"control"`). This additive legacy
transport mode is not a new account/device lease or secure-pairing protocol;
that coordinated work remains separate. Do not put credentials into shared
links or logs.

## Original seven fruit rows

Set `CHAO_TINY_GARDEN_PAYLOAD` to the user's local decoded A2NE Tiny Garden
payload. `Config::tiny_garden_payload` supplies the same explicit path to test
hosts. Missing configuration disables original fruit; configured invalid data
fails startup instead of substituting guesses. The read-only Rust core checks
exact length 191,340 and SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`
before source-specific offsets. It then bounds and hashes both tables:

| Table | Offset | Bytes | SHA-256 |
| --- | --- | --- | --- |
| Seven signed seven-channel rows | `0x12510` | 49 | `b33ae558a366ac031e7326d7ade4d3a38f3818f21c6139f6a97a8ceeaa3c49bd` |
| Seven little-endian prices | `0x12548` | 28 | `e9eaed988f3156a9b8c28f72dc48adac7c1735915afd280c7a2406529cc2dc19` |

The source/visual mapping established these original fruit identities:

| Inventory ID | Fruit | Original price |
| --- | --- | --- |
| `tiny-fruit:0` | Orange | 30 |
| `tiny-fruit:1` | Blue | 60 |
| `tiny-fruit:2` | Pink | 55 |
| `tiny-fruit:3` | Green | 50 |
| `tiny-fruit:4` | Purple | 30 |
| `tiny-fruit:5` | Yellow | 55 |
| `tiny-fruit:6` | Red | 70 |

Original decoded rows and sprite outputs remain local; no table or artwork is
embedded in source fixtures. Native row fields are Mood, Belly, Swim, Fly,
Run, Power, Stamina. The native application multiplies the first two by 20 in
a 0–200 range and the five skills by 10 in experience bars with native level-up
behavior. The shared MMO adapter is explicitly
`adapted-tiny-fruit-care-v1`: it normalizes needs to the existing 0–100 range
(raw ×10), applies signed five-skill deltas as raw ×10 to existing bounded
points, and preserves all levels/grades, Luck, Intelligence, energy, animal
history and owner privilege. Native experience-bar level-up behavior remains
unported. Signed raw rows/application labels remain in the durable receipt;
they are not mixed into SA2 animal/drive raw totals. Existing owned fruit
survives a restart without configured native source, but cannot be purchased
or consumed until the verified source is available again.

## Verification and remaining systems

The current full Rust host suite passes 136 tests, with three explicit local
source tests ignored by default. New coverage includes all safe gardens,
unchanged desktop proximity, funds/99-stack limits, strict fields, activity
conflicts, concurrent requests, rollback, opaque metadata preservation,
once-only buy/feed/restart and automatic-resident lifecycle. Real temporary
WebSocket listeners prove two-device buy/feed/replay plus automatic Tiny
entry, game suspension/stop and phone disconnect while a read-only widget
stays connected. The transport fixture advances the trusted server clock
between distinct actions to respect its existing cooldown.

The reusable core passes 25 tests and its separate owned-payload parity test.
The explicitly enabled host native test buys/feeds all seven actual rows and
proves their prices/effects/14 receipts survive restart. All tests use fresh
temporary profiles. Formatting, strict all-target Clippy and the release build
pass. The tested release executable has SHA-256
`5db5752fa44194d6272a20203a1c9d52f41afea52ba4e62bb16f8d12f025813d`.

Isolated actual-browser QA with a distinct disposable profile confirmed Garden
Fruit purchase at three rings (wallet 20 →17, owned quantity 0 →1), feeding
(quantity 1 →0), and durable acknowledgements. Memory Play while automatic
residency was active suspended the routine, and Stop resumed it without a Take
control step. The canonical/live preview profile and existing server were not
written or restarted. Full original-fruit browser playthrough, physical-device
behavior and the accounts integration remain separate acceptance gates.

Still unported: native experience thresholds/level-up and resulting stat
rolls; original shop stock/rotation and unlock rules; egg purchase/incubation/
hatching; persistent toy ownership/use/learning; weeds and broader native
garden lifecycle/behavior; native save/transfer execution beyond the existing
bounded readers. The seven verified fruit tables plus explicit shared-care
adapter do not establish a full native game-engine or gameplay port. AFK
training service integration remains its existing separate workstream.
