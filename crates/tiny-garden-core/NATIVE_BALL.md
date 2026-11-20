# A3VE Chao Ball

The reusable Rust API is `native_ball::BallSession`. Give it a seed and a
`BallSource::from_verified_payload` obtained from the owner's decoded A3VE Tiny
Garden. Advance one original frame per `step(held_keys)`. A=1, B=2, Start=8,
Right=16, Left=32, Up=64 and Down=128. Read `play.stage.board` (10 by 10),
`play.stage.scroll`, `play.player`, `play.shot`, `play.falling_rings()`,
`screen()` and the round ring count as needed for presentation. The upcoming
feed and RNG are trusted internal game data. Give clients controlled views.

The return to Garden emits `Some((Finish, round_rings))` on one frame. Later
frames return `None`. A service integrating this crate must still commit the
canonical wallet and a once-only receipt atomically before acknowledging the
client. The standalone `cartridge_wallet` models the retail 99,999 wallet cap.
The existing adapted phone Ball protocol and its 100-ring cap need a coordinated
integration decision. Source marker quotas permit at most 13, 29, 35 and 51
collectible rings across the four stages (128 total).

## Version and source map

A3VE revision 0 cartridge SHA-256
`5690eb0a58ac24616fd6747a8aedddc7777f28d6f3579cb4428ea8fa5d30208b`.
The literal at cartridge offset `0x1D19C` names the LZ stream at `0xD0970`.
The 152,552-byte stream expands to 188,716 bytes, SHA-256
`33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb`.
All subsequent offsets are in this **decoded A3VE program**, mapped at
`0x02000000`. The earlier client-side Rust cartridge reader documents
decompression/provenance in `asset-tools/crates/sonic-pinball-party/docs/GARDEN.md`.

| Offset | Native behavior represented |
| --- | --- |
| `0x3A92 / 0x3AAE / 0x3AD4` | Garden object 4 with nonzero secondary field selects the Thumb entry for `0xD294`. The retained cartridge oracle executes this loader by address. |
| `0xD294 / 0xD504 / 0x1534` | Reset round; initialize stage; native RNG. The feed alternates color runs of 1–4 cells. Its last run can extend beyond the playable rows. Read all four quota rows and three marker IDs from the digest-gated payload at `0x2E108` and `0x2E114`. Placement consumes native RNG for each marker. 20, 30, 40 and 50 rows. |
| `0xDFBC / 0xDBA8 / 0xDE24` | Player begins at x=104 / native column 6. Edge-triggered A/B controls choose native colors 2/1, with B first. Held Left has priority over Right. Movement takes eight 2-pixel frames. The child animation timer controls the shot recovery. |
| `0xE134 / 0xE08C` | Flight starts at y=155 and immediately advances four pixels. The collision row includes the signed board scroll and 8-pixel bias. Any occupied cell, including the original ceiling and rings, intercepts a shot. |
| `0xE3B4 / 0xE1F0` | Board descent first waits 30 active frames. Subsequent stage intervals are 116, 96, 76 or 57. Shift the board, then animate its remaining 14 pixels two per frame. Occupied low-nibble cells on native row nine trigger loss; clearing all markers waits for live falling/caught ring actors. |
| `0xE430 / 0xEA5C / 0xE974` | After 15 frames, check seven rays in the payload's order (`0x2E11B`). Flip an opponent run only when enclosed by the shot color. A flip gets a 31-frame interval. |
| `0xED90 / 0xECC8 / 0xEB9C` | Orthogonal scanline fill with the native 64-slot circular queue. A connected component of at least ten of one color clears, followed by a 36-frame effect countdown. |
| `0xEEF8 / 0xEF50 / 0xF010 / 0xF0F0` | Record each column's first gap, lift remaining cells toward the ceiling, repeat in four-pixel increments, and recheck eligible cells for cascades. |
| `0xF2DC / 0xF218 / 0xF194` | Release exposed ring bundles from the bottom up in each column, staggered by three frames. Fall in original fixed point. A catch requires y=136..167 inclusive and horizontal distance strictly less than 12. Successful collection adds exactly one per actor and keeps the round active through its collection animation. |
| `0xD8A4 / 0xF688 / 0xF72C / 0xF778 / 0xF8D0 / 0xF8F8 / 0xF624` | Intro, pause selection and debounce, loss/final result, intervening stage clear, fade, stage generation with continued RNG, and Garden wallet return. |

## Presentation handoff

The native source has matching graphics. The `0xD294` loader reads OBJ
palettes at `0x1606C` and OBJ tiles at `0x1614C`; BG palettes are at
`0x17074` and BG tiles at `0x17214`. The respective LZ outputs are 10,848
and 17,760 bytes. The BG load destination is VRAM + 0x40. During `0xE888`,
each 16-pixel cell uses four consecutive 8-pixel tile entries. Top-left entries
are `0x8205` for the ceiling, `0xA209` and `0x9209` for the two colors, and
`0xB20D`, `0xB211`, `0xB215` for the ring bundles. The high bits select the
original palette bank. The player child reads the native OBJ descriptor at
`0x2DCFC`; native caught ring animation uses tiles `0x143` / `0x147`.
For zero-based board indices the cell top-left is `(16+16*c,16*r-scroll)`. Falling-ring and shot coordinates are pixel anchors (the server converts ring Y from signed 16.16). The player child anchor is `(player.x,159)`; its idle/left two-piece offsets are (-13,-31) and (-5,-31); the right movement/throw pair uses (-12,-31) and (+4,-31). These offsets come from the descriptors selected at DF8E–DF98, not a centered sprite rectangle. The shot at E134 uses descriptor 2DD1C with (-8,-8).

No source tiles, palettes, decoded programs, saves, or audio are included.

## Verification and limits

Run the ordinary Rust tests and the separate original-CPU differential:

```sh
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml
python crates/tiny-garden-core/tools/verify_native_ball.py --payload PRIVATE_DECODED_A3VE --cases 512
```

The differential needs an offline Cargo dependency cache and Unicorn 2.1.4.
An optional `--unicorn-dir` points at a private local module directory. It
rejects a wrong digest or size before executing the version-specific offsets.
It compiles the Rust probe into a temporary directory, checks the emitted
gameplay state against the CPU model, and fails on an incomplete native
return, a wrong ending, a missing Rust output, or a state mismatch.

The operation checks vary all four stages, RNG seeds, player phases, keys,
board flags, ring bounds, flight, flip direction, compound clear/cascade
timelines, scroll and wallet wrap/cap. Eight deterministic stage replays
start from generated boards. They compare board cells, remaining markers,
horizontal position, player phase, column, timer, scroll, board flags and ring
total on every frame through four victories and four losses.

A 512-case run matched 4,608 operation cases and 13 replays. The four win-stage replays ran 4,836, 6,028, 6,835, and 7,359 frames and collected 6, 10, 13, and 11 rings. Full sessions took 70 frames for a pause quit, 1,132 for an early loss, 26,144 for a fourth-stage loss with 42 rings, and 26,826 for a four-stage victory with 51 rings. A second pause-quit trace (seed 26) catches one ring on frame 854, then returns to Garden on frame 877 with that ring. The server fixture replays those buttons using the same source gate and proves the single partial award and receipt survive a repeated save and restart. The late-loss and full-win traces carry the original RNG and rings across all four stages. The full-session digest includes the board, stage and ring count at every frame, including projectiles still active behind a result banner.

Full round checks enter the actual A3VE `0xD294` loader, dispatch its native
frame callback, execute its linked-list task allocator and scheduler, and
compare the return and credited ring count. Their checkpoints cover pause
quit, loss, and cross-stage returns with native RNG continuity. Host shims
supply BIOS arithmetic or omit graphics submission, sound, and the garden
scene backup/restore. The lower-level operation oracle isolates task creation
when inspecting its exact emitted arguments. There is no instruction-level
claim for hardware I/O, art/audio reproduction, or the ROM's surrounding
Garden, title and save engine. The source-gated service adapter, its clock and atomic payout are documented
in `../../PHONE.md`; phone graphics and presentation remain separate from
these CPU gameplay checks.
