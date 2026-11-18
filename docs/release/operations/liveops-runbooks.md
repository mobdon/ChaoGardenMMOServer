# Live-Ops Runbooks — Chao Garden MMO

Draft operating procedures, not proof of a deployed production service or
implemented operator tools. The current local authority implements Tiny Garden
minigames and resident Watch; durable AFK training jobs, scheduled away-time
competitions, moderation/correction tools and push campaigns remain separate
implementation work. Confirm the relevant feature and evidence before using
its proposed procedure.

Each runbook names the actual code/config touched. Never edit player
`profiles.json` by hand outside the documented procedures; the authority
(`src/service.rs`, Node reference `game.js`) performs atomic writes only.

## 1. Item catalog / shop price changes

- **Source:** fixed-price shopping catalog in server code (`src/game.rs`
  gameplay constants; Node reference `constants.js`). Cosmetic ownership is
  validated server-side.
- **Procedure:**
  1. Draft change in a branch; update catalog constants and tests
     (`tests/game.rs`, `tests/reference/game.test.js`).
  2. `cargo test --locked` and `npm test` green on CI.
  3. Staging soak ≥ 24 h with the change; verify purchase, equip/unequip
     (`equip-owner-gear` / `unequip-owner-gear` for `gear.footwear`,
     `gear.hands`), and refund path §5.
  4. Announce in-app + support channel (§6), deploy to production server,
     confirm `/api/status` and a live purchase.
- **Rollback:** revert constants and redeploy; already-purchased cosmetics are
  owned state and must not be revoked — compensate via §5.

## 2. Simulation / training version rollout

- **Current evidence:** `crates/tiny-garden-core/src/training.rs` contains a
  tested standalone TrainingLedger adaptation with frozen policy, a maximum
  8-hour duration, monotonic job IDs and once-only settle/cancel transitions.
  The production server does not depend on that crate yet. Its training UI,
  durable job storage/settlement and device/activity ownership hooks are not
  connected. `src/phone.rs` hosts the five Tiny minigame adaptations;
  `src/watch.rs` and Node `watch.js` implement the resident idle/wander/rest
  routine, which awards no passive stats or rings. Those are not AFK training.
- **Validation now:** run the independent crate suites explicitly from the
  server repository; the main package test does not include them automatically:

  ```sh
  cargo test --locked
  cargo test --manifest-path crates/tiny-garden-core/Cargo.toml --locked
  cargo test --manifest-path crates/tiny-garden-games/Cargo.toml --locked
  ```

- **Proposed AFK rollout gate:** first implement the agreed training protocol,
  authorization/lease checks, one-job-per-Chao limits, cost reservation and an
  atomic job-plus-profile save. Prove restart recovery, duplicate requests,
  failed saves and once-only settlement against copied test profiles. Preserve
  the standalone policy's bounds unless an explicit balance change is agreed.
  Only then define staging comparisons, rollout timing, metrics and rollback
  for the actual integrated implementation.
- **Current restart behavior:** explicit `save` acknowledgement and clean
  `CHAO_SHUTDOWN_STDIN` shutdown flush supported progression. Resident Watch
  restores a fresh routine without service-downtime catchup. Do not claim that
  live AFK training jobs resume until the adapter and recovery tests exist.
- **Constraint:** 62 extracted race configurations stay disabled until routes
  and obstacles are verified (`ACTIVITIES.md`); enabling any requires verified
  activity terrain under `data/activities/terrain/{worldId}.json`.

## 3. Competition scheduling (races, karate, mini-games)

- **Source:** race/karate catalogs (`data/activities/{races,karate}.json`
  provenance; authority in `src/activities.rs`); mini-games from
  `crates/tiny-garden-games` (memory, rps, search, bounce, ball) via
  `PHONE.md` protocol.
- **Procedure:**
  1. Schedule published ≥ 7 days ahead with start/end UTC times and rewards.
  2. Rewards must be bounded in-game items/rings only (no cash prizes —
     `compliance/age-rating.md`).
  3. Operator enables the competition entry in the activity configuration and
     verifies via `/api/activities` detailed catalog output.
  4. During the event, monitor participation and `posts` WS error rates.
  5. After close, settle rewards through the authority (never by editing
     profiles directly); publish results.
- **Cancellation:** disable the entry, announce, compensate §5 if needed.

## 4. Emergency feature disablement (kill switch)

Triggers: exploitable dupe, award overflow, crash loop, abuse wave.
- **Immediate:** stop traffic at the reverse proxy for the affected endpoint
  (e.g., `/api/session`, `/ws`) or the whole garden.
- Clean shutdown: launcher `shutdown` line (`CHAO_SHUTDOWN_STDIN=1`) or
  SIGTERM flushes pending progression (`README.md`).
- Feature-level switches (training, activities, shopping, widget sync) are
  added as config-gated flags in the server configuration block read by
  `Config::from_env`; document each flag here when added:
  - `[ ] CHAO_DISABLE_TRAINING` — proposed; add to `.env.example` when
    implemented.
  - `[ ] CHAO_DISABLE_ACTIVITIES` — proposed.
  - `[ ] CHAO_DISABLE_SHOP` — proposed.
- Communicate via status page + support email; publish a post-incident report
  within 5 business days.
- **Rollback:** redeploy previous tagged binary after cause fixed.

## 5. Refunds and balance/ring corrections

- Store-side refunds (Apple/Google) handled per store console; grant or revoke
  cosmetic ownership through a support tool that uses the authority's own
  write path — **never** manual JSON edits.
- Balance corrections (rings, stats): operator submits correction request with
  evidence; second admin confirms; correction logged in the admin action log
  (`operations/moderation.md`).
- Duplication exploits: freeze affected accounts (moderation action), correct
  balances to the last verified snapshot, communicate outcome.
- All corrections are auditable: record actor, before/after values, reason.

## 6. Notification campaigns

- **Scope:** opt-in push notifications (races opening, garden events); no
  marketing without consent; store-compliant frequency.
- **Procedure:**
  1. Draft copy → trademark check (`compliance/review-gate.md`) and
     accessibility review (plain language, no all-caps).
  2. Segment = opted-in profiles on target servers; honor quiet hours in
     recipients' local time.
  3. Send to 5% pilot first; check opt-out/uninstall spike; then full send.
  4. Log campaign ID, copy, segment size, opt-outs.
- **Kill:** halt send job at provider; note in campaign log.
