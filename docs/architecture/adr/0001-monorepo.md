# ADR-0001 — Separate repositories and Rust authority

Status: required user/repository boundary, November 18, 2026.

## Decision

Keep `mobdon/ChaoGardenMMOClient` and `mobdon/ChaoGardenMMOServer` independent.
The combined development checkout may retain the server under ignored
`server/`; that convenience does not combine Git histories or ownership.

Production game systems, identity, saves, progression, inventory, training,
competitions and validation belong in Rust server/shared cores. Shared browser
mechanics use the agreed Rust/WASM boundary. Client/Android presentation and
game-format tooling belong in the client repository; readers/converters are
Rust. Existing browser JavaScript rendering remains a presentation adapter.

The generated `apps/api`, `apps/mobile` and `packages/simulation` do not replace
the working Rust service or satisfy these tasks. Their presence in Git is not
acceptance. The Node server remains a reference, not a concurrent authority.

## Integration consequence

Agree contracts through the server [protocol](../../coordination/PROTOCOL.md)
and [board](../../coordination/BOARD.md). Isolate work and provide small shared
entry-point patches. Test real canonical service round trips alongside core
checks. Source publication does not authorize runtime replacement or migration.

Source: [AGENTS.md](../../../AGENTS.md), [Cargo.toml](../../../Cargo.toml),
[service](../../../src/service.rs). No new build is claimed by this record.
