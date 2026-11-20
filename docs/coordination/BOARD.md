# Repository coordination board

Updated November 18, 2026. This is the server-repository coordination layer for
the twelve implementation workstreams and the original integration chat. Read
the [complete briefs](WORKSTREAMS.md) and [proposed protocol contract](PROTOCOL.md)
before starting. The Vibe-generated release documents arrived in server commit
`b525d8b` through PR #1; they are planning drafts, not implementation evidence.
No Vibe website board or external chat has been updated by creating these files.

## Animated footwear integration — locally verified

Client `codex/footwear-motion-fix` commit
`d298d4f2276512e18af58a71dcfd00c02bc04c3f` fixes frozen visible shoes using
animated006/013 sockets and overlay targets, embeds the chest emerald on the
torso surface and adds original-atlas adapted heel jets. See
[FOOTWEAR-FIX.md](FOOTWEAR-FIX.md) for the client paths and exact evidence.
14 client tests, build, three-form native rig proof,14 disposable real Rust
service checks and a198-frame actual UI clip passed. Existing live backend and
canonical Watch/save were preserved. No new server protocol/runtime is included.

The separate native movement commit86f41f9 remains unmerged. Its invalid-height
fallback is fixed; duplicate-time history resets and legacy water fallback
mismatch remain. Future merge must retain actual006/013 footwear targets and
the attachment/effect lifecycle checks. This documentation is recorded on the
local server coordination branch; it does not publish or deploy the retained
independent local locomotion-route source commit5265601.

## Incoming Vibe source — review blocked

**UNACCEPTED — Rust requirement reaffirmed by the user.** Game systems, shared
mechanics and asset readers/converters require Rust; shared browser mechanics
use the agreed Rust/WASM boundary. The generated Expo/TypeScript mobile delivery
does not complete those tasks. Read [AGENTS.md](../../AGENTS.md) before further
implementation. Platform/rendering adapters must use the real Rust protocol.

The remote server head reviewed is
`5ec51f94ad24135a41d6a0c2320d155cccd486f3`, following release-doc commit
`b525d8b`. Read the integration
[review report](VIBE-REVIEW-2026-11-18.md) for concrete findings and repair gates.
These generated PRs are source deliveries under review; they are separate from
the thirteen ownership/workstream rows below and do not prove those chats have
started or completed their briefs.

| Vibe PR / source commit | Delivered paths | Review status |
| --- | --- | --- |
| #2 / `826c0f2` | Infrastructure, deploy/backup scripts, Rust metrics/probes/migration | REVIEW BLOCKED: current source fails compilation; deployment/persistence/migration/rollback require repairs and tests. No production infrastructure was executed. |
| #3 / `7476c94` | E2E, k6 and Maestro scaffolding | REVIEW BLOCKED: invalid fixtures/runner paths and false-positive checks; no passing capacity or end-to-end acceptance established. |
| #4 / `5ec51f9` | Expo mobile UI and contracts under the server repository | REVIEW BLOCKED: integration/contract findings; source UI is not completed Tiny Garden, secure pairing or an installed Android widget. |
| #7 / `282f6ab` | Competing NestJS/Prisma API and TypeScript simulation/contracts, 59 later paths | UNACCEPTED: conflicts with the user's Rust game/server requirement. [Source review addendum](VIBE-PR7-REVIEW-2026-11-18.md) records 17 blockers/reliability findings and validation limits. This is not a replacement Rust persistence layer. |

The main repository already contains these commits. The live preview remains
the separate tested versioned runtime with SHA-256
`51A07F9C382ECB70785654297180349DD2E34F677D5921818EA2B9A6ECEC3100`.
Fetching/reviewing source does not deploy it. Server runtime changes and live
rollout remain held. Review notes, Rust requirements and corrected planning
documents may be published separately; that does not accept the feature code.

## Status and evidence rules

- **ACTIVE — verified:** the integration chat has observed a started workstream.
  This does not mean its implementation has merged or passed acceptance.
- **READY — activity unverified:** a scoped brief is ready; there is no verified
  branch/PR/chat activity recorded here. Existing related code does not prove
  that this separate workstream has started.
- **LOCAL TESTED:** implementation has recorded local source/test evidence.
  This does not establish production deployment, native fidelity, mobile OS
  installation, app-store approval or a finished product.
- **PLANNED:** design or a checklist exists; runtime behavior needs code and
  acceptance evidence. Record branch, base, changed files, PR/commit, tests and
  remaining limits when updating a row. Preserve unfinished user requests.

## Workstreams

The stream numbers match [WORKSTREAMS.md](WORKSTREAMS.md). File paths in an
implementation brief are relative to its named repository.

The native movement stream's current scene/helper proposal and verified motion
states are recorded in [NATIVE-MOVEMENT.md](NATIVE-MOVEMENT.md). Integration has
no pending scene edit at the recorded baseline; the isolated patch still needs
review and real snapshot-driven proof before merge.

| # | Workstream / repository | Coordination status | Existing baseline versus next delivery | Dependencies / shared boundaries |
| --- | --- | --- | --- | --- |
| 1 | Tiny Garden app / client | READY — activity unverified | LOCAL TESTED: browser Tiny UI, five adapted minigames, canonical stats and Watch display. Next: complete touch/reconnect/saved-state acceptance, PWA behavior, real care/items/training controls. | 3 training, 4 identity/leases, 9 care; agree schemas before editing shared UI/transport. |
| 2 | Android home-screen widget / client | READY — activity unverified | LOCAL TESTED: browser widget layout/snapshot preview only. PLANNED: installed Android widget, secure pairing, background refresh and quick server-validated pet/care. Full app owns minigames and AFK training controls. | 4 revocable credentials/observation/care authority; 1 app entry; 3/9 real status and intents; Android toolchain/device evidence. |
| 3 | Durable AFK training / server | READY — activity unverified | LOCAL TESTED: standalone `crates/tiny-garden-core` TrainingLedger. PLANNED: persisted live jobs, start/cancel/settle API, restart recovery, costs/rewards and Tiny controls. | 4 ownership/revisions; common atomic store and once-only receipts; coordinate `game.rs` / `service.rs` hooks. |
| 4 | Accounts, pairing and saves / server | ACTIVE — verified; isolated, unmerged | Known chat `01a1ee49-51ad-7083-8f40-5a83421d5e29`; worktree `.local/worktrees/server-accounts` in the combined development workspace; branch `codex/accounts-pairing-ownership`, recorded base `a8188a3`. Proposals and implementation are not yet a shipped v2 protocol. | [PROTOCOL.md](PROTOCOL.md): desktop/Tiny/widget migration, atomic profile migration, private auth, observe/control separation, lease/revision/operation semantics. Preserve later upstream history and the locomotion asset-route patch. |
| 5 | Rust SA2 + BATTLE asset tooling / client | READY — activity unverified | LOCAL TESTED: existing local extraction/conversion pipeline and selected native outputs. PLANNED: bounded Rust readers/converters with parity; no complete native game-engine port. | Common manifest/workspace agreement with 6–8; local inputs/outputs and source provenance; consumers 9–11. |
| 6 | Rust Sonic Advance asset tooling / client | READY — activity unverified | LOCAL TESTED: exact ASOE source inventory/header/save-format evidence. PLANNED: version-specific complete resource catalog and verified Rust decoders. | Shared manifest with 5/7/8; use exact source offsets; no raw ROM/art/save publication. |
| 7 | Rust Sonic Advance 2 asset tooling / client | READY — activity unverified | LOCAL TESTED: A2NE identity, embedded Tiny payload/header/LZ77/save evidence. PLANNED: broader bounded Rust catalog/extraction parity. | Shared manifest; preserve known hashes and unknown fields; no assumption of shared layouts across versions. |
| 8 | Rust Sonic Pinball Party asset tooling / client | READY — activity unverified | LOCAL TESTED: A3VE source inventory and selected Tiny resource evidence. PLANNED: native table/resource catalog and Rust decoding; full pinball physics remains separate. | Shared manifest and exact-version evidence; Tiny Chao Ball adaptation is not original ROM execution. |
| 9 | Chao care, animals and evolution / server | READY — activity unverified | LOCAL TESTED: current bounded prototype care/shop/evolution rules, audited animal/drive records and explicit Watch pet. PLANNED: canonical animal/inventory/life-system slices and deeper original-rule fidelity. | 4 transaction/ownership; 5–8 source-backed records; publish UI contract for 1/2 and distinguish native curves from MMO tuning. |
| 10 | Native movement and 3D presentation / client | ACTIVE — reported isolated work; unmerged | Integration already published native Run helper/scene hooks/tools/tests at client279a7b7 and recorded Walk → Run → Skate. Movement chat reports codex/native-chao-movement; consume the existing helper rather than duplicate it. Wider transitions/contact/multi-client acceptance remains. | Shared scene.js only a small reviewed patch. Preserve rigs/alpha/grounding/depth-occluded bloom. Exact locomotion route works in the current runtime but its source patch remains separately unpublished while server build is blocked. |
| 11 | AI races and Karate while away / server | READY — activity unverified | LOCAL TESTED: interactive adapted Karate authority and race validation against synthetic routes. All 62 extracted original race configurations remain unavailable pending genuine routes/rules. PLANNED: away-time competitors, scheduling, matches and durable results. | 4 activity authority, 9 capabilities, source catalogs; Watch wandering never triggers race or minigame rewards; AFK training stays in 3. |
| 12 | Capacity and deployment / server | READY — activity unverified | LOCAL TESTED: loopback service, HTTP/WS and persistence tests. PLANNED: measured 100–500-player capacity, accounts/TLS/reachability, deployment/backups and load evidence. No achieved population or public hosting claim. | Measure integrated tick/resident/snapshot/save costs; coordinate 4 and 13; no automatic exposure, service purchase or live restart. |
| 13 | Original chat — integration and release QA / both repositories | ACTIVE — verified | Owns cross-repository contracts, canonical owner/save protection, playable preview, merge order, release evidence and this coordination layer. Source publication is verified; later changes need their own audited release. | Reviews every shared-file/protocol change, resolves conflicts and performs coordinated client/server rollout. |

## Current implementation evidence

The separate Rust Accounts/pairing chat reports v2 source in its isolated
`codex/accounts-pairing-ownership` worktree, with pairing/revocation/leases and
account/identity/migration tests. That is reported work awaiting integration
review and compatible client rollout; it has not merged or touched the live
preview/save. Its incoming `docs/ACCOUNTS-V2.md` contract and durable AFK-job
ownership hooks still need review/consumer agreement. Preserve that Rust work;
the competing NestJS auth delivery does not replace it. No direct reply to
another chat was sent on the strength of its incoming progress note.

These are recorded local results, not fresh claims that every check was rerun
while writing this board:

- Server source baseline `a8188a3` contains authoritative custom gameplay, five
  Rust Tiny game adaptations, projected ring/receipt commits, explicit durable
  Save, and persistent resident Watch. Validation recorded 78 host tests, 22
  Tiny-game tests and 48 Node reference tests plus fmt/strict Clippy and an
  isolated copied-native release smoke. See [PHONE.md](../../PHONE.md),
  [WATCH.md](../../WATCH.md) and [ACTIVITIES.md](../../ACTIVITIES.md).
- Watch keeps one opted-in Chao visible and moving/resting while the server
  runs and the owner is disconnected. It grants no passive rings or skills and
  simulates no service downtime. Explicit pet is a bounded care intent. It is
  an adapted routine, not retail Chao AI or away-time competition.
- The later exact `chao-locomotion.glb` route patch touches `src/service.rs`,
  `index.js`, `tests/protocol.rs` and `tests/reference/assets.test.js`. Focused
  byte/MIME/HEAD/traversal tests, strict Clippy and copied-native smoke passed.
  Release SHA-256:
  `51A07F9C382ECB70785654297180349DD2E34F677D5921818EA2B9A6ECEC3100`.
  Retain this patch when integrating accounts from its earlier base.
- The browser widget preview reads authenticated development snapshots and
  cached presentation state. It is not an installed Android widget and does
  not implement the planned native quick-care interaction. Generated store
  screenshots/checklists cannot prove that interaction shipped.
- Live movement proof on November 18 used ordinary browser inputs in three real
  takes: Walk at 0.798 units/s, native Run119 at 1.236 and ShadowSkate174 at
  1.994. The 182 captured 1848×1252 frames produced a 7.73-second 1280×868 MP4
  and 720×488 GIF; gaps between takes were trimmed without fabricating motion.
  Native Air Shoes/Gloves and the canonical Chao were retained; idle/air fallback
  was checked. Swimming, mounted transitions, two-client presentation and wider
  foot-contact acceptance remain open. Source/transform details and local-only
  evidence paths are recorded in client
  [docs/SHADOW-SKATE.md](https://github.com/mobdon/ChaoGardenMMOClient/blob/main/docs/SHADOW-SKATE.md).
- AFK job-service wiring, production account/pairing migration, native widget
  installation, account deletion, moderation tools, push campaigns and public
  load/deployment acceptance are not established by the release templates.

## Dependency and merge order

1. Agree the versioned ownership/observation/lease/revision envelope through
   [PROTOCOL.md](PROTOCOL.md). Accounts, AFK training, Tiny UI and Android may
   work in isolated modules concurrently; integration coordinates shared hooks.
2. Prove legacy-save migration and atomic reward/receipt preservation with
   synthetic or copied data before enabling new device credentials. Update
   desktop, Tiny Garden and widget transport together at the agreed boundary.
3. Wire AFK jobs and quick care to the real service contract, then validate the
   corresponding app/widget controls. No client clocks or offline claimed
   rewards; passive observation does not silently acquire control.
4. The four asset tracks agree a manifest independently, preserving exact
   version/source evidence. Care, movement and competitions consume verified
   outputs, keeping unknown native behavior explicit.
5. Capacity measures the integrated implementation; integration reviews PRs,
   regression evidence and source-only snapshots before release. Preserve
   `b525d8b` and all earlier server history rather than replacing it.

## Product scope and release drafts

The user's active development scope remains the Sonic Adventure 2 + BATTLE
Chao project with locally imported native assets and the supplied GBA source
investigations. Original inputs, native outputs, ROMs, saves and reference tools
stay local/ignored; source tooling, tests, schemas and documentation can be
published. This boundary does not establish distribution rights or store
eligibility.

The [Vibe release packet](../release/README.md) is a draft for review. Its
all-original creature/store concept, alternate names, checklist answers and
signature fields are not an approved replacement for that development scope,
completed policies, completed legal review or feature implementation. Record
such product/release decisions explicitly when actually made; do not invent
approvals or treat an unchecked checklist as evidence.

## Workstream handoff

Use an isolated branch/worktree and a reviewable PR. Include changed paths,
base commit, proposed schemas, dependency changes, reproducible tests, native
versus adapted behavior and outstanding acceptance. Update the row with actual
branch/PR/evidence once verified. Do not mutate another checkout, live saves,
the playable preview or a shared index, and do not start messaging other chats
on the strength of this board alone. Repository updates do not imply that a
Vibe website dashboard has been synchronized.

Dev realm travel (server base 799e8af, local): src/game.rs and src/watch.rs use the authenticated player's admin status for Hero/Dark entry and resident restore; unchanged alignment, form and protocol. tests/game.rs covers ordinary and dev travel. Fixed existing build blockers in src/service.rs and src/metrics.rs. Game/watch suites: 36 passed; disposable Rust WebSocket session confirmed garden, hero, dark and hub with unchanged form and alignment. Running port 3001 binary and owner save untouched.

### Local character roster preview — 2026-11-19
Client documents source and remaining gaps in docs/SA2-CHARACTER-PORT.md. Server uses a private CHAO_ROSTER_TABLE extracted by the Rust PE reader. Owner-only dev-actor selection; snapshots carry devActor/rosterGait, normal input and attack intents remain authoritative. Verified all 17 selections and visitor denial over a real service socket with disposable storage. No canonical save migration, deployed protocol switch or production rollout.

### Character tech / whole-game Rust port, November 19
Rust owns character state, native land queries, target acquisition, missile flight, dig pickups and flight ring drain. Existing input adds bounded tech hold, action adds roster-dig; snapshots add rosterTech, rosterVertical, rosterLocks and the mission rail. Browser uses native pose/rocket assets. Isolated preview only; no canonical save migration. See client docs/SA2-CHARACTER-PORT.md and server/tests/character_tech.rs for input traces and limits.
