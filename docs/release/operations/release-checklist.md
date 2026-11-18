# Release Checklist — Chao Garden MMO (Full App + Widget)

Draft for review, not a record of shipped capabilities or completed sign-offs.
Use [the coordination board](../../coordination/BOARD.md) for current status.
Evidence links (test run IDs, screenshots, logs) go in the release ticket.
Reference implementations: `.github/workflows/check.yml` CI,
`cargo test --locked`, `npm test`.

## 0. Preconditions

- [ ] `compliance/review-gate.md` signed (blocking).
- [ ] `licensing/asset-inventory.md` updated at the release tag.
- [ ] CI green: `cargo test --locked` (root + workspace crates) and `npm test`.
- [ ] Version/tag recorded; `Cargo.lock` and `package-lock.json` committed.

## 1. Server / full app

- [ ] `CHAO_OWNER_BOOTSTRAP` disabled in production config; verify via
      `/api/status` and by confirming no new owner grant on prod data dir.
- [ ] `CHAO_ALLOWED_ORIGINS` set to production browser origins only.
- [ ] WSS over valid TLS via reverse proxy; loopback-only listener unless
      intentionally bound (`GAME_HOST`/`GAME_PORT` from `.env.example`).
- [ ] Data directory `CHAO_DATA_DIR` on persistent storage; **backup taken
      immediately before deploy** and restore tested in staging.
- [ ] Clean shutdown verified: launcher `shutdown` line / SIGTERM produces a
      `saved` event and atomic profile writes (no temp-file leftovers in
      `data/`).

## 2. Account deletion & privacy settings

- [ ] In-app deletion removes the profile (evidence: `profiles.json` diff),
      clears device token and widget cache (`compliance/account-deletion.md`).
- [ ] Old token reuse returns `resumeOnly` 404 and creates no profile.
- [ ] Web deletion URL live; privacy policy URL live and linked in both
      consoles.
- [ ] Watch opt-in toggle persists (durable opt-in per `WATCH.md`) and revoke
      returns direct-control behavior.
- [ ] Widget opt-out/removal clears cached state; no ghost creature.

## 3. Accessibility

- [ ] Touch targets ≥ 44pt/48dp in the app and widget-config UI.
- [ ] Screen-reader labels on primary screens and widget.
- [ ] Color-contrast check on race/activities UI.
- [ ] Reduced-motion setting respected (evolution/mech animations).
- [ ] Text scales to largest dynamic type without truncating controls.

## 4. Deep links & protocol surface

- [ ] Deep links (garden invite / competition announcement) open the correct
      screen and respect origin checks server-side (`CHAO_ALLOWED_ORIGINS`).
- [ ] Unauthenticated deep link falls back to title screen (no auto profile
      creation; resume-only 404 path verified).
- [ ] `/api/status`, `/api/session`, `/api/activities`, `/ws` smoke-tested
      against the release build (`README.md` protocol list).

## 5. Widget (Tiny Garden phone widget)

- [ ] Fresh state renders correct creature snapshot.
- [ ] Stale state shows the staleness notice when `serverTime` exceeds the
      threshold (`compliance/widget-disclosure.md`).
- [ ] Offline airplane-mode: cached state + notice, no errors.
- [ ] Empty state after deletion/setup on both platforms.
- [ ] Planned Android refresh and quick-care use the agreed authenticated
      service contract, respect OS background limits and label stale/offline
      state. Verify on an installed widget; the browser preview is not proof.
- [ ] Widget screenshots captured for stores.

## 6. Offline behavior

- [ ] App offline: title screen reachable; no profile writes attempted;
      explicit resume failure messaging (404) shown, no silent new profile.
- [ ] Reconnect: session resumes, snapshot catches up; no duplicated training
      awards (server timestamps + once-only settle verified).
- [ ] Watch residency: creature remains garden-resident per `WATCH.md` with
      no simulated activity during downtime.

## 7. Backups, alerts, rollback

- [ ] Backup of `CHAO_DATA_DIR` (profiles, terrain, activities catalogs)
      automated; retention and rotation documented; restore drill passed.
- [ ] Alerts active: process down, WS error rate, save-failure notices,
      disk-full on data dir.
- [ ] Rollback drill: previous tagged binary redeployed against a copy of the
      current data dir; profiles load; training jobs resume (no award loss).

## 8. Staging test results (attach evidence)

| Test | Result | Evidence |
| --- | --- | --- |
| `cargo test --locked` (root) | `[ ]` | `[fill]` |
| Workspace crates tests (`tiny-garden-core`, `tiny-garden-games`) | `[ ]` | `[fill]` |
| `npm test` reference suite | `[ ]` | `[fill]` |
| Staging deploy + smoke (status/session/ws) | `[ ]` | `[fill]` |
| Deletion flow on staging | `[ ]` | `[fill]` |
| Widget fresh/stale/offline on real device | `[ ]` | `[fill]` |
| Load: 2-peer WS test (`tests/protocol.rs`) at production caps | `[ ]` | `[fill]` |

## 9. Go/no-go

- [ ] `compliance/support.md` contacts live; moderation rota on-call
      (`operations/moderation.md`).
- [ ] Live-ops flags/kill switches documented and reachable
      (`operations/liveops-runbooks.md` §4).
- [ ] Rollout plan: staged 10% → 50% → 100% (Play), phased availability
      (App Store); monitor plan for first 48 h.
- [ ] Release manager sign-off: ____________
