# Live-Ops Runbooks — Chao Garden MMO

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

- **Source:** server-timed idle training policy in
  `crates/tiny-garden-core/src/training.rs` (frozen server policy, max 8-hour
  duration, monotonic job IDs, once-only settle/cancel) surfaced by
  `src/phone.rs` / Node `watch.js`.
- **Procedure:**
  1. Policy changes must preserve: bounded awards, max 8 h duration, stored
     server timestamps, once-only transitions. Update the frozen-policy tests.
  2. Run `cargo test --locked` (incl. `crates/tiny-garden-core/tests` and
     `crates/tiny-garden-games/tests/public_rounds.rs`).
  3. Shadow-run on staging with a copied profile data dir; compare awards.
  4. Roll out during a low-activity window; save first (`save` event ack /
     `CHAO_SHUTDOWN_STDIN` clean stop) before restart so no job is stranded.
  5. Watch settle/cancel error rates and award totals for 24 h.
- **Rollback:** redeploy previous binary (`Cargo.lock` pinned); in-flight
  training jobs resume from stored server timestamps — do not rewrite job rows.
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
