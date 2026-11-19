# Native A2NE shop state slice

`src/native_shop_state.rs` is reusable pure Rust for the original Tiny shop's stock selection, purchase-to-held-item transition, eight-fruit capacity, saved toy presence and television unlock countdown. It reuses `native_growth::NativeRng` and the existing `shop::TinyShopCatalog` source gate. This delivery does not change the live service, export a new mutation endpoint, deploy a runtime or authorize imported native saves.

The source is the owner's supplied A2NE USA revision 0 decoded garden payload, 191,340 bytes, SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`. Reference assembly/C: SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, `chao_garden/asm/code.s` and `chao_garden/src/game.c`. Addresses below use the native load base `0x02000000`; data offsets are relative to the decoded payload. No ROM, save or converted native art is embedded in these sources.

## Implemented behavior

| Native source | Rust behavior |
| --- | --- |
| `02007688..020076A8` | Countdown at save+0x14 decrements once per eligible shop-task update, saturating at zero; global flag0x10 pauses it, while flag0x80 does not. Host supplies task-update counts, never client elapsed time. |
| `0200773E..02007786` | First stock build initializes save+0x1C from native frame counter modulo5; seven fruit subjects are created. The host must first pass the native panel/open-button/held-item gates outside this bounded stock function. |
| `02007786..020078CC` | An egg is offered only when save+0x59 is FF and runtime03001501 is zero. Raw save+0x58 equalFF forces normal egg0 without RNG or caching. Otherwise the session reuses runtime03001530 or performs the original weighted draws/cycle rotation, preserving exact inclusive thresholds and RNG call order. |
| `020078CC..0200793C` | Offer the first missing toy in order0,1,2, using the original saved x-coordinate as presence. Toy2 also requires countdown zero. Stock rebuilds only when the shop opens. Save+0xCE becomes1. |
| `02007978..02007AFA` | A fruit purchase requires enough rings and fewer than8 saved fruit IDs. It fills the first FF slot and converts that displayed subject to a held item. That same row cannot be bought again until a new opening. |
| `02007B00..02007C3E` | Egg purchase writes the offered native ID at save+0x59 and subtracts its exact verified price. |
| `02007C4C..02007D92` | Toy purchase subtracts its price; buying ID1 sets countdown0x34BC0 (216000 native task updates). |
| `02007DA0`, `02008010`, `020080E0` | Immediate held-item positioning saves cursor x and cursor y+0x13, retaining original byte stores. Toy1 also clears save+0xCF. Further drop/use/food/hatching behavior is outside this slice. |

Original purchase inputs preserve the panel/cursor/modal/holding gates, global flags0x10/0x80, new-A press and no held direction bits0xF0. The row-hit interval has its original inclusive upper and exclusive lower comparison. Unknown raw fields are retained by name rather than assigned unverified meanings. `read_raw_fields`/`write_raw_fields` touch only mapped fields and preserve opaque bytes. The native runtime wallet03005274 is a caller-owned value; no unverified wallet save offset is invented.

Original native ownership here means a garden's item/toy state. It does not replace canonical MMO account/Chao ownership. The authoritative host must supply its own state, RNG and clock, authenticate bounded intents and commit wallet/items/result once in a durable transaction. The existing live shop still uses its established reviewed contract; this new native module is not integrated merely because it exists.

## Verified source prices

`NativeShopPrices::from_verified_payload` first calls the existing full A2NE payload and fruit-table gate. Fruit prices remain the existing reader's seven rows at0x12548. It then reads exactly twelve little-endian egg prices at0x12564 (48 bytes; SHA-256 `53168556e91578a45777528ddd9f9ce8ed0ae63bbdfc0afb8850ca8fdc055b2d`) and three toy prices at0x12594 (12 bytes; SHA-256 `0935002e4c7afc324f5aca05efcb6b3d674834a5cd1e6391f2fdf7e66c940909`). The following0x125A0 words are composition pointers and are not additional prices. No duplicate cartridge/compression/checksum reader was introduced.

## Validation

Eleven focused Rust tests passed, including the actual supplied payload's7/12/3 prices and corruption/truncation rejection, all32768 ordinary egg rolls, jewel thresholds and all five cycle rotations, cache reuse/no extra RNG, native timer gates, first-empty slot/capacity, one displayed subject per opening, exact wallet boundaries, input rejection without mutation, and opaque byte preservation. The module is additively exported through the existing Rust crate; its tests use that normal public module. This does not wire a service consumer.

The optional `tools/verify_native_shop.py` executes the actual supplied Thumb instructions beginning at02007688 with Unicorn2.1.4. It models only task allocation, sprite queuing and BIOS signed Div/Mod, and compares the Rust result with native save bytes, RNG seed, cached egg, offered fruit/egg/toy IDs and RNG call counts across first and repeated openings. The allocation stub does not invent the chosen subjects: the native instructions write their IDs into the allocated task records. Reader methods are unreachable stubs in the standalone rustc proof harness only; real source-gate tests above use the existing Rust reader.

Final proof: `C:/Users/Admin/Documents/ChatGPT/mmo/.local/reviews/native-shop-thumb-final/result.json`: **5260 cases passed**, including4096 random cases, with217 compared bytes per case and1025 native BIOS modulo calls. Corpus SHA-256 `225db28b42cce47cd2535ab372c0dcf5f7a67521f785222b9fb32c5ed1b1d109`; Rust module SHA-256 `40d3d494565344d30810f96ea2bb158e0d4d76acfc9a9ea4d537a32625d560d4`. Proof output and native assets remain ignored/local.

```powershell
# Activate the existing Rust environment first, where needed.
$env:CHAO_TINY_GARDEN_PAYLOAD = 'C:/Users/Admin/Documents/ChatGPT/mmo/.local/gba/mb-chao-garden-A2NE.gba'
cargo test --offline --manifest-path crates/tiny-garden-core/Cargo.toml --test native_shop_state -- --include-ignored
cargo clippy --offline --manifest-path crates/tiny-garden-core/Cargo.toml --test native_shop_state -- -D warnings
python crates/tiny-garden-core/tools/verify_native_shop.py --payload $env:CHAO_TINY_GARDEN_PAYLOAD --output-dir .local/proofs/native-shop-new --unicorn-dir 'C:/Users/Admin/Documents/ChatGPT/mmo/.local/tools/native-growth-proof-python' --random-cases 4096
```

## Remaining boundaries

The CPU proof covers stock/timer/cache execution, not native purchase machine instructions. Purchases are source-traced and exercised by the focused Rust tests. Full panel animation, native rendering/audio, carrying/drop/terrain placement, fruit refusal/chewing, trumpet/duck/TV interactions, hatching/new-Chao creation, transfer, save-sector checksums and durable MMO transactions are separate work. Clearing `held` belongs to a later native interaction adapter and is not presented as implemented dropping. Stock's raw save+0x58/+0x68 meanings and egg appearance/genetics names remain deliberately unasserted.

The original signed BIOS modulo can produce invalid cycle bytes after a negative frame-counter rollover. When the cycle is uninitialized, Rust rejects counters above i32::MAX before mutation; this unsupported-source-domain guard is explicit. Valid native cycle/fruit/egg IDs are checked rather than reproducing arbitrary native out-of-range memory reads. This is a bounded native subsystem port, not a full GBA game-engine or live egg/toy gameplay acceptance claim.
