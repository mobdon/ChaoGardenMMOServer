# Native A2NE weeds

`native_weeds` ports the original garden's weed spawning, emergence, cursor pulling, removal and child-effect timing as reusable pure Rust. It uses original source data and `native_growth::NativeRng`. It adds no wallet/stat rewards, service endpoint, save migration or live gameplay rollout.

Source is the owner's supplied A2NE USA revision0 decoded Tiny payload: 191340 bytes, SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`. Assembly/C reference: SAT-R/sa2 commit `2dd3f17d30cd3c156e508aac0017620ae485b9ba`, `chao_garden/asm/code.s` and `chao_garden/src/game.c`. Addresses use load base0x02000000. Source/assets/saves remain local; no original binary bytes are embedded in this crate.

| Original source | Implemented operation |
| --- | --- |
| `02003424` | Sixteen saved slots at save+0x37/0x38+2×slot. Global flagsF3 or full capacity pause the task. Timer at save+0x10 uses a wrapping decrement: zero underflows; spawning occurs only when the decrement becomes zero. |
| `0200344A..0200349A` | Reset timer to `(native_rng % 30 + 60) × 60` task updates. Draw x=`rng % 160 + 8`, y=`rng % 136 + 24`. Accept only exact native map words9/13. Every rejected candidate also consumes an extra discarded raw RNG draw before retrying. First saved x=0 slot receives the original position. |
| `02003534` |64 eligible emergence updates, preserving original y displacement and fixed-point ObjAffineSet input16384/frame. Flag10 suppresses the callback/render; flag80 preserves rendering but pauses emergence. |
| `020035CC` | Restored saved weeds begin active at their original position, rather than repeating emergence. |
| `02003614` | Native interaction byte19 states0..4 create/start an independent original child effect. Final removal waits for the cursor's completion flag, clears the saved x byte only, decrements the live count and disposes the weed. The saved y remains untouched. |
| `02003748`, `020038EE` | Bounded begin-pull helper preserves idle/new-A/no-direction/modal/flag/shop-boundary gates and native class3 hit bounds. Original scene pick sorting remains the host's responsibility. |
| `02003C24` | Cursor pull phases last8/1/16/8/1/16/13/12 updates,75 total. The original trig table drives the pull arc. The cursor callback pauses on flag80; its distinct handling of flag10 is preserved. Source sounds15/D are reported as native sound IDs. |
| `020036EC` | Separate child task uses original tiles18E/190/192, advancing by2 every10 release updates; it disposes at194. The effect can outlive its parent weed. |

The matching original asset was found and visually verified: native tile109, composition205E4, OBJ palette1 (`1C` attributes), using the original secondary OBJ stream268B4. It is the green weed sprite. The child uses composition205FC, original tile18E and palette8 (`8C` attributes). This module reports native effect/timing data; it does not substitute a handmade effect or claim those assets are mounted in the UI.

## Bounded source data and task ownership

`NativeWeedAssets::from_verified_payload` reuses the existing exact A2NE payload gate before reading the22×20 native map at21BC4,256 signed original trig values at1FB74 and class3 hitbox at20708. Each span has its pinned SHA-256. No duplicate ROM/decompression/checksum reader is introduced.

The original rejection loop is unbounded. Rust accepts a nonzero candidate budget and can return a pending operation. Continuing it preserves the same RNG sequence and does not decrement/reseed the timer again. The authoritative host must finish that pending native callback before advancing task time. This explicit scheduling guard bounds execution without substituting another position policy or granting offline gains. Callback ordering, server time and RNG belong to the host; phone elapsed time and uploaded rewards are never inputs.

Known raw-field read/write helpers touch only save+0x10..0x14 and0x37..0x57, preserving opaque bytes. They do not validate native save sectors, establish identity or authorize importing inventory/stats into MMO saves. Restored runtime state must be built from a consistent host-owned field snapshot.

## Checks and proof

Ten focused Rust tests passed, covering exact RNG/discard/resume behavior, timer zero underflow, paused/full capacity, first free slot, original map words,64-step emergence,75-step pulling and child lifetime, original hit boundaries, opaque bytes, actual supplied native map/trig/hitbox and1000 source-bound seeds. Real source corruption/truncation rejects. The module is additively exported through the existing crate and tests use its normal public API.

The optional `tools/verify_native_weeds.py` executes the actual supplied Thumb callbacks at3424/3534/3614/36EC/3C24 under Unicorn2.1.4. The native map, trig and RNG execute from the verified original payload. It models task allocation/disposal, affine-slot allocation/release, render queues/sounds, BIOS signed Div and ObjAffineSet's output; it observes the original affine inputs. The harness keeps an explicit cursor→weed→child task order and compares raw save bytes, seed/call count, weed/cursor/effect fields and lifetime.

Final local evidence: `C:/Users/Admin/Documents/ChatGPT/mmo/.local/reviews/native-weeds-thumb-final-source/result.json`: **4819 actual-Thumb cases passed**, including4096 random cases and120 bytes compared per case;20627 native BIOS divisions. Rust module SHA-256 `2ca24859d977b6b3121ffcf2fc11f766f54ff1987731bee52d967005a53a0fc6`; corpus SHA-256 `6de07a739d669a23867514725b2f4cefea8f2bbcb0f77e2e47ffe26566f69538`. Original source/private proof output remains ignored/local.

```powershell
$env:CHAO_TINY_GARDEN_PAYLOAD = 'C:/Users/Admin/Documents/ChatGPT/mmo/.local/gba/mb-chao-garden-A2NE.gba'
cargo test --offline --manifest-path crates/tiny-garden-core/Cargo.toml --test native_weeds -- --include-ignored
python crates/tiny-garden-core/tools/verify_native_weeds.py --payload $env:CHAO_TINY_GARDEN_PAYLOAD --output-dir .local/proofs/native-weeds-new --unicorn-dir 'C:/Users/Admin/Documents/ChatGPT/mmo/.local/tools/native-growth-proof-python' --random-cases 4096
```

The proof does not establish original whole-game task scheduling, draw/pick ordering, an integrated renderer or durable Rust service acceptance. Native Chao behavior's reaction to weed density is a separate state machine. Actual Tiny minigames are separate ports. No original callback in this traced weed operation grants rings or stats, and this Rust slice invents no such rewards.
