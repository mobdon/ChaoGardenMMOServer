# Repository workstream briefs

Updated November 18, 2026. Server-repository coordination copy of the client `docs/WORKSTREAMS.md`, with server-relative entry points adapted below. These are complete implementation briefs, not evidence that every chat has started. Read [BOARD.md](BOARD.md) for verified activation/current implementation status and [PROTOCOL.md](PROTOCOL.md) before shared persistence, authentication, activity or client/server changes. GitHub repositories are mobdon/ChaoGardenMMOClient and mobdon/ChaoGardenMMOServer.

Server workers can read all thirteen briefs and the proposed cross-feature contract here without a client checkout. Paths within a brief refer to its named repository. Client-specific source evidence is available through the [client documentation](https://github.com/mobdon/ChaoGardenMMOClient/tree/main/docs); if an unpublished reference or asset is needed, record that dependency in a PR rather than inventing behavior.

## Coordination rules

- The user's Rust requirement in [AGENTS.md](../../AGENTS.md) applies to every brief: game/shared mechanics and asset readers/converters must be Rust. Shared browser mechanics use the agreed Rust/WASM boundary; presentation/platform adapters call the real Rust protocol. The unaccepted Expo/TypeScript delivery is not a completed Rust task. Read [the Vibe review](VIBE-REVIEW-2026-11-18.md) before extending it.
- Use the server repository's `docs/coordination/BOARD.md`, `PROTOCOL.md` and `WORKSTREAMS.md` for Vibe Coder/repository coordination. This chat does not update the private Vibe website board. Claim a workstream and record its branch, dependencies, evidence and PR there; do not assume an unreported chat is running.
- The original integration chat owns integration, release QA, the playable preview, the client TASKS.md and cross-repository contracts. Repository coordination lives in docs/coordination/ in the server repository; generated release templates do not establish feature completion.
- Use a separate branch/worktree for each implementation chat. Start from the verified published source; do not let multiple chats mutate the same checkout/index. Branches use codex/ prefix. Open reviewable PRs; the integration chat coordinates merges.
- Client root is C:/Users/Admin/Documents/ChatGPT/mmo. Server root is C:/Users/Admin/Documents/ChatGPT/mmo/server. Do not combine the two Git histories.
- Read this file, BOARD.md and PROTOCOL.md first. Server workers also read PHONE.md, WATCH.md and ACTIVITIES.md at the server root. Client workers read client TASKS.md, docs/DESIGN.md and docs/REQUESTS.md; these remain client reference documents. Preserve the two repositories and their history.
- Own only the files listed for your workstream. For shared entry points such as src/scene.js, src/tiny.js, server/src/game.rs or server/src/service.rs, provide a small integration patch and coordinate it before changing the shared file. Do not perform unrelated refactors.
- Server owns position, time, stats, items, scoring, progression, rewards and saves. Never accept client-claimed gains. Do not expose privileged admin logic or credentials in client bundles.
- Original assets, ROMs, saves, converted outputs and reference tools remain ignored/local. Publish tooling, schemas, manifests without embedded copyrighted contents, tests and docs. Preserve source hashes/provenance. Never edit the installed games.
- A Rust reader/converter is a tool port. Converted models/textures are data. Neither proves a complete game-code/mechanics port. Mark verified, adapted and unknown behavior separately.
- Preserve all user ideas in the ledger. Report changed files, tests, limitations, protocol dependencies and the PR/commit. Do not overwrite this chat's live profile or restart its preview.

## Start order

Start Tiny Garden, Android widget, AFK training and device pairing together. The four game-specific Rust asset tracks can run independently after agreeing the common manifest format. Care, movement and competitions consume those outputs. Capacity/deployment measures the integrated service. This original chat integrates throughout.

## Copy-and-paste chat briefs

### 1. Client — Tiny Garden app

```text
Work in ChaoGardenMMOClient on Tiny Garden mobile UI/PWA only. Read docs/WORKSTREAMS.md, TASKS.md and docs/TINY-GARDEN.md first. Own tiny.html, src/tiny.js, src/tiny.css, src/tiny-avatar.js and src/tiny-watch.js. Keep mobile strictly Tiny Garden: care, seven stats, items, AFK training and all five minigames. Preserve native Neutral/Hero/Dark Chaos art and real save/connectivity state. AFK training controls must use the AFK backend contract, never a fake countdown or local stat gains. Validate touch, 390px layout, reconnect and saved results. Use an isolated codex/ branch/worktree, coordinate shared/protocol edits, open a PR and report evidence. Do not restart the live preview.
```

### 2. Client — Android home-screen widget

```text
Work in ChaoGardenMMOClient on the real Android Tiny Garden widget. Read docs/WORKSTREAMS.md and docs/TINY-GARDEN.md first. Android is the selected first platform. Own android/ and widget.html/src/widget.js/src/widget.css; web preview is not an installed widget. Build compact/wide/large layouts with native Chao art, chest/orbit emerald identity, live needs/stats, real training status and one-tap Tiny Garden entry. The widget is specifically Tiny Garden: check on the same Chao and use quick pet/care actions; open the full Tiny Garden app for minigames and AFK training controls. Use authenticated snapshots, server-validated care/training intents and secure revocable device pairing; label cached/stale data. Keep Rust authority on the server; native Android glue may use Kotlin/RemoteViews or Glance. Respect OS background limits; no pretend continuously running widget game. Build/lint and test an actual APK/widget when toolchain/device access exists; report missing prerequisites honestly. Isolated codex/ branch, coordinate API requirements, PR plus proof.
```

### 3. Server — Durable AFK training

```text
Work in ChaoGardenMMOServer on TCG-08/16: production AFK training for the Tiny Garden app. Read PHONE.md, WATCH.md, docs/coordination/WORKSTREAMS.md and docs/coordination/PROTOCOL.md; use client docs/TINY-GARDEN.md as the linked product reference. Reuse tested crates/tiny-garden-core TrainingLedger. Own new src/training.rs, training tests and a versioned protocol proposal; coordinate game.rs/service.rs integration. Implement server-timed bounded jobs, eligibility/cost reservation, one job per Chao, atomic persistent start/cancel/settle, once-only results and restart/reconnect survival. Enforce activity/device ownership; client time/results never grant stats. Publish schemas early to UI/widget/pairing chats. Test forged/replayed requests, duplicate devices, save failure and restart. Isolated codex/ branch, PR with evidence. Preserve live saves; do not restart the preview.
```

### 4. Server — Accounts, pairing and saves

```text
Work in ChaoGardenMMOServer on TCG-06, AUTH-01 and DATA-01. Read docs/coordination/WORKSTREAMS.md, docs/coordination/PROTOCOL.md, PHONE.md and WATCH.md in this server repository. Design and implement short-lived desktop-approved Android pairing, revocable device credentials, one canonical Chao and exclusive activity leases/revisions. Preserve existing local profiles and legitimate rewards through a tested migration. Secrets never enter URLs, QR payloads that outlive pairing, logs or client bundles. Coordinate versioned contracts with AFK/UI/widget chats; provide isolated modules and small agreed game.rs/service.rs hooks. Test expired/replayed pairing, stale inputs, concurrent devices, reconnect, atomic saves and revocation. Isolated codex/ branch, PR with migration and recovery evidence.
```

### 5. Client tools — Rust SA2 + BATTLE assets

```text
Work in ChaoGardenMMOClient on Rust asset tooling for the installed Sonic Adventure 2 + BATTLE sources. Read docs/WORKSTREAMS.md, docs/INVENTORY.md and docs/ASSET-PIPELINE.md. Own asset-tools/crates/sa2/ plus a proposed common asset manifest/schema; coordinate shared asset-tools workspace/core files with other asset chats before editing them. Catalog and port bounded readers/decoders/converters for archives, models, textures/palettes, rigs/motions, stages/collision, UI, audio and Chao Transporter parts. Reuse existing extraction evidence and hashes. Preserve native proportions, original pixels/alpha, stable IDs and provenance. Compare Rust outputs to existing verified outputs; list undecoded fields explicitly. Keep all native inputs/outputs local, publish source/tests only. A full native gameplay port remains separately gated. Isolated codex/ branch and PR with parity evidence.
```

### 6. Client tools — Rust Sonic Advance assets

```text
Work in ChaoGardenMMOClient on Rust asset tooling/catalog for the supplied Sonic Advance ASOE ROM only. Read docs/WORKSTREAMS.md and docs/TINY-GARDEN.md. Own asset-tools/crates/sonic-advance/ and its version-specific docs/tests; coordinate common schemas/workspace files before edits. Identify/hash exact local source; inventory every reachable sprite, palette, tile/map, animation, UI/font, audio, save and Tiny Garden/minigame resource with offsets and decoded/unknown status. Implement bounded Rust extraction/decoding in small verified groups; preserve opaque unknowns and compare outputs against actual source evidence. Reuse existing tiny-garden-core where appropriate, never assume other versions share layouts. No raw ROM/art/saves in Git, no claim that inventory is a full game port. Isolated codex/ branch, PR and evidence.
```

### 7. Client tools — Rust Sonic Advance 2 assets

```text
Work in ChaoGardenMMOClient on Rust asset tooling/catalog for supplied Sonic Advance 2 A2NE. Read docs/WORKSTREAMS.md and docs/TINY-GARDEN.md. Own asset-tools/crates/sonic-advance-2/ and its docs/tests; coordinate common schemas/workspace files. Preserve the already verified ROM and embedded Tiny Garden payload hashes; build from current header/LZ77/save evidence. Catalog/extract sprites, palettes, tiles/maps, motions, UI/audio, transfer/save data and minigame tables with offsets/provenance. Implement bounded Rust readers/converters, corrupted-input tests and parity checks against actual local bytes. Distinguish original behavior from MMO adaptation and unknown fields. All ROMs/converted assets/saves stay local. Isolated codex/ branch, PR with reproducible evidence; full main-game mechanics remain separate.
```

### 8. Client tools — Rust Sonic Pinball Party assets

```text
Work in ChaoGardenMMOClient on Rust asset tooling/catalog for supplied Sonic Pinball Party A3VE. Read docs/WORKSTREAMS.md and docs/TINY-GARDEN.md. Own asset-tools/crates/sonic-pinball-party/ plus its docs/tests; coordinate common schemas/workspace files. Map all native sprite/palette/tile/table/animation/UI/audio/save/transfer and Tiny Garden resources, including Chao Ball, with exact offsets/hashes and unknowns. Port bounded decoders/converters to Rust and compare source-backed outputs; never reuse Advance offsets without evidence. Pinball table physics and gameplay are separately documented port work. Keep original ROM/art/saves local; publish source/tests/manifests only. Isolated codex/ branch, PR with parity evidence.
```

### 9. Server — Chao care, animals and evolution

```text
Work in ChaoGardenMMOServer on authoritative Chao life mechanics. Read docs/coordination/WORKSTREAMS.md and docs/coordination/PROTOCOL.md here; consult client docs/ANIMAL-CARE.md, docs/MECHANICS.md and docs/TINY-GARDEN.md through the linked client repository. Own new care/inventory/evolution modules and tests; coordinate shared host integration. Use audited 21 animals/4 drives/stat rows without inventing undocumented native curves. Implement stage pickup to canonical inventory to care/evolution in small verified slices; preserve negative stat effects and all seven skills. Separate verified original rules from published MMO tuning. Atomic spending/items/needs/effects, retries and activity conflicts must be tested; no client-claimed stats. Provide the Tiny UI contract early. Isolated codex/ branch, PR plus evidence; preserve live saves.
```

### 10. Client — Native movement and 3D presentation

```text
Work in ChaoGardenMMOClient on native movement/presentation. Read docs/WORKSTREAMS.md, docs/ACTIVITY-MOTIONS.md, docs/SHADOW-SKATE.md and TASKS.md. Own new motion/retarget helpers and targeted tests; coordinate src/scene.js hooks with integration. Native Chao animations come first; adapt stage-character actions only for capabilities Chao lacks. Keep normal walk/run below the Air Shoes threshold and genuine Shadow-derived skating above it with speed-scaled cadence. Preserve original Chao rig, body/eye/wing proportions, uniform prop scale, grounding and authoritative movement. Preserve transparent flame/emeralds and opaque bodies with depth-correct bloom. Validate native three forms, shoes/gloves, mounted/air/stop transitions and camera with browser proof. Isolated codex/ branch and PR.
```

### 11. Server — AI races and Karate while away

```text
Work in ChaoGardenMMOServer on COMPETE-01: opt-in AI races/Karate while the owner is away. Read WATCH.md, ACTIVITIES.md, docs/coordination/WORKSTREAMS.md and docs/coordination/PROTOCOL.md here; consult client docs/ACTIVITIES.md and docs/MECHANICS.md through the linked client repository. Own new competition modules/tests and a versioned protocol proposal; coordinate activities.rs/game.rs integration. Reuse audited native rosters/configurations and Chao capabilities. Implement eligibility, scheduling/budgets, stat-scaled server competitors, match simulation/results, durable once-only rewards and spectator/replay state. Resident wandering must never trigger player race/minigame rewards. AFK training remains Tiny Garden's separate job system; enforce conflicts through agreed leases. Prove one race and one Karate slice before broadening. Isolated codex/ branch, PR and anti-replay/restart tests; no live save edits.
```

### 12. Server — Capacity and deployment

```text
Work in ChaoGardenMMOServer on measured capacity and deployment for 100–500 total players. Read docs/coordination/WORKSTREAMS.md, docs/coordination/PROTOCOL.md and README.md here; consult client docs/DESIGN.md through the linked client repository. Own load-test tools, metrics, deployment/config documentation and isolated performance fixes. Measure current tick timing, resident costs, snapshot bytes, slow-client queues, reconnect/save latency and room interest filtering before proposing shards. Preserve one owner for each Chao and atomic rewards; no capacity claim without reproducible results. Plan HTTPS/WSS, private server/admin boundaries, accounts/store/backups and Android reachability. Prepare reviewable deployment artifacts; coordinate shared runtime changes and public deployment with integration. Isolated codex/ branch, PR with measured baselines/limits. Do not expose this local bootstrap server or purchase services.
```

### 13. This original chat — Integration and release QA

```text
Act as integration/release lead for ChaoGardenMMOClient and ChaoGardenMMOServer. Keep the client TASKS.md/design/request docs and the server docs/coordination/BOARD.md, WORKSTREAMS.md and PROTOCOL.md current. Review the other workstreams' protocol and shared-file changes, integrate compatible PRs in order, run appropriate client/server and cross-device tests, preserve the canonical owner and durable saves, and maintain one healthy playable preview. Verify original-vs-adapted claims, asset provenance/private boundaries, Android widget status and exact outstanding work. Coordinate release snapshots in both independent source repositories. Never claim an asset conversion, mocked UI, Rust core test or web widget preview is a complete native game/app. Keep this chat focused on integration rather than taking over every implementation lane.
```
