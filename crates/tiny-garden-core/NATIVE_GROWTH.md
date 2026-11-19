# Native Tiny Garden fruit growth

`src/native_growth.rs` ports one bounded operation from the supplied Sonic
Advance 2 USA Tiny Chao Garden program into reusable Rust. It covers signed
fruit channels, needs, experience, level-ups, grade-based point awards and
the exact caller-owned random seed. It is independent of server sessions,
rendering, clocks and files.

The source-only module is not a claim that the entire game is ported. It does
not implement food selection/refusal, chewing, object lifetime, eating timers,
animations, inventory consumption, native save resealing or authoritative
persistence. The existing MMO feeding adapter is a separate policy; it must
not silently switch to these rules and overwrite the canonical Chao's levels,
grades or points. An explicit reviewed adapter/transaction is still required.

## Verified source

Reference: SAT-R/sa2, commit
`2dd3f17d30cd3c156e508aac0017620ae485b9ba`, files under `chao_garden`:

| Reference file | SHA-256 of inspected source |
| --- | --- |
| `asm/code.s` | `36dffd71ff6c21ab959d3c7f59f4798ae23dd49935ad0e4fc62196cfe708c2a2` |
| `src/game.c` | `c0270d1e3d44b8045a5a8bd350cebfbc194537df308ed29e02d6eb54eba6d7df` |
| `include/save.h` | `d6de43595d9eb9571f1057bffde6d950bdfbac84e0881cbddb8324d2b682b688` |
| `ldscript.txt` | `99e0e07ebf579a1bc06b55b6f6e907ae83d243cde126490c69ac156752d2db22` |

The operation starts at `0x0200493C` and ends before animation allocation at
`0x02004A4A`. Its actual machine code was executed from the private decoded
191,340-byte A2NE payload, SHA-1
`2c7da8e1c9af85986eecf4effae39197cdac9abf`. This fingerprint is the decoded
multiboot program, not the retail cartridge fingerprint. No original program,
fruit table, graphics or source listing is bundled in this change.

Native mapping, relative to `gSaveGameState = 0x03003B80`:

| Field | Offset | Encoding |
| --- | --- | --- |
| Mood / belly | `0x73` / `0x74` | Unsigned bytes |
| Five experience gauges | `0x78..0x7C` | Unsigned bytes |
| Five grades | `0x80..0x84` | Unsigned bytes |
| Five levels | `0x88..0x8C` | Unsigned bytes |
| Five point totals | `0x90..0x99` | Big-endian `u16` |

Skill order is swim, fly, run, power, stamina. The verified fruit reader in
`shop.rs` supplies signed `[mood, belly, swim, fly, run, power, stamina]` rows
from `0x12510`; this module does not embed a substitute fruit catalog.

## Exact arithmetic and adapter boundary

The first two signed ratings multiply by 20 and clamp needs to 0..200.
Each skill rating multiplies by 10; negative resulting experience clamps to
zero without changing levels, grades or points. Experience above 99 subtracts
100 **once**. A skill already at level 99 then gets experience 99 and consumes
no random roll. Otherwise its level increases once and its points increase by
`roll(5) + 3 * grade + 11`, plus one when the **post-fruit** mood is strictly
greater than 180. Points cap at 9999. Level 98 may reach 99 and receive points.
A negative fruit can reduce a level-99 skill's experience below 99.

The native RNG wraps `seed * 0x41C64E6D + 0x3039` as `u32`, extracts
`(seed >> 16) & 0x7FFF`, then uses the positive modulus. This is not a full
high-16-bit roll. Only actual level-ups consume the seed. The original `STRB`
behavior, including one-subtraction byte wrapping for out-of-domain ratings,
is retained by the raw `apply_fruit` operation for native-format tools.

Authoritative callers should use `apply_fruit_checked`. It rejects needs over
200, gauges over 99, grades over 5, levels over 99 or points over 9999 before
mutation; it also rejects an invalid result before committing either state or
RNG. It does not silently normalize corrupt imports. The caller must still
select a verified fruit on the server, authorize ownership, prevent duplicate
consumption and commit item removal, updated stats and seed atomically. These
range checks do not establish credential, grade, receipt or save provenance.

Raw field reads/writes require at least `0x9A` bytes. Writes preserve every
opaque byte and fail before mutation on truncation. They do not validate or
repair native checksums and must not be treated as trusted save imports.

## Validation

The crate exports `native_growth` for reviewed consumers; integration tests
use that public API. They cover signed clamps, strict mood bonus, one-only overflow,
level 98/99 behavior, point caps, negative max-level experience, exact RNG
wrapping, big-endian fields, unknown-byte preservation and atomic rejection
of every invalid field/result.

Run from the server checkout with Rust on PATH:

```powershell
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --test native_growth
cargo clippy --manifest-path crates/tiny-garden-core/Cargo.toml --test native_growth -- -D warnings
```

The independent opt-in verifier executes original Thumb growth and RNG
instructions in Unicorn 2.1.4. Only GBA BIOS SWI6 signed integer division is
modeled, with the actual path restricted to a 15-bit nonnegative dividend and
divisor five. Any other interrupt, input, unexpected stop or mismatched byte
fails the command. It stops before animation allocation and is not full game
emulation. All 154 bytes of the native record and all four RNG seed bytes are
compared against a compiled Rust harness for each case.

```powershell
python crates/tiny-garden-core/tools/verify_native_growth.py `
  --payload C:/path/to/private/mb-chao-garden-A2NE.gba `
  --output-dir C:/path/to/private/.local/native-growth-proof-new `
  --unicorn-dir C:/path/to/private/.local/python-packages
```

The supplied output directory must be new and beneath `.local`. The tool
requires an exact payload fingerprint, never downloads assets, never contacts
a game service and writes only its temporary Rust harness/executable and JSON
proof result there. `--unicorn-dir` is optional when the pinned dependency is
already installed; the production crate has no Unicorn/Python dependency.

On November 18, 2026, the initial corpus passed 9,136 cases: 5,040 deliberate
branch-boundary combinations and 4,096 deterministic random cases split
between valid gameplay states and malformed byte fields. The native path
made 11,917 BIOS Div calls. Source, verifier and corpus hashes are recorded in
the local `result.json`; original table bytes and native inputs remain private.
This proves the bounded operation, not a server transaction or phone round trip.
