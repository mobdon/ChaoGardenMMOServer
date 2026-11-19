# Original A2NE Chao Bounce

`native_bounce::A2neBounceSource::from_verified_payload` accepts only the exact
191,340-byte decoded A2NE USA Tiny Garden program, SHA-1
`2c7da8e1c9af85986eecf4effae39197cdac9abf`. It reuses the existing source gate;
there is no new cartridge reader or substitute table. `begin_round` starts
the original gameplay scene and `step(NativeButtons, &impl BiosMath)` advances
one native video frame. `view` exposes presentation state; `finished` returns
`Some((won, rings))` only after the original result/return fade. Repeated steps
after completion do not produce a new result.

This translates SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, `chao_garden/asm/code.s`, SHA-256
`36dffd71ff6c21ab959d3c7f59f4798ae23dd49935ad0e4fc62196cfe708c2a2`.
Task allocation/order follows `chao_garden/src/game.c`, SHA-256
`c0270d1e3d44b8045a5a8bd350cebfbc194537df308ed29e02d6eb54eba6d7df`.
No original program, table bytes, graphics, audio, saves or credentials are
included in published tooling.

The retained `tiny-garden-games::bounce` is a separate adaptation. This core
does not silently replace its existing phone protocol or reward transaction.

| Native entry | Ported behavior |
| --- | --- |
| `D294`, `D498`, `D538` | Three lives; 13 by 2 moving targets; four stages; final-stage jewel; stage/restart task initialization. |
| `D958`, `DC6C` | Chao 16.16 position, gravity, wall reflection, original camera scroll and hitbox; miss/clear animations and life transitions. |
| `DD60`, `DCC0`, `DE98`, `DEEC` | Moving target rows, alternating direction/wrap, inclusive collection bounds, once-only native ring addition and collection-burst lifetime. |
| `E614`, `E710`, `E850`, `E964`, `EB14`, `ED00`, `EE7C` | Left/right movement; B speed; A launch/hold; exact compression markers, five contact zones, spring/right-platform launch, boost scale and child-controlled lockout. |
| `E364` | The right spring child changes parent power and movement lockout. Those gameplay writes are retained; it is not treated as a disposable rendering callback. |
| `EFA0`, `F048`, `F11C`, `F4C0` | Native cloud motion; later-stage moving bumpers, collision distance/reflection, recoil and cooldown. |
| `D664`, `D814`, `DFF8`, `F58C`, `F5AC`, `F620` | Pause debounce/selection/resume/quit, native clear/loss/result counters, delayed restart dispatch and 16-frame return fade. |

`NativeBounceActor` and mutable round fields are trusted inspection APIs,
not a client save schema. Failed checked steps preserve the entire candidate
frame, including changes made before a missing BIOS service. The native task
allocator resets fractional positions while preserving integer positions and
unwritten tail fields. Reuse and the extra root-task slot introduced with each
new bumper are represented where those fields affect gameplay/visible actors.

Controls use the original masks: A=1, B=2, Start=8, Right=16, Left=32,
Up=64, Down=128. `pressed` is the native edge/repeat mask and `held` is the
held mask; hosts must derive them on the authority's frame clock. Clients
must not set elapsed time, positions, score, target state or completion.

The original target awards total at most 113 rings: 26 in each of the first
three stages and 35 in the last (one ordinary target becomes a ten-ring
jewel). Native score is a wrapping 16-bit field; the service must validate
the legal round result against its separate economy policy. `finished` is
a core return boundary and never acknowledges a saved MMO reward. The host
still needs its existing atomic canonical-wallet/once-only receipt transaction.

## Native assets and presentation boundary

Source offsets below are in the verified decoded program:

| Asset | Native mapping |
| --- | --- |
| BG palette | Raw `17524`, 0x140 bytes, destination `05000000`. |
| BG tiles | LZ `17664` to `06000040`; LZ `188D8` to `06008000`. |
| BG map | LZ `18624`, then the original 24-column by 40-row copy to `0600D000`. |
| OBJ palette | Raw `15EE0`, 0xE0 bytes, destination `05000200`. |
| OBJ tiles | LZ `15FC0` to `06010000`; common HUD LZ `199D0` to `06017EE0`. |
| Sprite descriptors | Eight-byte records `2E4C8` through `2E560` for Chao, targets, spring parts, bumpers, clouds and HUD. |
| Timing/geometry | Trig at `1FB74`; target burst tiles `2EB5A`; spring walk tiles `2EB60`; spring contact heights `2EB64`. |

`BounceActorView` carries source descriptor offset, original tile and packed
sprite style/transform/palette inputs. The root spring record is a mechanical
state, not a directly drawable sprite. It has several native child parts and
an always-bottom duplicate when the camera follows the Chao. A renderer must
use the original child descriptors and composition rather than treating its
compression angle as an OBJ tile. Native graphics/audio output and the full
composite spring presentation remain adapter work; the source-backed pose
inventory emitted by the private verifier supports that work.

The shared `native_bios_math::ReferenceBiosMath` is an integer numerical
reference checked against pinned mGBA 0.10.3 C. It does not claim authentic GBA
BIOS execution/cycle parity. `RejectBiosMath` fails closed on missing Sqrt or
ArcTan2. The original A2NE trig table is retained and is not replaced with
floating-point trigonometry or another game's table.

## Validation

Rust tests cover the native allocator reset boundary, ten-ring once-only
collection, exact result dismissal/fade completion, first-miss restart rather
than game end, right-spring child lockout and atomic BIOS failure. An optional
owned-payload test exercises a complete local round and immutable completion.

`tools/verify_native_bounce.py` executes the original native `D538` task setup
and `F5AC`/`F620` scene callbacks, including the actual linked task scheduler and
child creation/removal. It compares all source-defined gameplay fields, active
actor presence, fixed-point positions, native visible target tiles, counters,
target kinds, pause/restart/result/return boundaries and original BIOS call
arguments. Task-list IDs, unused offscreen tile garbage, native OAM/affine
output and unrelated Garden actors restored after return are excluded.

The initial complete corpus passes 32 traces and 48,503 native frames across
all four starting stages. Twenty-eight ordinary controller policies cover
launch, held boosts, fast movement, misses/restarts, pause/resume/quit and
bumper collisions. Four controlled empty-target setups exercise all native
clear/next-stage/final-result branches; those setups are explicitly fixtures,
not claims that the test player collected every target. Collection is compared
in ordinary traces and independently tested for once-only jewel credit.

Audio callbacks are skipped. Graphics copy/fill/LZ and render-only affine
outputs are modeled without replacing native gameplay. Every encountered
Sqrt/ArcTan2 scalar vector is independently compared in bulk with a pinned
local C reference executable before a pass can be written. Final counts,
source/probe/verifier/corpus hashes and the local native sprite-pose manifest
are emitted only beneath a new private `.local` output directory.

```powershell
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml
python crates/tiny-garden-core/tools/verify_native_bounce.py `
  --payload C:/private/mb-chao-garden-A2NE.gba `
  --output-dir C:/private/.local/native-bounce-proof-new `
  --bios-reference C:/private/primary-bios-reference.exe `
  --unicorn-dir C:/private/unicorn-packages --frames 2000
```

The verifier rejects a wrong source/executable, missing original return,
unexpected BIOS call, changed projection shape, incomplete output or any
compared mismatch. It never contacts a game server or writes a ROM/save.
