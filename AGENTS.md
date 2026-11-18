# Project implementation requirements

Read `docs/coordination/BOARD.md`, `docs/coordination/PROTOCOL.md`, and the assigned brief in `docs/coordination/WORKSTREAMS.md` before implementation. The current Vibe batch review is `docs/coordination/VIBE-REVIEW-2026-11-18.md`.

## Rust requirement

- The user requires Rust for game systems, shared mechanics and game-format readers/converters. Production simulation, care/progression, inventory, training, competitions, rewards, identity, saves and validation belong in Rust. Shared Tiny Garden mechanics must be reusable Rust crates; browser execution of shared logic uses Rust/WASM where appropriate.
- A TypeScript/Expo or NestJS implementation does not satisfy a Rust subsystem task. The generated `apps/mobile`, competing `apps/api`, TypeScript `packages/simulation` and invented contract deliveries are unaccepted. Their presence on the upstream branch is not approval to build further game systems on them. Implement server/game systems and shared simulation in the existing Rust authority/crates.
- Browser rendering and Android OS bindings must stay presentation/platform adapters to the agreed Rust core and authoritative protocol. They must not become an alternative implementation of gameplay, progression or trusted saves. Document the adapter boundary and test it against the real service.
- Keep the two repositories separate: server Rust authority belongs here; browser, Tiny Garden and Android widget presentation belongs in `mobdon/ChaoGardenMMOClient`. The retained Node server is a comparison/reference implementation.

## Implementation and evidence

- Use the actual `src/service.rs` routes, `src/game.rs` snapshot/session fields, `PHONE.md` and `WATCH.md`. Proposed v2 pairing/leases require coordinated server and all-client changes. Do not invent REST endpoints, refresh tokens, snapshot versions or schemas and label them implemented.
- Preserve the same canonical Chao, existing credentials/owner state, rings, stats, gear, Tiny receipts and resident opt-in. Atomic durability and once-only rewards are required. Migration must preserve unknown fields, survive failure and maintain a single authoritative writer.
- The current authority uses local JSON files and in-memory state. PostgreSQL/Redis infrastructure does not make it distributed. Before replication, implement and prove shared ownership/storage or use a single authority with durable storage and tested backup/restore.
- Run appropriate Rust formatting, Clippy and tests, including separately packaged Tiny crates. For new client/platform adapters, prove compilation and an actual request/response round trip. A mock-only result or generated checklist does not establish integration.
- Tests must fail when acknowledgments, persistence, authority or load thresholds fail. Use disposable data, explicit isolated ports and fail-closed environment checks. Never default a mutating test to the live preview.
- Mobile scope is Tiny Garden: same Chao, care/stats, AFK training and all five minigames. Android widget scope is viewing and quick care, with full activities opening Tiny Garden. Neither a web mockup nor Expo scaffolding proves an installed widget or durable training service.
- Keep source files, asset inputs/outputs, ROMs, saves and credentials private/local where already required. Publish source tooling, tests and documentation. Preserve native asset provenance and Chao rigs/proportions.
- Always use native game effects and assets first. Search the installed sources and existing inventory before choosing a fallback. Never substitute a handmade effect or a different game's effect when the matching original exists. Preserve its geometry, textures or vertex colors, material/alpha behavior, rig and proportions; document any required attachment or timing adaptation.
- Use isolated branches/worktrees. Record the exact base/commit, changed paths, tests, protocol dependencies and remaining limits in the coordination board. Preserve other work and the running preview; source review/publication does not authorize deployment or live data migration.

These instructions record the user's November 18, 2026 Rust and repository requirements. The reviewed Vibe batch remains unaccepted until its applicable findings and acceptance gates are resolved.
