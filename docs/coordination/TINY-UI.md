# Tiny Garden UI contract and acceptance handoff

November 18, 2026. Workstream 1 is active in client branch
`codex/tiny-garden-mobile`, originally based on `af74a8d`, rebased onto
published client `435f6a3`. Client draft [PR #5](https://github.com/mobdon/ChaoGardenMMOClient/pull/5)
contains source `f4f2f05f28b7964e2ed295d80b1e570f6d08edbe`; its review
record is `docs/TINY-UI-REVIEW.md`. This server change records coordination
only; it changes no authority, store, profile, native asset route or runtime.

The browser adds Garden, Stats, Training, Games and Items screens. All game
rules, collisions, care effects, inventory, time, rewards and saves stay in
Rust. JavaScript renders canonical state and sends bounded intents; it is not
a replacement simulation or a claimed Rust/WASM game-system port. The
unaccepted Expo/NestJS/TypeScript deliveries are not dependencies.

## Contract dependencies and rollout gates

| Workstream | Tiny Garden consumer state | Required completion evidence |
| --- | --- | --- |
| 3: AFK training | No advertised durable production jobs exist. The screen shows unavailable without options, timers or gains. The client's `TINY-TRAINING-UI.md` is a disabled proposal, not an agreed or implemented service API. No separate AFK chat/PR was verified during this work. | Publish the Rust request/snapshot/receipt contract and connect atomic cost/job/result persistence to ownership. Prove restart, cancel/claim replay, failed saves and duplicate-device exclusion through the actual service before enabling UI controls. |
| 4: Accounts/pairing | Optional adapter consumes the exact isolated Rust `docs/ACCOUNTS-V2.md`: bearer HTTP, first-frame WS authentication, canonical identity, activity leases and sequence ACKs. It never falls back to legacy credential URLs after failed discovery. The old development path is loopback-only. | Reconcile the implemented flat envelope with `PROTOCOL.md` review requirements; coordinate desktop/Tiny/widget migration, passive observation, explicit ownership transitions, revoke/stale commands, and durable receipts before merge/rollout. An advertised capability plus fixture tests alone does not establish production acceptance. |
| 9: Care/inventory | Owner-only `snapshot.care`, exact `adapted-care-v1` version, quantities and eligibility are rendered. Compatible local use/pet preserves the same UUID/original care revision on manual retry; canonical snapshots alone change inventory/stats. | Care and accounts currently both use top-level `expectedRevision` for different state. Account v2 care is disabled until a central agreed envelope protects the private care payload and durable retry identity. Prove the combined Rust service; do not translate into invented client endpoints or apply local deltas. |

Opening the stats/garden viewer acquires no new activity lease. Explicit game
or Watch actions request their activity; a foreign lease requires an explicit
control request and server approval. Remaining passive-socket ownership and
all-client migration gates in `PROTOCOL.md` are preserved, not marked done.

## Recorded evidence and limits

- Native Neutral/Hero/Dark Chaos atlases and Dark-only flame have source tests
  and browser screenshots; original rigs/atlases are unchanged and assets stay
  local. Screenshots are presentation evidence, not native mechanics parity.
- Chrome touch fixtures cover all five games, canonical wallet/stats, strict
  durable save display, reconnect's first-snapshot gate, foreign leases and
  320/390/430/844-pixel layouts. They never proxy to the preview.
- After the client rebase, build passed and 86 isolated tests passed with no
  failures/skips (62 source, 20 touch/browser and four production-PWA cases).
  The unchanged private-source-boundary test separately passed in the primary
  client checkout. A `.local`-nested worktree matches Vite's existing deny glob;
  the production file boundary was not weakened to run tests there.
- `scripts/verify-tiny-live.mjs` starts a disposable Rust process on an
  ephemeral port with a new data directory and owner bootstrap disabled.
  Against runtime SHA-256
  `9a717fee289a2ad0de05404939d327079b81727a0b1474c398999e901f7f60be`,
  all five games started/stopped without cancellation rewards. Memory earned
  90 saved rings from public observations; the same Chao, wallet and one receipt
  survived reconnect and a restart of that test process.
- No live save/profile/credential, installed game or running preview was read
  or migrated by that acceptance script. Existing preview/runtime rollout stays
  with integration. The test's own temporary process is the only restarted
  service.
- The final runner also used the real Tiny account adapter against an isolated
  Rust accounts-v2 executable. Both final legacy/v2 runs earned 82 saved rings
  and retained one receipt after reconnect/restart. V2 executable SHA-256:
  `4317b6d1e887b71274e484ceff3245da86cc86fa74f2d9155ad1692caeef46e1`;
  observed accounts source was clean at
  `56af22dece8551376f4c8b9a1882acecfff02bf1`. This proves that tested adapter
  path against isolated source; it does not remove the all-client migration,
  passive-observer, care-envelope or production rollout gates.
- Durable AFK service, integrated v2 care, production identity rollout, physical
  phone installation and full earned/failure rounds of the other four games
  remain open. A PWA shell or pure Rust training ledger is not those features.

Keep client and server commits/PRs separate. Preserve later upstream history,
including the native locomotion asset route, when combining older isolated
accounts/care source. The integration chat retains final merge and rollout
ownership.
