# Native ASOE Janken active and finish rules

November 18, 2026. `native_rps_game`, `native_rps_finish` and the earlier
`native_rps` clock/waves implement the original ASOE active round through its
native result exit. This replaces no live adapted game by itself. Original
entry scene loading, pause menu, OAM/art/audio rendering and durable MMO
service transactions remain separate. The matching source is **ASOE**, not
the A2NE shop/Memory/Bounce payload.

The exact private decoded ASOE is138,752 bytes, SHA-256
`989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0`.
`NativeRpsAssets::from_verified_payload` validates this identity plus both
tables:64 signed16 cosine samples at0x11C5E (128 bytes, SHA-256
`647187a05a2e9ffb2711d0389755d07090e7ac9c4e2785526ddc8519315da0e2`)
and47 signed collision half-width bytes at0x11F84 (SHA-256
`80f750d2fa5d848bc87e77a92bb55cd58581e64dae5ae5d69bc421b015e850fe`).
No native table samples or ROM bytes are distributed. This is a checked
decoded-resource gate, not another cartridge or compression reader.

| Native source | Ported behavior |
| --- | --- |
| `0200A4E4/0200A528` | Actor x/countdown/mode; original autonomous movement/animation decisions and shared RNG draws, including repeated equal-goal rejection. |
| `0200ABE8` | Initialize3 cards, source-generated gestures, y116, shots5; preserve other raw record bytes. |
| `0200ACCC` | All10 projectile states; ascent, original source collision shape/strict edges and target order; victory/tie/reflection, signed velocities, shot loss, return/refresh delays and original biased gesture refresh RNG. |
| `0200B3B4/0200B3EC` | Three launch lanes, source marker easing, native left-before-right and fire-only-idle input. |
| `0200AB84` | Shot counter's original display twelfths at two updates per step. |
| `02007FB4` | Native integer quantization/quarter-table trig, including its exact endpoint/reflection behavior. |
| `0200B4E0`, phase2 | Actor→targets→projectiles→input→finish/clock, followed by shot-display common tail. |
| `02008744/020087CC`, modes2/4/5 | Complete result transition, wait and fade; targets keep updating until native return. |

Numeric gesture IDs remain0..2 pending original sprite/label mapping. The
client does **not** choose its projectile gesture. It selects a lane and fires
that lane's source-generated card. Native difference `card+2-target` gives
victory at1/4, tie at2, reflection otherwise. Winning and tying consume a
target; only defeated-target removal awards native round rings. Merely
pressing fire does not spend a shot: missed or reflected cards lose one when
their original lifecycle says so. Original refresh can choose a gesture that
defeats a remaining idle target; its exact source RNG count/order is retained.

The active round ends immediately when shots become0. Otherwise the clock
updates; expiration ends only while signed unhit-target count is positive.
Already-hit targets can finish and advance the wave even at00:00. A finish
signal is not the final score. Native mode2 runs63 updates, mode4 waits for
A/Start or180 updates, and mode5 runs17 updates. All these finish updates
continue the original target callback; the score freezes only on the final
exit. This takes81 finish updates with immediate A or260 automatically.

`native_bios_math` is the one shared numerical boundary for these Tiny ports:
`BiosMath`, `RejectBiosMath`, `ReferenceBiosMath` and `MathError`. It provides
signed16 ArcTan2 and a16-step floor integer sqrt. Reference ArcTan2 uses the
documented integer polynomial/quadrants already independently used by the
Pinball port; no floating point is used. The independent primary C reference
is pinned mGBA0.10.3 `src/gba/bios.c`, SHA-256
`8885962e1ca2d8c79a7f13a5d1e074bb2b39704a64aa164f76e1506868c8759a`.
This is **numerical reference parity**, not authentic Nintendo BIOS machine
code, register clobber, cycle timing or whole-hardware parity. Callers may use
the rejecting implementation when that boundary is unavailable. An error
commits neither state nor RNG for the failed tick.

One original undefined boundary is deliberately rejected: refresh selects
`roll / ceil(32767/eligible_count)`. With counts1 or7, roll32767 can address
one byte past the initialized native stack list. `NativeScratchRead` rejects
and rolls back that update; no uninitialized stack byte or invented target
is substituted. A fixed native seed858717501 test reaches this boundary.
Autonomous actor equal-goal rejection retains the native RNG sequence with a
128-draw host budget; exhaustion fails without partial mutation. These are
explicit bounded-host deviations, not silently changed native rules.

Server adapter API:

```rust
let assets = NativeRpsAssets::from_verified_payload(&owned_asoe)?;
let mut rng = NativeRng::new(private_server_seed);
let mut session = NativeRpsSession::default();
session.initialize(&mut rng); // exactly13 source draws
let event = session.tick(keys, &assets, &ReferenceBiosMath, &mut rng)?;
// session.result: Option<u8> becomes Some only after native finish exit.
```

Each call is one eligible native update, not one network input packet. Bounded
input uses native event bits: left0x20, right0x10, A/fire0x01. Start0x08 is
accepted only during result wait; active pause returns `UnsupportedPause`
until the actual pause menu is ported. The server owns the update schedule,
RNG, tables, card gestures, collisions, score and result timing. This private
native score is not a client-uploadable gain or an automatically durable MMO
wallet credit. Integrate it through the agreed existing transactional Tiny
result/receipt path, preserving the canonical Chao and prior adapted games.

Presentation must use the original card/target assets and palettes. Targets'
stored x/y are live. Active card state1 uses x=`12+64*lane_index` even though
its raw x may be stale; reflected state5 uses stored x/y. Original affine
return/refresh states3/4/7/8 have separate OAM projection, not a new gameplay
trajectory. Raw records alone must not be mistaken for complete native OAM.
The autonomous actor's native baseline is y47. Public render snapshots must
omit RNG/private scratch and identify any presentation attachment adaptation.

The focused tests cover all nine numeric gesture pairs, input ordering,
original timing boundaries, shot loss, display timing, BIOS failure rollback,
source identity, the bounded scratch rejection, full finish timing and delayed
final score. An opt-in test reads the actual owner ASOE and verifies its gate.
Final exact native/controller proof passed7,279 cases, including4,096 random
cases,128 active-round traces and32 complete original `0200B4E0` rounds from
phase2 through the native finish/cashout exit. Each compares194 bytes: all
target/card records, clock/wave/shot/actor state, round rings, RNG state/count,
gameplay sound counts and bounded/complete status. The oracle's logical
`finishing` projection includes terminal completion even though the original
exit resets its phase byte before switching the main callback to the garden.
All other compared source fields are retained without this projection.

The original callbacks execute, including native integer division/trig,
controller order and result transition. Display/actor drawing/common blink,
graphics CpuSet, sounds and final music fade are observed or skipped at
explicit out-of-scope boundaries. There are725 native SWI0A calls supplied
by the independent pinned C numerical reference. Rust scalar math separately
matches4,225 signed atan vectors and512 sqrt vectors against that executable;
the floor-sqrt test also checks every16-bit root and adjacent boundaries.
The forced uninitialized scratch edge is detected and rolled back in both
the Rust operation and the explicitly guarded native harness.

Ignored proof: client
`.local/reviews/native-rps-full-controller-final/result.json`.
Recorded source SHA-256 values:

- Game: `e09b47c21bd062c3a96bab0f4edc1d1b69c59eae3d5430b036be327bf39963a9`.
- Finish: `f8612f4337594c653d4cccf9b63a0ca20c191d77c6351d5d23df5d2e30dc68c5`.
- Shared math: `d560298fcd2ce0a9b14591d42e53a71c9ad527d28065df36c4def244b2fc954a`.

Nine ordinary game tests plus the explicit real-ASOE test pass; four finish
tests, the earlier nine wave tests, shared math tests and strict all-target
core Clippy pass. No live preview, durable profile or runtime was changed.
