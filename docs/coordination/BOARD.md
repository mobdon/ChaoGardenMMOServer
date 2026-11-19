# Repository coordination board

Updated November 19, 2026. This is the server-repository coordination layer for
the twelve implementation workstreams and the original integration chat. Read
the [complete briefs](WORKSTREAMS.md) and [proposed protocol contract](PROTOCOL.md)
before starting. The Vibe-generated release documents arrived in server commit
`b525d8b` through PR #1; they are planning drafts, not implementation evidence.
No Vibe website board or external chat has been updated by creating these files.

## Vibe merge repair

The user authorized useful-only PR integration. [The November 18 integration
record](VIBE-MERGES-2026-11-18.md) records exact incoming heads and useful-only
decisions. PRs #5/#6/#10/#12/#19/#20 are closed without merging; useful Rust
build/CI PR #8, corrected architecture PR #9 and operating-document repair #11
merged after green CI. The November 19 source integration below supersedes
the earlier merge status. Runtime acceptance and save migration have separate
gates.

## November 19 source integration

Server source baseline `1c5d9dd25fd64f857d6fcbad83cdcaff901d4c3a` includes
the Rust care, away-competition and measured-capacity changes. The isolated
12-client/12-resident smoke passed on a 30-second repeat after an initial
failure. Sustained 100–500-player capacity remains unverified.

- Client goal-ring replay was integrated in order. Each candidate passed Rust
  formatting, strict all-target Clippy, the 113 ordinary SA2 tests and eight
  installed-game goal and campaign tests. The changes are offline tooling.
  External game-service results remain caller-supplied. There are no server
  protocol dependencies.
  - [Client #187](https://github.com/mobdon/ChaoGardenMMOClient/pull/187)
    adds the pre-contact range gate in `goal_ring.rs` and its Rust, mounted
    campaign and PowerPC tests. Base `2691298e1c28c41ad464e46774215d16b51c3035`,
    merge `621980829b990707b66b6b83b59eb6bce45410f2`.
  - [Client #188](https://github.com/mobdon/ChaoGardenMMOClient/pull/188)
    adds the shadow-query cache replay in `goal_ring.rs`, `fan_stage_local.rs`
    and `goal_ring_ppc.py`. Base `621980829b990707b66b6b83b59eb6bce45410f2`,
    merge `b0d4719c18b79ed2f6c0f0e37298a4cb92e7cc6c`.
  - [Client #189](https://github.com/mobdon/ChaoGardenMMOClient/pull/189)
    adds alternate-animation and sound observations in `goal_ring.rs` and
    its Rust, mounted campaign and PowerPC tests. Base
    `b0d4719c18b79ed2f6c0f0e37298a4cb92e7cc6c`, merge
    `af0c41d5791478a1e8e024a4035bd8932369f1d3`. Playback and mission results
    are outside the replay.
- [Client #14](https://github.com/mobdon/ChaoGardenMMOClient/pull/14) was closed
  as superseded and its branch retained. The shared desktop/WASM runtime
  landed in `e816c10cd2558bdbd9aa8b1b8dd736b04cd3fc71`. The full release Rust
  suite passed with the local ASOE ROM at `6219808`, including the handoff
  regression for settled Y, the original spindash digest and both pickups.
- [Client #3](https://github.com/mobdon/ChaoGardenMMOClient/pull/3) remains
  a draft. APK and isolated service evidence exist. A read-only software
  emulator stayed offline in ADB, so on-device widget, Keystore, pairing,
  resize, reboot and Doze checks remain open.
- [Server #15](https://github.com/mobdon/ChaoGardenMMOServer/pull/15) remains
  a draft pending desktop, Tiny Garden and widget authentication and
  ownership acceptance together under [the v2 protocol gates](PROTOCOL.md).

The live preview and canonical save were outside this integration.

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
| 1 | Tiny Garden app / client | ACTIVE — verified isolated implementation; review pending | `codex/tiny-garden-mobile`, original client base `af74a8d`; five-screen browser UI, seven canonical stats, five touch minigames, native Chaos forms and offline view-only shell. Isolated Rust five-game/cancellation and Memory wallet/receipt reconnect/restart passed; fixture touch/layout checks are separately labeled. See [Tiny UI handoff](TINY-UI.md) for source, PR and exact limits. | Durable AFK service is unavailable; Training stays disabled. Accounts v2 remains an isolated optional consumer, requiring agreed desktop/widget migration. Integrated v2 care/envelope remains blocked; no runtime rollout. |
| 2 | Android home-screen widget / client | READY — activity unverified | LOCAL TESTED: browser widget layout/snapshot preview only. PLANNED: installed Android widget, secure pairing, background refresh and quick server-validated pet/care. Full app owns minigames and AFK training controls. | 4 revocable credentials/observation/care authority; 1 app entry; 3/9 real status and intents; Android toolchain/device evidence. |
| 3 | Durable AFK training / server | READY — activity unverified | LOCAL TESTED: standalone `crates/tiny-garden-core` TrainingLedger. PLANNED: persisted live jobs, start/cancel/settle API, restart recovery, costs/rewards and Tiny controls. | 4 ownership/revisions; common atomic store and once-only receipts; coordinate `game.rs` / `service.rs` hooks. |
| 4 | Accounts, pairing and saves / server | ACTIVE — verified; isolated, unmerged | Known chat `01a1ee49-51ad-7083-8f40-5a83421d5e29`; worktree `.local/worktrees/server-accounts` in the combined development workspace; branch `codex/accounts-pairing-ownership`, recorded base `a8188a3`. Proposals and implementation are not yet a shipped v2 protocol. | [PROTOCOL.md](PROTOCOL.md): desktop/Tiny/widget migration, atomic profile migration, private auth, observe/control separation, lease/revision/operation semantics. Preserve later upstream history and the locomotion asset-route patch. |
| 5 | Rust SA2 + BATTLE asset tooling / client | ACTIVE — isolated client source published for review | Client [PR 7](https://github.com/mobdon/ChaoGardenMMOClient/pull/7), `c6cd348`: bounded Rust readers/converters; 46 asset-free tests and 8 local parity tests. Stacked [PR 12](https://github.com/mobdon/ChaoGardenMMOClient/pull/12), `codex/sa2-native-runtime` at `2dd351e`: native hierarchy/linear-motion evaluation and Basic collision world/query commands; 67 ordinary tests, 2 native-runtime parity tests, fmt and strict Clippy pass. Independent comparisons cover 133 poses/5,320 node transforms and 94 meshes/2,898 oriented triangles; 191 stable queries pass, with 11 ill-conditioned reference heights explicitly excluded and their geometry/winding retained. Astra reviewed and fixed extreme-magnitude ray normalization. Stacked client [PR 17](https://github.com/mobdon/ChaoGardenMMOClient/pull/17), `1c4ce36`: 15 campaign lists, the three opening event cameras (7,780 independent samples), 58,926/59,020 resolved stage placements, and bounded City Escape callbacks. The offline host can open City Escape but cannot drive Sonic through it. 92 ordinary tests and 20 installed/reference integration suites passed. [Full-game tracker 11](https://github.com/mobdon/ChaoGardenMMOClient/issues/11) stays open; skinning/rendering, native response and gameplay remain unfinished. Native resources, outputs and detailed reports stay ignored/local. | Common schema remains `proposed-sa2-only`; evaluation locators distinguish format and generated-result identity requires output SHA-256 plus source/dependency hashes. Coordinate final shared types with 6–8/integration. No server/runtime/protocol changes, canonical-service integration or live rollout. Consumers 9–11 must distinguish asset/tool parity from native gameplay parity. |
| 6 | Rust Sonic Advance asset tooling / client | READY — activity unverified | LOCAL TESTED: exact ASOE source inventory/header/save-format evidence. PLANNED: version-specific complete resource catalog and verified Rust decoders. | Shared manifest with 5/7/8; use exact source offsets; no raw ROM/art/save publication. |
| 7 | Rust Sonic Advance 2 asset tooling / client | READY — activity unverified | LOCAL TESTED: A2NE identity, embedded Tiny payload/header/LZ77/save evidence. PLANNED: broader bounded Rust catalog/extraction parity. | Shared manifest; preserve known hashes and unknown fields; no assumption of shared layouts across versions. |
| 8 | Rust Sonic Pinball Party asset tooling / client | READY — activity unverified | LOCAL TESTED: A3VE source inventory and selected Tiny resource evidence. PLANNED: native table/resource catalog and Rust decoding; full pinball physics remains separate. | Shared manifest and exact-version evidence; Tiny Chao Ball adaptation is not original ROM execution. |
| 9 | Chao care, animals and evolution / server | READY — activity unverified | LOCAL TESTED: current bounded prototype care/shop/evolution rules, audited animal/drive records and explicit Watch pet. PLANNED: canonical animal/inventory/life-system slices and deeper original-rule fidelity. | 4 transaction/ownership; 5–8 source-backed records; publish UI contract for 1/2 and distinguish native curves from MMO tuning. |
| 10 | Native movement and 3D presentation / client | ACTIVE — reported isolated work; unmerged | Integration already published native Run helper/scene hooks/tools/tests at client279a7b7 and recorded Walk → Run → Skate. Movement chat reports codex/native-chao-movement; consume the existing helper rather than duplicate it. Wider transitions/contact/multi-client acceptance remains. | Shared scene.js only a small reviewed patch. Preserve rigs/alpha/grounding/depth-occluded bloom. Exact locomotion route works in the current runtime but its source patch remains separately unpublished while server build is blocked. |
| 11 | AI races and Karate while away / server | LOCAL TESTED — isolated PR review | `codex/ai-competitions`, local implementation `7830ae7`, rebased onto `0226dc9` (initial base `282f6ab`). One adapted race and one Karate bout proven with local rosters: 25/50 durable rings, restart/replay and duplicate/save-failure coverage. 102 server + 43 Tiny crate + 49 Node tests passed; explicit local-catalog proof passed; format/Clippy/release build passed. [Changed paths, protocol dependencies and limits](COMPETE-01.md). All 62 native courses remain unavailable for manual play. | Integration review required; 4 must authorize the strict competition intents and share the canonical reservation with 3 training; 9 capabilities/source catalogs. Watch wandering grants no competition rewards. No deployment/live-save change. |
| 12 | Capacity and deployment / server | LOCAL TESTED — [PR #18](https://github.com/mobdon/ChaoGardenMMOServer/pull/18) merged; sustained capacity gates failed | Branch `codex/measured-capacity`, implementation `096bfdc` (measured base `282f6ab`, rebased over documentation/scaffolding head `0226dc9`). Rust room filtering, transport bounds, metrics, real-protocol measurements and private deployment/backup artifacts. [Measured report](../CAPACITY.md): 100/0 short timing slice passed, but 120-second lag p95 603 ms failed; extra residents and 250/500 failed. 90 host + 21 core + 22 Tiny-game tests, 11 negative gate tests and five backup tests passed; candidate 12/12 gated smoke passed. No sustained 100–500 capacity or public hosting claim. | [Shared-hook handoff](CAPACITY-HANDOFF.md) coordinates game/service/protocol tests with 4 and 13. Preserve v2 ownership/transaction guards and locomotion route patch; no merge, runtime rollout, migration, public exposure, service purchase or live restart authorized by publication. |
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

Client presentation follow-up: [client PR #9](https://github.com/mobdon/ChaoGardenMMOClient/pull/9)
at `c66c148d95ba18626d90fc1318de4c323dd0345f`, branch `codex/music-controls`, based on exact
`a7a8d093838d66cc5fa190aaf71a60a5ad410de9`, adds blue/gold Music/volume and
independent NPC voice controls, an authoritative Watch/Take control toggle,
exact mirrored right-native boots/title portrait, palm-mounted gloves,
smooth Shadow-derived cadence/reduced head sway and native Chaos sampler
restoration. Changed paths cover `index.html`, `src/{audio,main,scene,
chao-attachments,chao-materials,chao-locomotion,shadow-skate,sonic.css}`, focused
tests/native proof/portrait tooling and client reference docs. New native exits
use `src/garden-exits.js`, `src/world-layout.js`, a scene picking hook, focused
tests and actual-native geometry verifier. No Rust protocol,
authority, identity or save-schema changes. Final client tests/build (24 tests),
three-form original-asset proofs, 2,166-frame glove contact, 648-frame skating
and an isolated frozen-Rust proof (14 checks, latest 59 samples/59 observer snapshots)
passed. Actual isolated browser music mute/volume/voice independence/persistence,
keyboard isolation and Watch start/stop/manual movement passed. The fixture
uses disposable profiles; native terrain copies keep renderer/authority aligned.
Wider terrain, all movement transitions and multi-client visuals remain open.

Tiny/widget readiness checked November 18: canonical browser Tiny at 5173 is
available and shows the same 139-ring Chaos. New native-art browser previews
at 5177 have separate origin/storage. Client PR3 `cf3b236` includes a real local
debug APK, SHA-256
`15adadc4aba82d63e4d35a7a058d0ea904c1bd0568a35616f09291ace626a493`;
build/lint/44 JVM tests and isolated Java-to-Rust pairing/care/restart evidence
are recorded in its `android/verification/STATUS.md`. No installed Android
device/launcher/Keystore/background proof exists. The current live service
does not support that accounts-v2 adapter; ports remain loopback-only. Real
phone connection to the canonical Chao requires coordinated account migration
and reachable HTTPS. Durable AFK training remains unwired. No runtime rollout,
live save migration or public exposure was performed.

New native-area scope: Kindergarten (all nine actual areas, including Black
Market), complete Neutral/Hero/Dark gardens, native doorways returning to the
hub, and original SA2 stage/Chao selection. Client documentation tracks these
requests separately from Amy's four-item prototype shop and catalog cards.
Native bounded doorway clicks are implemented against original mesh/recess
geometry. All three native GLB ray proofs pass; actual browser walking/clicking
the Neutral cave returned a disposable Chao to Rust hub spawn (0,6). Dark's
approach encountered an obstruction near (7.245,6.960), so its walk/click
acceptance remains open. Hero's original doorway lies beyond the current
authoritative terrain domain. Home remains available; original automatic
transition volumes and complete garden collision are untraced/unaccepted.
Client `docs/NATIVE-GARDEN-EXITS.md` records the exact sources and limits.
`docs/NATIVE-KINDERGARTEN.md` audits all nine rooms, 365 decoded room textures
and 55 original NPC/door/motion/light files. Rust GC visual decoding, room
contracts/allowlists, original placement/transitions/collision, classroom and
Black Market economy remain separate integration work. No new room is live.
PR9 is stacked on unmerged client PR8; neither source publication changes the
backend runtime or authorizes a live migration.

Client presentation follow-up: [client PR #8](https://github.com/mobdon/ChaoGardenMMOClient/pull/8)
at `a7a8d093838d66cc5fa190aaf71a60a5ad410de9` corrects animated foot
sockets/toe alignment, shrinks the uniform boot fit and embeds the chest gem
deeper. Original Shadow jet meshes replace the prior adapted sprite effect;
exact retail emitter matrices/timing remain untraced. Final client tests/build,
Rust bundler checks, native geometry/PBR parity, three-form 1,086-sample proof
and actual browser movement capture pass. The disposable Rust protocol proof
passed 14 checks. Local rendering aliases were backed up and installed through
existing routes; live server binary/process and canonical Chaos/139 rings,
gear, resident Watch and mute remain intact. No protocol/authority change,
backend rollout or save migration; preserve these 006/013 targets when
reviewing the independent movement proposal.

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

Head-bob follow-up (November 18): client branch codex/steady-chao-head, base c66c148d95ba18626d90fc1318de4c323dd0345f, anchors actual visible head016 after native animation/skating/foot grounding, restoring its original local pose before animation. Idle/walk/run/skate/stops use the full lock; mounted pilots retain cockpit pose. Changed client src/chao-head.js and scene hook, focused unit/native proof and docs. 26 tests/build and 11,880 actual-native frames pass across three forms; all 21 head/eye meshes steady below 1.96e-7 world units, other local channels/scales/parents intact. Current preview confirms full lock and canonical 139 rings. No protocol/authority/backend/save change. See client docs/HEAD-STABILITY.md and source PR for exact head.
Published head-bob source: client PR #10, https://github.com/mobdon/ChaoGardenMMOClient/pull/10 at 025dcce565d42d22bae3fc558695cbe4f1b92541 (stacked on PR #9); seven client paths, no server code changes.

November 18 follow-up: the user's repeated bob report remained valid after
actor-local head locking. Actual render/camera investigation found a second
camera interpolation behind the already interpolated actor. A disposable
native-garden downhill capture measured 0.042728 world focus height error;
direct camera focus now measures zero error and below0.001px vertical head
center range after turns. Cached draw-mesh measurements supplement the prior
11,880-frame rig proof. Client base025dcce, branchcodex/steady-chao-head/PR10;
changed src/chao-head.js,scene.js,shadow-skate.js,chao-shadows.js,focused tests,
native verification/extraction tooling,Vite exact asset bridge and reference
docs/TASKS/REQUESTS. 32 client tests/build, native head proof and three-form
90-frame sustained coast/30-frame larger-air-gap exit proof pass. Native
downhill Rust/browser observations explain skate resets: genuine small
jumpOffset values instantly cleared the old presentation. The new near-floor
descending coast retains phase, truthful air state and jets-off, with fresh
monotonic observation renewal and bounded gap/deadline. Final current-service
positive-gap browser coast/jump acceptance remains explicitly open; grounded
native slope/camera/shadow captures pass. See client SLOPE-PRESENTATION.md.

Native PC shadow exists: al_mm_kage.prs/kyotu32_marukage was decoded by the
existing Rust reader, original alpha preserved. Client uses native pixels
with explicitly adapted terrain-triangle projection,size and opacity; full
retail shadow-engine internals remain untraced. Native bytes stay ignored.
Server route slice basef0fa6a278b1d1ffc421924d75dbbccc323bfc5e9 changes only
src/service.rs,index.js,tests/protocol.rs,tests/reference/assets.test.js plus
this board: exact chao-shadow.png allowlist, MIME/GET/HEAD/missing/private-path
tests. Formatting, strict scoped Clippy, two Rust asset tests and Node asset
test pass. No gameplay/protocol/save-schema changes or backend rollout. The
local client exact PNG bridge avoids replacing the frozen authority. Preserve
the current canonical balance (last observed142, not the older139/141 notes),
credentials, gear and resident choice.

Client follow-up source is
`02fea862f54c0a5ba96083decffeb7b566ba39aa` on `codex/steady-chao-head`,
associated with client PR #10:
https://github.com/mobdon/ChaoGardenMMOClient/pull/10.
The independently reviewed server route slice is published for review on
`codex/native-chao-shadow-route`, based on
`f0fa6a278b1d1ffc421924d75dbbccc323bfc5e9`. Its only source changes are the
four route/test files above plus this coordination record. Source publication
does not merge, deploy or restart the Rust authority, copy native assets, or
modify a canonical save. The server PR records its exact immutable head.
