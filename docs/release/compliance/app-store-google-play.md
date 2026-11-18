# Google Play Submission Checklist — Chao Garden MMO

Owner: release manager. Evidence recorded in the submission ticket before upload.

## 1. Play Console metadata

- [ ] App name `Chao Garden MMO`; short description ≤80 chars and full
      description from `compliance/support.md` (trademark-checked per
      `compliance/review-gate.md`).
- [ ] Package name matches signing key; Play App Signing enabled.
- [ ] Category: Games → Simulation.
- [ ] Content declarations completed:
  - [ ] Ads: none in MVP.
  - [ ] In-app purchases / billing: list all server-cosmetic SKUs; free balance
        corrections handled via support (see `operations/liveops-runbooks.md` §5).
  - [ ] Data safety form filled from `compliance/privacy-data-inventory.md`
        (data types: account name, user IDs, gameplay data; all stored on
        player-chosen/community server; no third-party SDK collection).
  - [ ] Content ratings questionnaire (IARC) answered per
        `compliance/age-rating.md` → obtain ESRB/IARC rating certificate.
  - [ ] Target audience: 13+; not designed for children — do **not** declare
        target age under 13 (chat-free but persistent social play).
- [ ] Government apps / financial features / health: not applicable.
- [ ] News apps: not applicable.

## 2. Account & privacy requirements

- [ ] Data deletion: provide a web deletion request URL (Account Deletion
      disclosure) and an in-app path — see `compliance/account-deletion.md`.
- [ ] Privacy policy URL published and linked in Play Console **and** in-app.
- [ ] Permissions minimization: verify only INTERNET, FOREGROUND_SERVICE (if
      used for connectivity), POST_NOTIFICATIONS (campaigns), and
      widget-related bindings are requested; document each permission string in
      the data-safety notes.

## 3. Widget

- [ ] Widget preview image uploaded (see `compliance/support.md` placeholders).
- [ ] Widget discloses data source and stale behavior per
      `compliance/widget-disclosure.md`.
- [ ] Widget tested on Android 12 (Material You sizing), Android 13/14 pinning,
      and launcher-restart stale handling.

## 4. Build & release track

- [ ] Release built from tagged commit; `cargo test --locked` and `npm test`
      green (`.github/workflows/check.yml`).
- [ ] Staging test results attached per `operations/release-checklist.md`
      (account deletion, privacy settings, offline, widget stale states).
- [ ] Pre-launch report reviewed (crashes, ANRs) and blockers triaged.
- [ ] Internal → closed → production staging progression documented in the
      release ticket; staged rollout percentage recorded (10% → 50% → 100%).
- [ ] Release notes copy from `compliance/support.md` "release notes"
      placeholders, trademark-checked.

## 5. Server side

- [ ] Hosted demo/community server reachable over WSS with valid TLS; allowed
      origins set via `CHAO_ALLOWED_ORIGINS` (`.env.example`).
- [ ] Play Console "app access" instructions for reviewers (how to create a
      profile: `POST /api/session` with a name; no special credentials needed).
