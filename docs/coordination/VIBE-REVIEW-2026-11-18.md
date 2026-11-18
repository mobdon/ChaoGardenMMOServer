# Vibe batch review — November 18, 2026

**Disposition: UNACCEPTED.** The user reaffirmed Rust as a project requirement. The new Expo/TypeScript phone app does not fulfill the Rust game/shared-mechanics work. The Rust server changes also contain confirmed compilation failures. Keep feature acceptance and deployment blocked; the commits are already on upstream main, which is separate from accepting or running them.

Review snapshot: `5ec51f94ad24135a41d6a0c2320d155cccd486f3`, against the previously published Rust baseline `a8188a3030eb176ec4c3b7156f93410da5bb6f09`. The four Vibe deliveries change 97 paths. This is a finite review of this snapshot; later commits need a separate review.

**Later incoming batch:** PR #7 advanced upstream to `282f6abd1bdb75d0477622f8952667073dc2b751`, adding/changing 59 paths after this review snapshot. Its `apps/api` NestJS/Prisma authority and `packages/simulation` TypeScript gameplay/growth/training conflict with the mandatory Rust scope and are unaccepted. Detailed source findings belong to a separate addendum; the 25 findings below do not claim to review those later 59 paths. The Rust instructions are being published separately so further tasks receive this constraint immediately.

| Delivery | Commit | Review result |
| --- | --- | --- |
| PR #1 — release/operations documents | `b525d8b` | Draft scaffolding; inaccurate current-feature claims corrected in the documentation follow-up. No completed store/policy/feature approvals. |
| PR #2 — infrastructure/deployment | `826c0f2` | Blocked: Rust compilation, canonical storage/backup, migration and infrastructure errors. |
| PR #3 — E2E/load testing | `7476c94` | Blocked: setup cannot start the real service, failed suites can exit successfully, and key assertions/workloads do not prove the claimed behavior. |
| PR #4 — mobile UI/contracts | `5ec51f9` | Unaccepted technology/delivery boundary, plus launch, protocol, session and state defects. Not a working Android Tiny Garden/widget. |

The existing local preview remains the separately copied, tested Rust runtime with SHA-256 `51A07F9C382ECB70785654297180349DD2E34F677D5921818EA2B9A6ECEC3100`. It was not replaced or migrated during review. Client movement source was independently published as `279a7b7`; that publication does not accept this Vibe batch.

## Required architecture and acceptance

`../../AGENTS.md` records the Rust requirement at the repository entry point. Implement game/shared mechanics and readers/converters in Rust. Reuse Rust Tiny crates and the authoritative service; use Rust/WASM for shared browser mechanics as appropriate. Platform/rendering adapters must speak the agreed real protocol. Client/Android presentation belongs in the separate client repository.

Do not spend the next tasks extending the invented Expo backend contract. First agree the canonical Rust schema, identity/observation/activity ownership, atomic save boundary and adapter responsibilities in [PROTOCOL.md](PROTOCOL.md). Keep native assets and the user-selected Chao scope; original placeholder art is not an approved replacement for that scope.

Priorities below use P1 for a blocking functional/data-integrity defect and P2 for a significant reliability or verification defect. Static findings are identified separately from executed reproductions.

## Server and infrastructure findings

| ID | Priority / location | Trigger, consequence and repair gate |
| --- | --- | --- |
| VIBE-01 | P1 — `src/service.rs:24,28,83`; `src/metrics.rs:92–95` | Server source cannot compile. `Instant` is imported twice; `Config::local` initializes an undeclared `metrics` field; two closures retain overlapping mutable borrows of `out`. `cargo test --locked` and strict Clippy reproduce E0252. Direct compilation of unchanged `metrics.rs` reproduces E0499; the nonexistent field is also a source-level error. Resolve all three and pass the existing Rust suites before integration. |
| VIBE-02 | P1 — `infrastructure/terraform/modules/api/main.tf:59`; Dockerfile `:16`; `src/service.rs:101–103,177–193`; `scripts/backup.sh:20–26` | Production defines two independent authority machines, without a durable mount or implemented shared store. Rust reads/writes only local `/data/profiles.json`; PostgreSQL/Redis configuration is unused by gameplay. Requests across replicas see different profiles/wallets, and replacing ephemeral machines loses their state. The backup script dumps the unused database instead of the canonical save. Retain one authority with durable storage and verified JSON backup/restore, or implement actual shared ownership/storage first. Prove credential/state continuity across restart/replicas and restore. |
| VIBE-03 | P1 — `src/service.rs:203–216` | Release migration rewrites the canonical profile with `fs::write`, which truncates before writing. A crash/write failure can destroy the save; running it alongside the old writer can overwrite newer progress. Use a tested migration/backup and exclusive transaction boundary with atomic replacement/fsync, preserving the existing reward/receipt rules. A schema marker alone is not durability. |
| VIBE-04 | P1 — `infrastructure/terraform/modules/data/main.tf:44,49` | Terraform references undeclared `cloudflare_account_id` and `cloudflare_zone_id`. No declarations exist in this module/tree, so the module cannot validate with these references. Declare/pass required inputs and validate with the actual pinned providers. Provider-resource compatibility remains unverified; it was not assumed correct. |
| VIBE-05 | P1 — `infrastructure/terraform/envs/staging/main.tf:15`; production `:55` | Migration command names `/usr/local/bin/chao-server`; the image installs `/usr/local/bin/chao-game-server`. Configured migrations cannot start. Verify the built executable and release command in an isolated image. |
| VIBE-06 | P2 — `src/service.rs:329` | Readiness only calls `create_dir_all` on the existing data directory. It can return healthy while canonical saves fail on a read-only/full volume. Test the actual durable write boundary with isolated failure injection and report not-ready when saves cannot commit. |
| VIBE-07 | P2 — `scripts/release.sh:4,19,30–33` | `set -e` exits immediately when deployment fails, before the advertised rollback branch. Handle the deployment status explicitly and prove rollback on that failure path with a disposable/mock CLI. No rollout was executed in review. |
| VIBE-08 | P2 — `src/metrics.rs` `record_event_drop`; `src/service.rs:668,692,746` | Event-drop accounting is never called; lagged events are discarded. Persistence failures are counted on only two paths, missing autosave/final-save/Watch failures. Alerts can miss the failures they claim to observe. Instrument the common persistence/event boundary and test counter changes without changing reward semantics. |

## E2E and load findings

| ID | Priority / location | Trigger, consequence and repair gate |
| --- | --- | --- |
| VIBE-09 | P1 — `e2e/fixtures/seed.js:112–119` | Writes `{profiles:[...]}` while the real loader requires a raw `Vec<Value>`. It also writes a 21×21 terrain with an empty heights array, independently rejected by terrain validation. The service cannot load these fixtures. Generate real canonical synthetic profiles and valid terrain, then load them through the actual Rust loader. Unknown invented inventory/Chao fields do not prove those systems work. |
| VIBE-10 | P1 — `e2e/scripts/run-integration.sh:49–59` | `TOKEN_FILE` is not exported; assignments after `node -e` are arguments, not environment variables. Both token-writer attempts see `process.env.TOKEN_FILE` unset and fail before server startup. Pass environment variables before the command or use explicit arguments; prove a clean invocation without ambient exports. |
| VIBE-11 | P1 — `e2e/scripts/run-integration.sh:75–78,86` | Tests are piped into `tee` without `pipefail`/captured test status. A failed Node suite can leave `FAIL=0` and exit successfully. Force a failing test and verify the complete runner returns nonzero. |
| VIBE-12 | P1 — `e2e/integration/contract.test.js:126` | The save-durability test resolves successfully on timeout even when no save acknowledgment arrives. An isolated ephemeral WebSocket mock received the real save request, sent zero acknowledgments, and the actual test exited 0. Timeout must reject; check a committed receipt/profile and restart durability. |
| VIBE-13 | P1 — `e2e/integration/api.test.js:71–84` | The authority test sends `resumeOnly` without a body token, accepts 404 and checks `status.me`, which this service does not return. An isolated mock accepted the forged balance 999999 and the test still exited 0. Authenticate via the real protocol, send an actual forbidden mutation, and compare canonical before/after state. |
| VIBE-14 | P1 — `e2e/k6/care-command-burst.js:39–45`; reconnect `:39–45`; fanout `:50–54`; websocket `:31–33` | Workloads do not exercise their named features: care/reconnect omit the token and receive 404; fanout sends an unsupported `broadcastProbe`; WS drop detection expects nonexistent `seq`. Counting response codes cannot prove duplicate-free economy or fanout. Implement real authorized intents and observable server acknowledgments/state; then measure 100–500 players. No capacity result is established here. |
| VIBE-15 | P2 — `e2e/scripts/run-load.sh:34`; `e2e/integration/contract.test.js:57–80` | Load runner invokes missing `verify-thresholds.js`. Contract checks expect invented snapshot versions/fields from the health endpoint, and one fallback consumes the response body twice. Pin the actual Rust wire contract and make threshold verification executable. |
| VIBE-16 | P2 — `e2e/maestro/scripts/probe-status.js:17,27–29`; `run-e2e.sh:40` | A missing XHR produces `{checked:false}` and a log instead of failure; the runner has no promised final authoritative side-effect check. The isolated VM reproduced this no-op. Required care/training/inventory assertions must fail if not performed or not verifiable. |
| VIBE-17 | P2 — `e2e/scripts/run-integration.sh:24–28,80–82`; `run-e2e.sh:29–30,54` | Cleanup kills the authority only on the normal tail; failure can leave a process running after its disposable data is removed. Several ports are fixed despite parallel-safe claims. Always stop/wait for the exact test child in the cleanup trap before removing state; allocate actual isolated ports. |

## Mobile delivery findings

These remain evidence about the unaccepted scaffold, not a direction to keep building game mechanics in TypeScript.

| ID | Priority / location | Trigger, consequence and repair gate |
| --- | --- | --- |
| VIBE-18 | P1 — `apps/mobile/package.json:5`; `src/components/GardenCanvas.tsx:66,100`; `src/api/appClient.ts:4,10` | App cannot launch as delivered: entry `index.js` is absent, `styles` is declared twice in one scope, and `ConnectionBannerModel` is imported/constructed but never exported. A platform adapter needs a real build/entry smoke before acceptance. Full Expo build was not run because its dependencies/toolchain are absent. |
| VIBE-19 | P1 — `packages/contracts/src/index.ts:118,133–142` | Mobile requires session `{token,refreshToken,expiresAt,profile}` but Rust returns `{token,player}`. It requests absent REST snapshot/care/training/refresh endpoints; actual gameplay uses WebSocket snapshots/intents. An isolated injected-fetch reproduction rejects the authentic service-shaped session as `Invalid session payload`. One canonical Rust protocol must drive adapters and contract tests. |
| VIBE-20 | P1 — `apps/mobile/src/query/hooks.ts:98`; `src/api/client.ts:185–188` | Adoption passes an idempotency string into the client's second `AbortSignal` parameter. Isolated reproduction raises `signal?.addEventListener is not a function`. Align generated call sites with the actual typed function signature and prove the full authorized adoption flow after architecture agreement. |
| VIBE-21 | P1 — `apps/mobile/app/index.tsx:28`; `onboarding/index.tsx:58`; `auth/login.tsx:29` | Guest flow reaches adoption without establishing a session. Successful sign-in routes to adoption with an empty draft, disabling Adopt and preventing normal return to the existing garden. Explicitly test first-use and returning canonical-profile entry separately. |
| VIBE-22 | P1 — `apps/mobile/src/api/tokenStorage.ts:5,15–16` | Credentials live only in a module variable. Android process death loses the guest's credential and access to that save. There is no implemented secure persistent adapter. Persist revocable device credentials through the agreed Rust identity/pairing protocol and prove process-death resume of the same Chao. |
| VIBE-23 | P2 — `apps/mobile/src/query/hooks.ts:85–123` | Successful care/item/training responses are discarded without cache update/invalidation; no polling interval supplies eventual updates. UI can keep showing old needs/inventory/training. Reconcile authoritative committed results and verify refresh/reconnect behavior. |
| VIBE-24 | P2 — `apps/mobile/app/(tabs)/settings.tsx:37`; `src/query/hooks.ts:9` | Signout clears tokens while retaining the singleton query cache and UI drafts. Account-independent snapshot keys can show the prior account's garden during a later refetch. Clear private caches/drafts and key server state by authenticated identity. |
| VIBE-25 | P2 — `apps/mobile/src/api/client.ts:134–138` | Repeated resource 401 responses after successful refresh trigger an unbounded refresh/replay loop. Injected-fetch reproduction reached five resource attempts/four refreshes before deliberate termination. Bound reauthentication and fail explicitly. |

## Repair task order

1. **RUST-01:** agree the Rust/shared-WASM/platform boundary and real wire schemas. Keep the generated Expo contract unaccepted; put client adapters in the client repo. Link every implementation task to the canonical contract.
2. **BUILD-01:** repair Rust compilation and run formatting, strict Clippy, existing service tests and the independent Tiny crates. Preserve the working runtime until a later tested rollout.
3. **STORE-01:** choose one authoritative persistent store/ownership model; repair migration, backup/restore, readiness and deployment scripts. Prove single-writer safety and once-only Tiny receipts with copied/synthetic data.
4. **TEST-01:** repair fixture/runner execution and fail-closed exit status. Replace false positives with actual durable-state and rejection assertions; test their negative controls deliberately.
5. **TINY-01:** connect Rust-owned pairing/care/AFK jobs and compatible desktop/Tiny/widget adapters. Prove process-death resume, current stats, revocation, stale commands and saved results. AFK service and installed Android widget remain unfinished.
6. **LOAD-01:** implement real authenticated workload scenarios and enforce measured thresholds. Only then make a 100–500-player capacity claim or review a deployment candidate.

## Validation and limits

- Executed: Cargo test/Clippy attempts demonstrating server compilation failure; independent unchanged metrics compilation; isolated injected-fetch mobile reproductions; ephemeral loopback HTTP/WS negative controls demonstrating false-green tests; isolated VM of the no-op Maestro probe. Source paths and actual service schemas were cross-checked.
- Not executed: native Android/Expo build, real k6/Maestro load/device runs, Terraform/provider validation, Docker build or deployment. Missing tools were not installed for review. Static findings do not claim those checks ran.
- One separate Node test-filter invocation unexpectedly ran the API test file against its default preview port. It sent status reads and invalid/absent-token `resumeOnly`, rejected-origin and oversized requests, with no valid token/WS/create request. This was reported and further runtime checks stopped. The saved profile was checked afterward: one canonical Chao named Chaos, 138 rings. Do not describe every review command as isolated.
- No new Vibe runtime was deployed, no live migration was executed, and no source bug was silently fixed/reverted during this review. Documentation corrections and this report are separate from accepting feature code.
- An alleged Docker `include_str!` catalog compile failure was investigated and rejected: current code loads catalogs at runtime. It is not a finding.
