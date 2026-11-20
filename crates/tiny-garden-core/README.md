# Tiny Chao Garden core

Reusable Rust foundation, version0.1.0. The shop reader is available to the server adapter; the pure training ledger remains separate from durable service integration. No ROM, extracted program, save, third-party C/assembly, original graphics or audio is included in this package.

Implemented:

- Raw GBA header inspection with bounded reads, original bytes, complement check and source fingerprints.
- Bounded BIOS-format type0x10 LZ77 byte decoding, including overlapping back references. Invalid distances, truncated streams, output overruns and caller limits return errors.
- Fingerprint-gated extraction of the exact verified Sonic Advance2 USA Tiny Chao Garden program; a bounded aligned scan can locate that same fingerprint elsewhere.
- Read-only native save-record checksum validation and physical-sector selection. Every body byte remains opaque, including any unknown Chao type, stat, form, animal history or genetic data.
- Fingerprint-gated, bounded native Tiny shop reader for seven fruit prices and seven signed raw channels per fruit from the exact A2NE multiboot payload. Gameplay interpretation remains a separate server adapter.
- A **custom MMO adaptation** for server-timed idle training, with frozen server policy, a maximum8-hour duration, stored server timestamps, monotonically increasing job IDs, once-only settle/cancel transitions, bounded awards and trusted-store restoration.

This is not a full Tiny Chao Garden port, GBA emulator, sprite/type renderer, phone login system or database synchronization layer. It does not claim that a shared sprite proves support for Chaos Chao type codes. Native field semantics require a separate source/runtime audit. Five original main-game stats are selectable in the custom training adaptation; it does not manufacture hidden-stat training rules or overwrite a native save body.

## Native evidence

The inspected reference is [SAT-R/sa2 commit2dd3f17d30cd3c156e508aac0017620ae485b9ba](https://github.com/SAT-R/sa2/tree/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden). Its [save.h](https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/include/save.h) and [save.c](https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/src/save.c) identify the0x858-byte record, marker0x1965, six physical sectors10–15 at0x1000-byte spacing and greatest unsigned counter selection. Equal counters retain the first sector. Counter wrap follows that original comparison rather than an invented rollover heuristic. The reader inspects the first64KiB bank; any trailing bytes are reported without assigning them unverified semantics.

[game.c sub_0200185c](https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/src/game.c) initializes a16-bit accumulator toFFFF and complements its result. [data3.s](https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/data/data3.s) locates its256-word table at payload offset0x1FD74. **Every table word was verified against the actual locally decoded program**: reflected polynomial8408, initialFFFF, final XORFFFF. The CRC checks record bytes2..0x858. The standard `123456789` check is906E.

The byte-format decoder was independently written for this crate and checked against [mGBA's BIOS-format LZ77 implementation](https://github.com/mgba-emu/mgba/blob/master/src/gba/bios.c). The parser reports metadata and retains raw header bytes; it does not embed Nintendo's logo or infer ownership from a checksum. Source-format facts and local verification do not imply affiliation with SEGA, Nintendo, mGBA or SAT-R.

Private local verification on November18,2026:

| Supplied ROM | SHA-1 | Result |
| --- | --- | --- |
| Sonic Advance, ASOE, revision0,8MiB | `d842afa7dd1e84de08addb94a51506f1bcafd551` | Valid header; the one USA Advance2 program fingerprint was not found. This does not rule out a different native garden version. |
| Sonic Advance2, A2NE, revision0,16MiB | `7bcd6a07af7c894746fa28073fe0c0e34408022d` | Exact reference retail fingerprint; compressed program at0xAED70. |
| Sonic Pinball Party, A3VE, revision0,8MiB | `92b5c19cb4dee8fdfc15a8abdc699822d5373042` | Valid header; the one USA Advance2 program fingerprint was not found. No shared offset assumption. |

The A2NE stream consumes155,273 compressed bytes and produces191,340 bytes. Its SHA-1 is `2c7da8e1c9af85986eecf4effae39197cdac9abf`, matching the reference's [mb_chao_garden.sha1](https://github.com/SAT-R/sa2/blob/2dd3f17d30cd3c156e508aac0017620ae485b9ba/chao_garden/mb_chao_garden.sha1). The reference README's retail-ROM hash is not reused as the multiboot-program hash. Decoded SHA-256: `84d4df3f397e25885768d212a46287db706857651102b9145595cfda30cee7ca`. All original bytes and inspection results remain under the ignored client `.local/gba/` directory.

## Build, inspect and verify

From the client workspace in PowerShell:

```powershell
. ./scripts/use-rust.ps1
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) '.local/tiny-garden-validation'
cargo test --manifest-path server/crates/tiny-garden-core/Cargo.toml
cargo clippy --manifest-path server/crates/tiny-garden-core/Cargo.toml --all-targets -- -D warnings
cargo run --manifest-path server/crates/tiny-garden-core/Cargo.toml --bin tiny-garden-inspect -- header .local/gba/A2NE-f2b7a3329401.gba
cargo run --manifest-path server/crates/tiny-garden-core/Cargo.toml --bin tiny-garden-inspect -- scan .local/gba/A2NE-f2b7a3329401.gba
```

The CLI also supports `save RAW_FLASH_DUMP`, `verify-payload RAW_MULTIBOOT` and `extract-reference RAW_ROM NEW_OUTPUT_FILE`. Extraction requires the full known retail fingerprint and decoded payload fingerprint, verifies the native CRC table, and refuses to overwrite an existing file. A scan miss covers only this one verified program fingerprint. It never labels other streams or regional versions absent.

Opt-in real-ROM regression test, using the user's private local file:

```powershell
$env:TCG_REFERENCE_ROM = Join-Path (Get-Location) '.local/gba/A2NE-f2b7a3329401.gba'
cargo test --manifest-path server/crates/tiny-garden-core/Cargo.toml --test local_reference -- --ignored
```

Validation completed:21 unit tests, the opt-in real-ROM test and Clippy with warnings denied. Unit fixtures are synthetic. The real-ROM test verifies header identity, exact decompressed bytes/hash/length, all256 native CRC table entries and the bounded scanner's matching offset.

## Verified Tiny shop tables

`shop::TinyShopCatalog::from_verified_payload(&[u8])` first requires the exact
191,340-byte payload and existing reference SHA-1 above. It then checks both
bounded table spans against their SHA-256 fingerprints before reading any row:
effects at `0x12510`, 49 bytes,
`b33ae558a366ac031e7326d7ade4d3a38f3818f21c6139f6a97a8ceeaa3c49bd`;
prices at `0x12548`, 28 bytes,
`e9eaed988f3156a9b8c28f72dc48adac7c1735915afd280c7a2406529cc2dc19`.
The resulting public `fruits` array contains seven `TinyFruit` rows with
zero-based `id`, little-endian `u32` `price` and unchanged `[i8; 7]` `raw`.
No original decoded tables are embedded in source or synthetic fixtures.

This reader does not assign channel names, apply effects, spawn inventory,
charge rings or establish a native save/server transaction. It rejects another
payload/version or a full cartridge ROM rather than guessing offsets. Server
consumption, purchase authorization and any MMO application policy remain
separate from the source reader.

Opt-in verification needs only the owner's decoded payload, not the full ROM:

```powershell
$env:CHAO_TINY_GARDEN_PAYLOAD = 'C:/path/to/private/mb-chao-garden-A2NE.gba'
cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --test local_shop -- --ignored
```

The test checks exact payload/table identity and parity of every decoded row,
without printing or redistributing the original table contents.
The current core run passes 25 unit tests, the opt-in owned-payload shop test,
formatting and Clippy with warnings denied. The unrelated full-ROM test stays
opt-in and was not required for this reader.

## Training integration boundary

The default custom rule awards one main-stat point per completed minute, for at most8hours/480points. A server-configured policy is validated, bounded and frozen into each job. Early settlement returns a remaining duration; later arrival cannot extend the job or award. Canceling an unfinished job closes it without payment; completed work must settle. Backwards server clocks, duplicate or stale IDs, overlapping jobs, overflow and malformed stored state are rejected.

Only the authoritative server may supply `server_now_ms` or training policy. Clients choose a permitted stat and duration, then send intents; they never submit elapsed time or gains. `from_trusted_json` is for a trusted database row associated with the expected Chao ID, not client save uploads. Stored JSON is not authentication.

A future adapter must authorize the profile/Chao, lock its shared record, load this ledger, settle/cancel, apply an award to that same canonical desktop/phone Chao, and atomically persist both changes. Granting points separately from recording settlement can duplicate or lose awards after a crash; this pure crate deliberately does not claim to solve that database transaction. Native opaque data, all seven canonical stats, forms, animal history and genetics must survive that adapter unchanged except for explicitly authorized progression.
