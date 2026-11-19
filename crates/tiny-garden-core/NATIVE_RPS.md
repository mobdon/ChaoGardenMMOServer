# Original ASOE Janken clock and target waves

November 18, 2026. `native_rps.rs` ports a bounded original Sonic Advance Tiny
Chao Garden rock-paper-scissors/Janken subsystem into Rust. This is genuine
source behavior, not the existing MMO-adapted five-game rules. It is **not yet
the complete native minigame or a connected service**.

The supplied decoded ASOE payload is 138,752 bytes, SHA-1
`e2a1beec8519d36eae6478cbf3732942ac692023`, SHA-256
`989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0`.
No original payload, assets or save data is bundled. The existing client
`asset-tools/crates/sonic-advance` reader owns cartridge/decompression work;
this module adds no second reader and must not reuse the A2NE shop gate for
ASOE. The opt-in native oracle independently gates the exact decoded payload.

The ASOE garden selector at `0200360E..0200362A` reads object byte1A: nonzero
selects `0200B4E0` (Janken); zero selects `020090E8` (Memory). These are source
specific addresses, not inferred from A2NE. ASOE `02001114` uses exactly the
LCG already implemented by `native_growth::NativeRng`; its seed is `03000840`.

| Original address | Rust scope |
| --- | --- |
| `0200A77C` | Set clock seconds byte and reset subframes. |
| `0200A798` | Add to signed seconds, cap above98 at99:00; otherwise keep subframes. |
| `0200A7C0` | Exact byte/signed countdown and expiry return value. |
| `0200A800` | Advance wave; initialize ten target state/gesture/route fields; ten RNG draws; first30-second clock or later10-second bonus. |
| `0200A8C8` | Set waveFF, speed2, then initialize the first wave. |
| `0200A8EC` | Ten target counters; rectangular route; original state2/state3 removal timing, movement window, score and next-wave/speed decisions. |
| `02008128` | Minigame round-ring byte addition and signed cap. |

Native target records are ten twelve-byte rows at `03000E78`; wave/speed/
active/unhit bytes are `03000EF0..03000EF3`. Clock seconds/subframes are
`03000E75/03000E76`; round rings are `03000DE0`. All twelve bytes of each
record survive. A new wave changes state, gesture and route only; the original
counter, x/y, velocities and opaque byte3 are deliberately preserved.

The route advances using signed speed, hides negative positions without
changing their x/y, and subtracts392 only once when positive position exceeds
392. Original corner coordinates and the exact signed16 wrap are retained.
State2 defeated targets move with native signed byte velocities inside the
inclusive x[-32,239]/y[-32,159] window, including their removal update. After
the old counter exceeds29 they are removed and add signed-wave/2+1 round
rings. State3 ties remove at the same time without score; their original
eight-update blink decisions are exposed. Even inactive records increment
their byte counter.

The port exposes original sound/render decision masks without substituting
handmade assets/effects. Masks describe decisions, not final OAM words:
removal precedes state3 blink; next-wave initialization occurs last and hides
all records. An adapter must retain this order. Original state2 palette
transitions, sprite tile/palette/OAM projection and playback remain outside
this pure slice. Numeric gesture IDs remain unlabeled pending original art
mapping. No real-time or durable progression policy is introduced.

The clock has the native boundary: starting30:00, update1800 reaches00:00;
update1801 returns expired and remains00:00. Normalization to a different
timer would change the source behavior. The host must call clock and target
callbacks separately in the traced controller order, not decrement time once
per network packet. Round rings are a private native score; this module does
not mutate the canonical wallet or produce a trusted reward receipt.

Nine focused Rust tests cover clock/bonus boundaries, original RNG count,
unwritten record preservation, route corners/wrap, removal timing, flight
window, signed counters and round-score wrapping. Verification:

```powershell
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --test native_rps
python crates/tiny-garden-core/tools/verify_native_rps.py `
  --payload <owned-decoded-ASOE.gba> `
  --output-dir <new-path-under-.local> `
  --unicorn-dir <local-unicorn-2.1.4-directory>
```

The recorded actual Thumb parity run passed7,234 cases, including4,096
randomized cases, comparing140 bytes per case: clock, all120 target bytes,
wave metadata, round score, RNG state/count, expiry return and appearance/
award sound counts. It executes the seven original functions above, including
the native RNG and software division. Only the out-of-scope display callback
is skipped, and the sound callback is observed/skipped. No mocked native
route, timer, RNG, removal or ring helper replaces the source instructions.
OAM is mapped so the original writes execute, but its values are not compared.
This proves bounded state/sound-count parity, not rendering or task scheduling.
Ignored evidence: client `.local/reviews/native-rps-thumb-first/result.json`.

Remaining full-game work is explicit:

- Port original cannon/input callback `0200A528` after tracing its fields.
- Port projectile creation/shot accounting `0200ABE8` and update/collision
  `0200ACCC`; retain original card gestures and collision table `02011F84`.
- Port native reflection/defeat velocity through `0200DB00` (BIOS ArcTan2)
  and `02007FB4` trig. A floating-point replacement is not native parity.
- Trace original gesture sprite table `02011F78`, effects `0200B3EC`, complete
  controller transitions, clock/shot finish rules and actual native art/UI.
- Test complete native rounds before choosing a separately labeled MMO
  durable once-only reward policy and Rust/WASM/browser service integration.

`unhit_targets` is presently an input from trusted source state: the missing
native collision callback owns its decrements. It must never accept a client
claim that a target was hit. No production service, save or runtime rollout
was changed by this port.
