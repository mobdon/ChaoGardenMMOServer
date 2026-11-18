# ADR-0006 — Native source provenance and local assets

Status: required local-data/tooling boundary, November 18, 2026.

Preserve the user's Sonic Adventure 2 + BATTLE Chao development scope and the
supplied Sonic Advance, Sonic Advance 2 and Sonic Pinball Party investigations.
Original installed games, ROMs, native/converted art, audio, saves, credentials
and reference tools stay local/ignored. Publish source tooling, bounded tests
and provenance manifests without embedding those original contents.

Use Rust for game-format readers/converters. Record exact source version,
hashes, offsets, output provenance and verified/unknown semantics. Do not reuse
one ROM's layout for another version without evidence. A decoder, selected
asset export or byte-match test is not a complete engine/gameplay port.

Preserve native Chao proportions, rigs and source motions; use Chao actions
before adapting a stage-character capability. Keep rendering adaptations and
MMO gameplay tuning explicitly labeled.

This record makes no licensing, distribution, trademark, affiliation or store
eligibility conclusion and does not replace the selected scope with invented
species/currencies. Release-asset decisions require their own actual review.

References: [repository requirements](../../../AGENTS.md),
[local assets/data](../../../README.md),
[native inspection limits](../../../crates/tiny-garden-core/README.md),
[release draft status](../../release/README.md).
