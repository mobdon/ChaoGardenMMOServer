# Original A2NE Chao Memory gameplay

The native Memory work is isolated in `src/native_memory.rs`. It translates
the supplied Sonic Advance 2 USA Tiny Garden rules into reusable Rust. It
does not replace the currently connected adapted Memory service until a
reviewed round adapter and matching native presentation are integrated.

The source is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`,
`chao_garden/asm/code.s`, SHA-256
`36dffd71ff6c21ab959d3c7f59f4798ae23dd49935ad0e4fc62196cfe708c2a2`.
Rules are gated to the actual decoded A2NE program, 191,340 bytes, SHA-1
`2c7da8e1c9af85986eecf4effae39197cdac9abf`. A matching algorithm or shared
game name does not establish binary/rule parity with ASOE or A3VE. No original
ROM, art, audio, extracted table or source listing is published in this work.

## Native operations and current adaptation gaps

| Area | Verified A2NE behavior | Current connected adaptation |
| --- | --- | --- |
| Board | Six columns, **five rows**, 30 cells; 14 distinct native card IDs whose adjacent ID pairs match. `sub_0200A554`. | Six columns, four rows, 24 cells with duplicate face IDs. |
| Placement | Two exact native RNG draws per attempt, shifted right seven before row modulo 5 / column modulo 6; occupied positions retry both draws. | Generic custom-RNG shuffle of an array. |
| Initial reveal | 270 video frames: 60-frame waits alternating with 30-frame center/middle/outer flips. Main state4 at `0x0200B078`. | All faces visible for 2,000ms, then hidden together. |
| Chao shuffle | Original perimeter entry selection, direction and segment rolls, 15 tile traversal steps, 32-frame walking segments, eligible pushes into empty neighbors. Main state5 at `0x0200B154`. | Five random adjacent swaps at 300ms intervals. |
| Trip | Once, when walking horizontally; native frame sequence freezes the walk and existing moving-card updates during the trip branch. | A 300ms reveal and shuffle timer that continues to advance. |
| Ring zones | Eighteen outer cells worth 1, ten middle worth 3 and two center worth 5. | Four-row approximation of the zones. |
| Selection | Native cursor wraps across6×5. The first card may still be flipping when the second is selected; each flip takes30 video frames. Matching phase `0x0200B6A4`. | Selected faces appear immediately. |
| Mismatch | 84-frame display, then reverse flip; three mistakes lose. The original final reverse update leaves selected flip bytes at-1. | 600ms display before clearing the pair. |
| Match | Remove the pair at the start of the native match animation; seventeen-frame transform, twenty-one-frame travel, then ring addition and pair completion. | Immediate match, score addition and marking. |
| Score | Native round score caps 99; seven pairs clear. Clear additions are 30+20+10, 20+10 or 10 for 0, 1 or 2 mistakes. | Same clear-bonus totals but different timing; server credit cap 100 is a separate MMO economy policy. |
| Results | Original win/loss UI counters, delayed bonuses, 180-frame wait or A/Start dismissal, and 17-frame fade. The 21-frame match travel counter carries into the win intro. | Immediate result handling with custom timing. |

Original source facts are distinguished from the old adaptation; the latter
must not be labeled a completed native port merely because it runs in Rust.

## Reusable API boundary

`A2neMemorySource::from_verified_payload` reuses the existing exact A2NE
fingerprint gate. Its board constructor is deterministic and commits the
caller-owned RNG only after all 14 placements succeed. A 4,096-draw safety
budget rejects pathological exhaustion without altering the seed; it does
not replace native roll probabilities or collision behavior.

`NativeMemoryReveal`, `NativeMemoryShuffle`, `NativeMemoryMatching` and
`NativeMemoryResult` use
original video-frame counters. The matching API preserves original substates
0..8, validates imported fields and handles the native delayed scoring and
win/loss transitions. Raw RAM mapping is explicitly an inspection/proof API,
not a network contract or trusted client save import.

`begin_round` assembles those phases into `NativeMemoryRound`, starting after
the original instruction screen. The round owns a private RNG and private
board; `step` accepts only native button state and advances one original video
frame. Failed steps preserve the entire round. `view` exposes occupied cells,
legally revealed paired faces, flips, Chao/card positions, cursor and visible
score. It omits native IDs, RNG/seed and future choices. Debug output is also
redacted. `finished` remains empty until the original result wait/fade reaches
the native wallet-write boundary. It returns a core result, never a durable
MMO reward acknowledgment.

The board and selection IDs are **private gameplay information**. These types
do not derive serialization. The service must publish only occupied cells,
legally visible faces, native flip/move presentation and player-facing stats.
Seeds, unseen IDs and future shuffle choices must not enter client snapshots.

The current gameplay slice excludes instruction/pause presentation, native
audio/rendering, cursor/score-display interpolation and durable wallet/receipt
integration. Result counters and bonuses are implemented; their graphics and
audio are not. An unsupported Start/pause input during gameplay fails closed;
A/Start dismissal during results uses the original timing.
Native results must still pass through the existing server-owned once-only
reward transaction; a core win is never a durable-save acknowledgment.

## Validation

Synthetic Rust tests cover the observed native seed-zero board, rejection
draw consumption, all ring zones, delayed matching, reverse flips, three
mistakes, clear bonuses, cursor wrapping, raw-field preservation, full reveal
timing and deterministic traversal/card preservation over seeded rounds. A
complete-round test memorizes only the public initial peek, follows visible
pushes, moves the cursor with button inputs, clears all seven pairs and waits
for the original result boundary. Invalid matching/shuffle/result state and
unsupported gameplay pause fail atomically.

The independent optional `tools/verify_native_memory.py` compiles a local Rust
harness and executes the actual original Thumb routines. Board parity compares
all 30 cells, RNG seed and draw count. Matching parity compares the board,
cursor, selection records, native state/timer, mistakes, pairs and score against
one real original frame. Traversal/reveal checks exercise the original main
dispatcher. Audio calls are skipped; two exact BIOS graphics-copy primitives
are modeled. Native RNG and software arithmetic execute as original code.
The final local corpus passed 4,101 boards and shuffle initializations, 4,096
matching frame cases, 4,096 traversal frame cases, 32 complete 270-frame reveal
traces and 48 complete result traces comprising 14,190 original result frames.
The separate `tools/native_memory_round_probe.rs` also plays solely from the
public peek, visible pushes and button inputs. Forty complete rounds compare
79,755 actual native frames through result completion: phase, score,
mistakes/pairs, all 30 occupied cells and legally visible faces, matching
cursor, Chao and pushed-card positions. The corpus covers ordinary wins,
horizontal trips, one/two mistake wins and third-mistake losses. This proves
phase assembly and the compared public view; it does not prove pause,
uncompared graphics/audio, an MMO adapter or durable rewards.

```powershell
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --test native_memory
cargo clippy --manifest-path crates/tiny-garden-core/Cargo.toml --test native_memory -- -D warnings
python crates/tiny-garden-core/tools/verify_native_memory.py `
  --payload C:/path/to/private/mb-chao-garden-A2NE.gba `
  --output-dir C:/path/to/private/.local/native-memory-proof-new `
  --unicorn-dir C:/path/to/private/.local/python-packages
```

The verifier requires the exact private payload, pinned Unicorn 2.1.4, Cargo
and a new output directory beneath `.local`. Unexpected instructions/BIOS
calls, non-return, output length or any compared value mismatch fail the run.
It never contacts a game service or modifies a supplied ROM/save. Final proof
counts and source/verifier/corpus fingerprints are recorded in the local JSON
result rather than embedding private native data here.
