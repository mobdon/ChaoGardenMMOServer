# Live-Operations Runbooks — Chao Garden MMO

Owner: Live-Ops / Engineering on-call. Escalation path: see
[moderation.md](moderation.md) for admin access control and
[release-checklist.md](release-checklist.md) for release gating.

All production changes go through the existing scripts in `scripts/`:

| Script | Purpose |
| --- | --- |
| `scripts/migrate.sh <image> <env>` | Runs `chao-game-server migrate` as a release command before the new image rolls; aborts on failure |
| `scripts/release.sh <image> <env>` | Rolling release onto `chao-api-staging` / `chao-api-production` with a post-release health gate that auto-rolls back |
| `scripts/rollback.sh <image> <env>` | Manual rollback to a known-good image + smoke test |
| `scripts/smoke-test.sh [env]` | `/livez`, `/readyz`, `/api/status`, `/api/activities`, `/metrics`, content-type and origin-guard checks |
| `scripts/backup.sh <env>` | Nightly `pg_dump` to object storage with retention pruning (7d staging / 30d prod); restore drill per `docs/runbooks/database-restore.md` |

Alert rules live in `infrastructure/monitoring/alerts.yml`
(`chao_http_errors_total`, `chao_persist_failures_total`, `chao_ws_connections`,
`chao_save_requests_total`, economy/event counters). Environments are
provisioned by Terraform under `infrastructure/terraform/envs/`.

---

## 1. Item catalog / balance changes

Balance data is owned by the server crate (`chao-game-server`, gameplay in
`src/game.rs`, `src/activities.rs`) plus the shared logic crates
`crates/tiny-garden-core` and `crates/tiny-garden-games`. Activity definitions
surfaced to clients come from `/api/activities` (`src/activities.rs`).

1. Propose the change in a PR touching the Rust source/constants **and** a
   test in `tests/` (e.g. `tests/activities.rs`, `tests/game.rs`) asserting
   the old/new reward or stat behavior. Any parallel client reference change
   must keep `npm test` green (the Node suite in `game.js` / `constants.js` /
   `activities.js` is the parity oracle).
2. Verify locally: `cargo test --locked` and `npm test`.
3. Staging first: build the image, then
   `scripts/migrate.sh <staging-image> staging` followed by
   `scripts/release.sh <staging-image> staging`, then `scripts/smoke-test.sh staging`.
4. If economy-affecting, announce in `#chao-liveops` at least 24h ahead.
5. Production: `scripts/migrate.sh <prod-image> production`, then
   `scripts/release.sh <prod-image> production`. The release script's health
   gate (`flyctl status ... healthy`) auto-rolls-back to the previous image on
   failure; escalate to `scripts/rollback.sh` if needed.
6. Watch `chao_economy_ring_rewards_total` and `chao_event_drops_total` on
   `$BASE/metrics` for 24h after rollout; record the change in
   `docs/runbooks/changelog.md` (date, PR, reviewer).

**Rollback:** `scripts/rollback.sh <previous-image> production` then
`scripts/smoke-test.sh production`. Server persistence is checkpoint-based
(atomic profile writes in `src/game.rs`), so reverting the image restores prior
behavior without corrupting saved profiles.

## 2. Simulation / gameplay logic rollout

Creature simulation logic is split between the server crate and
`crates/tiny-garden-core` (deterministic core). Golden behaviors are pinned by
unit tests in `crates/tiny-garden-core/tests/` and integration tests in
`tests/`.

1. Every logic change keeps `cargo test --locked` green, including the
   deterministic-core tests; if client-visible outcomes change, update the
   Node reference suite and confirm `npm test`.
2. Deploy staging (`scripts/migrate.sh` + `scripts/release.sh` +
   `scripts/smoke-test.sh staging`).
3. Production rollout is **staged by cohort**: internal accounts first, then
   10% → 100% across two days. Use Fly's machine/traffic controls to stage the
   roll; monitor `chao_http_errors_total`, `chao_persist_failures_total`, and
   `chao_ws_connects_total` against the thresholds in
   `infrastructure/monitoring/alerts.yml`.
4. Rollback = `scripts/rollback.sh <previous-image> production`; old profile
   checkpoints remain valid because the format is forward-compatible
   (unknown fields ignored on load).

## 3. Competition / event scheduling

Activities (races, bouts, event drops) are implemented server-side in
`src/activities.rs` with counters `chao_event_drops_total` /
`chao_economy_ring_rewards_total`.

1. Schedule at least one week ahead; validate scoring math changes with
   `tests/activities.rs` and `tests/mech.rs` before announcing.
2. Dry-run on staging: seed two profiles, run the full activity loop through
   `/api/session` → activity command → settle, and confirm awards once-only.
3. Announce in-app; record in `docs/runbooks/changelog.md`.
4. If scoring is wrong mid-event: redeploy the previous image
   (`scripts/rollback.sh`) — scores settle from checkpoints, so re-running
   after the fix does not double-award. Post an in-app correction notice.

## 4. Emergency feature disablement (kill switches)

There is no remote flag service in the MVP, so disablement is a **config-only
redeploy of the current image** (no code change, minutes not hours):

1. Declare the incident in `#chao-incidents` (severity, affected surface).
2. Restrict the surface at the edge/environment level:
   - Block the offending route (e.g. `/ws`, `/api/activities`) at the proxy or
     via `GAME_PORT`/firewall in `infrastructure/terraform/envs/production`.
   - Narrow `CHAO_ALLOWED_ORIGINS` if the abuse is browser-origin based.
   - Reduce `CHAO_OWNER_BOOTSTRAP` / admin grants if privilege abuse is
     involved (see `operations/moderation.md` §Admin access).
3. Redeploy the same image: `scripts/release.sh <current-image> production`
   (config change only), verify with `scripts/smoke-test.sh production`.
4. If the data layer is at risk, stop writes entirely by scaling machines to
   zero after taking an immediate `scripts/backup.sh production` snapshot.
5. Post an incident note; fix forward via a normal release
   (`scripts/migrate.sh` + `scripts/release.sh`).

## 5. Refunds & balance corrections

Profiles are persisted by the server (`src/game.rs` atomic writes); admin
authority is a profile-level flag (`"admin": true`) granted via
`CHAO_OWNER_BOOTSTRAP` at bootstrap only. See `operations/moderation.md` for
access controls.

1. Collect: player ID, item/amount, reason, evidence (ticket or report link).
2. Two-person review: requester + one admin.
3. Correction options, in order of preference:
   - Ask the player to re-run the affected action after a server fix
     (checkpoint settle is once-only, so no double award).
   - For restoring lost progress: redeploy a corrected image and have the
     player reconnect (server re-advances from their last checkpoint).
   - For real-money refunds: process in the store console (Apple
     *Report a Problem*, Google Play order refund) and record the transaction
     ID in the support log. Never edit `data/` files in production by hand;
     if a manual profile edit is unavoidable, take a `scripts/backup.sh`
     snapshot first and perform the edit on staging-verified tooling with a
     second admin present.
4. Notify the player in-app and log the outcome in the support ticket.

## 6. Notification campaigns

1. Campaign copy is reviewed against `compliance/widget-disclosure.md` (no
   dark patterns, no false urgency) and `compliance/privacy-data-inventory.md`
   (notification opt-in respected).
2. Sending is performed by the app/client layer from server-provided state
   (`/api/status`, `/api/activities`); the server itself sends no push in the
   MVP, so a "campaign" is a coordinated client release or widget snapshot
   change.
3. Rate limit: at most one re-engagement notification per 72h per player;
   quiet hours enforced client-side.
4. Any server-side change enabling sends must ship behind the §4 disablement
   procedure and be smoke-tested on staging first.

---

## Escalation summary

- Health/persistence alerts: `infrastructure/monitoring/alerts.yml`.
- Deploy failure: `scripts/release.sh` auto-rolls back; manual fallback
  `scripts/rollback.sh`.
- Data loss: restore per `docs/runbooks/database-restore.md`; nightly dumps via
  `scripts/backup.sh`.
- Abuse/security incidents: `operations/moderation.md`.
