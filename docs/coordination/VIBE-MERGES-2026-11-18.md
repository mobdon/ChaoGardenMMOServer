# Vibe PR integration — November 18, 2026

The user authorized fixing Vibe's GitHub PR merges and clarified: merge only
useful work that the project can use. Rust authority, native assets first,
separate repositories and existing-save protection remain required.

Reviewed base: `0226dc96adc24fc87ee22f8e5da6b8147d6b29a4`. The client PRs
and Rust implementation PRs 15–18 are separate deliveries and are not accepted
by this cleanup. Original branches remain available; none are deleted.

## Dispositions

| Server PR | Original head | Decision and evidence |
| --- | --- | --- |
| #5 simulation | `8ba8abbb32387b850fb3618310696e1e525453de` | Closed as superseded. All 16 changed-file blobs are identical on the reviewed main; merging contributes no usable source. Existing TypeScript simulation remains unaccepted. |
| #6 authentication | `f80497fb69c66c25adc5f94ed0671185e7b1974a` | Closed as superseded by later source including merged PR #13; 23/26 changed paths are identical; resolving the other three toward the incoming branch would regress newer package/schema/AppModule source. Rust pairing remains separately reviewed in PR #15. |
| #8 monorepo/CI | `5d4bc1aae8678ab27d287966d966dcdfc18fb02d` | Reworked into useful Rust build/CI repairs. Reconciled the original history with current main, retaining main's tree before adding reviewed Rust fixes. The competing Turbo/NestJS/TypeScript defaults and duplicate stacked source do not enter the final delta. |
| #9 architecture | `5e90c4b96333640ae72fb2339383cb5601d9b3b3` | Replace false accepted single-repo/TypeScript architecture with the actual Rust/two-repository design and explicit proposal/implementation limits. Preserve agreed Chao, rings, native content and Tiny Garden scope. |
| #10 widgets | `edcc1ade80891251d80f43d2387d413a5f5d7e15` | Closed without merging. No promised native widget implementation; wrong repository and snapshot shape. Android Tiny Garden widget work stays in client PR #3, under its own build/device/contract gates. |
| #11 operations | `6572e94a04002db396a4b4cfd5687dac4f8a51c4` | Keep only verified operating guidance: canonical JSON save boundary, private quiesced backups/restore checks and explicit rollout/migration limits. Drop unsupported claims about database backups, automatic rollback, preserved unknown fields and implemented live features. |
| #12 presence/visits | `0fa82723e59eda99617c2810f0e476c50689af0d` | Closed without merging. Adds competing unconnected NestJS services and no real Rust round trip. Shared gardens already use Rust snapshots; private garden visits remain a Rust feature/ownership-contract task. |
| #19 social/competitions | `943399b31bcd75363a0fe5eba070937b47f900b3` | Closed without merging. All 10 paths extend unaccepted Prisma/TypeScript repositories/contracts. Schema fields on User disagree with Profile queries/migration; SQL uses VisitPermission before creating it; changed competition contracts have unchanged callers. Preserve friends/privacy/visits as Rust implementation directions. |
| #20 presence/visits | `3ca0af957f2fa51360fc133a878f9e0e1f00f7cb` | Closed without merging. Includes #19 plus NestJS/Socket.IO/Redis/Expo paths, 23 files total. Invalid parameterized getter, unbound command handlers, missing actual room joins/Redis subscriber and in-memory replacement state do not establish canonical Rust integration. |

Original PRs #8/#9/#10 carried 83/97/107 stacked paths rather than only their own
30/14/12 changes. Seven inherited conflicts affected README, API/package
settings and simulation/contracts configuration; PR #10 added memory/export
conflicts. These branches must not be merged wholesale because Git resolves
their textual conflicts.

## Useful build repair in PR #8

- Remove duplicate `Instant` imports and invalid `Config.metrics` initializer.
- Render metrics without overlapping mutable captures of the output buffer.
- Apply the existing binary's formatting-only print correction.
- Add actual shared-counter/histogram and router metrics regression tests.
- Run formatting, strict Clippy and tests for the server, Tiny core and Tiny
  games as three explicit CI jobs. The independent core previously had no CI
  job. Retain the locked Node reference test path and release build.

Local validation with Rust 1.100/GNU:

- Server: 80 tests passed, including 18 real protocol tests, canonical session/
  persistence/receipt behavior and disposable executable shutdown/restart.
- Tiny core: 21 passed; one explicitly opt-in retail-ROM reference test ignored.
- Tiny games: 22 passed.
- Strict all-target Clippy and formatting passed for all three manifests.
- Node reference: 48 tests passed after `npm ci --ignore-scripts --offline`.
- Release build passed. Remote CI results are recorded with the final PR
  heads; local checks alone are not a claim that remote CI completed.

No new gameplay protocol, migration or runtime rollout occurs in this repair.
The existing canonical Chao, credentials, rings, gear, resident opt-in and Tiny
receipts remain under the running frozen Rust authority. This checkout uses
disposable test data; it does not replace the live executable.

## Recorded GitHub results

- PR #8 merged as `a1bc2cec560b1714e2a39d7d42de60870c1c2eee` after
  head `874874c5f213dbe18874439beec5b27d3e3a11b0` passed all four jobs in
  GitHub Actions run 44389478261: server, Tiny core, Tiny games and Node reference.
- PR #9 merged as `6677ce436884202ef24a3b053b501b998df45a93` after
  head `5bb509976d12d7df1103e2c56000acde90b2feff` passed run 44390226123.
- PR #11 merged as `72019e1461f19a5d3317dd45a96c88e1c35aaaf7` after
  exact head `adac0e43e5e98366a48bb8cf8c5c470f9ee67540` passed all four jobs
  in run 44391140234.
- PRs #5/#6/#10/#12/#19/#20 were closed without merging and their source
  branches retained. #19/#20 arrived during the pass and were reviewed against
  main `72019e1`. The final open inventory contains independent Rust PRs 14–18;
  those remain separate integration work and were not merged by this cleanup.

## Outstanding acceptance

The prior review findings are retained in
[VIBE-REVIEW-2026-11-18.md](VIBE-REVIEW-2026-11-18.md) and
[VIBE-PR7-REVIEW-2026-11-18.md](VIBE-PR7-REVIEW-2026-11-18.md).
VIBE-01 is repaired by this source change. It does not fix the separate
release migration, durable-volume/shared-writer, actual backup, readiness,
metrics instrumentation, broken generated E2E/load scaffolding, competing
TypeScript authority or unaccepted mobile/auth contracts.

Away-time competitions, AFK jobs, private visits and installed widgets retain
their implementation/acceptance tasks. Useful plans remain tracked even when
their incompatible Vibe PR is closed. No native game assets, ROMs, saves,
credentials, extracted outputs or local runtime binaries are published.
