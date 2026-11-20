# Native egg hatching and new Chao data

`src/native_hatching.rs` ports the A2NE Tiny Chao Garden new-body initializer
`sub_02001E74` as reusable Rust. This constructor creates a **separate new
native Chao record**. It must never run against the existing owner's canonical
Chao. The checked operation rejects an occupied native slot, invalid/absent
egg ID, truncated record or exhausted random budget without changing either
the record or caller-owned RNG.

## Exact source and data

The inspected primary reference is SAT-R/sa2 commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`,
`chao_garden/src/game.c`, `sub_02001e74`. Its source SHA-256 is
`c0270d1e3d44b8045a5a8bd350cebfbc194537df308ed29e02d6eb54eba6d7df`.
The initializer is called by the original egg hatch animation around
`0x020040BE`. It initializes data; the animation/timer is not included here.

Source data is loaded only from the exact private 191,340-byte decoded A2NE
program, SHA-1 `2c7da8e1c9af85986eecf4effae39197cdac9abf`, using the existing
complete payload gate before bounded reads and per-span SHA-256 checks:

| Data | Offset / length | SHA-256 |
| --- | --- | --- |
| Two native name sets, ten seven-byte names each | `0x202B6` / 140 | `3c3e22f3f953b3ab0579bc713390ac5387ba6b139e33b24fab63904cb4c7d2f5` |
| Native Chao body template | `0x2071C` / `0x780` | `ed21c8ce8c09c5bfde4eb07b7b2d3d8042e501ff7b70f01f293d5c4df35ebfe9` |

Names remain encoded bytes; no character-set meaning is invented. Egg IDs
remain the twelve native IDs 0..11. Existing source-backed shop prices and
shell palette assets do not establish jewel genetics, rarity or a desktop
SA2 appearance mapping. No original names, template, ROM or sprite data is
embedded in these published sources or synthetic test fixtures.

## Ported operation

The original operation clears save-relative `0x6A..0x858`, consumes the egg
at `0x59`, stores its ID at `0x72`, resets traced flags `0x58/0x62/0x69`,
selects one native name and initializes mood/belly to 100. It uses the exact
native RNG and the original three grade branches:

- A random permutation of grades 0..4.
- Five independently rolled grades 1..3.
- Twelve total grade points allocated in native order with per-skill cap 5,
  retaining rejected rolls and zero-roll RNG consumption.

The remaining native defaults, five grade pairs, source template and five
random identifier fields are written at their traced offsets. The identifier
arithmetic uses the native caller's frame counter and wrapping `u32` math.
All 2,136 native record bytes, RNG and three transient flag writes were checked
against the actual supplied Thumb code, including exact random call counts.
Opaque prefix and trailing bytes survive; no checksum is silently resealed.

The native grade-distribution loop has no declared timeout. This safe port
limits the operation to 4,096 RNG calls and rejects exhaustion atomically.
It does not change accepted rolls, weighting or normal loop behavior.

## Canonical adoption boundary

The current Rust server's `Player` combines one credential identity with one
Chao's stats, form, gear, care, position and records. It has no implemented
owned-Chao collection, selection API or adoption limit. Preserving an opaque
collection field alone would not make an additional Chao playable. Creating
another `Player` would incorrectly create another credential identity.

To make special Tiny eggs expand the player's collection, the next runtime
transaction must add a stable new Chao ID under the existing owner, preserve
the current active/admin Chao and credentials, and commit egg consumption,
newborn data, ownership and an idempotent hatch receipt together. Failed saves
must roll back all changes; retries/restarts must not duplicate hatchlings.
Collection capacity and selection/activity rules need an explicit versioned
contract. A native temporary empty record can construct newborn data, but it
must never substitute for an authoritative owner/collection transaction.

This source slice does not create a second playable server Chao, implement
durable adoption, change live saves, or claim SA2 genetics/Chaos appearance
semantics. Original A2NE egg/body fields are retained for that later audited
mapping rather than guessed from shell colors.

## Verification

Synthetic Rust tests exercise all three grade branches over 1,024 seeds,
source gates, exact defaults, flag writes, opaque-prefix/trailing preservation
and atomic refusal of occupied/invalid/truncated/exhausted requests. The opt-in
owned-payload test verifies the real source gate and generated growth ranges.

```powershell
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --test native_hatching
cargo clippy --manifest-path crates/tiny-garden-core/Cargo.toml --test native_hatching -- -D warnings
```

The independent `tools/verify_native_hatching.py` compiles a private temporary
Rust harness and executes original initializer/RNG Thumb instructions in
Unicorn 2.1.4. It models only BIOS Div and the exact zero-filled CpuSet call;
unexpected interrupts, parameters, stops and mismatches fail the command.
It never calls the game service. Output must be a new `.local` directory.

```powershell
python crates/tiny-garden-core/tools/verify_native_hatching.py `
  --payload C:/path/to/private/mb-chao-garden-A2NE.gba `
  --output-dir C:/path/to/private/.local/native-hatching-proof-new `
  --unicorn-dir C:/path/to/private/.local/python-packages
```

On November 18, 2026, 2,408 cases passed, covering all twelve egg IDs, both
native name sets, seed/frame-counter overflow boundaries and 2,048 deterministic
random cases. The actual native paths consumed 39,270 random values, made
17,598 BIOS Div calls and made exactly one zero-fill call per initializer.
The local JSON result records the exact source, verifier and corpus hashes.
