# Chao Garden architecture

Updated November 18, 2026. This describes the required Rust architecture and
the existing prototype boundaries. It supersedes the generated PR #9 proposal
that replaced the Rust runtime with a TypeScript monorepo. It does not accept
the unreviewed generated API/mobile/simulation code or authorize deployment.

## Repository and language boundaries

| Repository | Responsibility |
| --- | --- |
| `mobdon/ChaoGardenMMOServer` | Rust authority, shared Rust game cores, identity, progression, rewards, persistence and validation |
| `mobdon/ChaoGardenMMOClient` | Browser garden/Tiny Garden rendering, Android presentation/platform adapters, local Rust asset readers/converters |

Shared browser mechanics use the agreed Rust/WASM boundary. Presentation code
may render authoritative state and submit bounded intents; it cannot become
another gameplay, wallet or save authority. The retained Node implementation
is a comparison/reference implementation. Never run it and Rust as writers
against the same data directory.

## Current implementation and open work

The Rust service uses an in-memory `Game` and local JSON profiles. Five adapted
Tiny games are hosted through `src/phone.rs`; resident Watch persists an opt-in
and follows a small server routine while the service runs. The standalone
training ledger is not connected to production jobs. Device pairing/leases,
an installed Android widget and away-time AI competitions require separate
integration evidence. PostgreSQL, Redis and generated contracts in this
repository do not establish shared storage or a connected service.

The public source at `0226dc9` contains recorded build and persistence/deploy
blockers. The working preview uses a separate earlier tested binary. See the
[review](../coordination/VIBE-REVIEW-2026-11-18.md); this document makes no fresh
build, capacity or runtime-rollout claim.

## Reference map

- [System overview](system-overview.md): actual modules, persistence and wire boundary.
- [Decision records](adr/README.md): required boundaries and explicitly proposed work.
- [Game rules](../game-design/mvp-rules.md): current adaptations and planned mechanics.
- [Authority requirements](../game-design/anti-cheat.md) and [data handling](../game-design/privacy.md).
- [AGENTS.md](../../AGENTS.md), [coordination board](../coordination/BOARD.md) and
  [protocol proposal](../coordination/PROTOCOL.md): implementation/merge gates.

Original games, ROMs, native outputs, credentials and saves stay local/ignored.
Reader/converter tooling and selected native data do not constitute a complete
game-engine port. Preserve the Chao scope, rig proportions and source provenance.
