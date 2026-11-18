# Release & Operations Documentation — Chao Garden MMO

This directory is the single source of truth for app-store submission, privacy,
moderation, asset licensing, and live-operations readiness for **Chao Garden MMO**
(server repository: `mobdon/ChaoGardenMMOServer`, client: `mobdon/ChaoGardenMMOClient`)
and its companion **phone widget**.

> **Compliance review gate (blocking):** before any store build is uploaded, the
> reviewer must sign [compliance/review-gate.md](compliance/review-gate.md). The
> gate confirms all names, creatures, art, music, sound effects, icons, and
> marketing materials are **original or properly licensed** and that the product
> uses **no Sonic, SEGA, or Nintendo trademarks**. Native game assets referenced
> by development tooling are local-only data (Git-ignored `data/`, `.local/`) and
> are never redistributed in a store build.

| Document | Purpose |
| --- | --- |
| [compliance/app-store-ios.md](compliance/app-store-ios.md) | iOS App Store submission checklist |
| [compliance/app-store-google-play.md](compliance/app-store-google-play.md) | Google Play submission checklist |
| [compliance/privacy-data-inventory.md](compliance/privacy-data-inventory.md) | Data collection inventory & privacy nutrition labels |
| [compliance/account-deletion.md](compliance/account-deletion.md) | Account-deletion flow requirements |
| [compliance/age-rating.md](compliance/age-rating.md) | ESRB / IARC / App Store age-rating questionnaire answers |
| [compliance/widget-disclosure.md](compliance/widget-disclosure.md) | Widget-specific disclosures & review notes |
| [compliance/support.md](compliance/support.md) | Support contacts, in-store copy, screenshot placeholders |
| [compliance/review-gate.md](compliance/review-gate.md) | Blocking IP/licensing review gate (sign-off) |
| [licensing/asset-inventory.md](licensing/asset-inventory.md) | Bundled asset + dependency license inventory |
| [operations/moderation.md](operations/moderation.md) | Reports, blocks, evidence retention, appeals, admin access |
| [operations/liveops-runbooks.md](operations/liveops-runbooks.md) | Catalog changes, sim rollout, competitions, kill switches, refunds, notifications |
| [operations/release-checklist.md](operations/release-checklist.md) | Full-app + widget release checklist (staging evidence) |

## Product summary

- **Server:** Rust authority (`src/service.rs`, `src/game.rs`, `src/phone.rs`,
  `src/watch.rs`, `src/activities.rs`) started via
  `cargo run --release --locked`; configured with `CHAO_DATA_DIR`, `GAME_HOST`,
  `GAME_PORT`, `CHAO_OWNER_BOOTSTRAP`, `CHAO_ALLOWED_ORIGINS`, `CHAO_SHUTDOWN_STDIN`
  (see `.env.example`).
- **Client:** browser client (proxying `/api` and `/ws`); the Node reference in
  `index.js`/`game.js`/`constants.js`/`activities.js`/`watch.js` is retained for
  comparison and tests (`npm ci && npm test && npm start`).
- **Widget:** Tiny Chao Garden companion widget (phone home-screen) rendering
  Chao state via the Rust core crates `crates/tiny-garden-core` (save/GBA format
  handling, idle training) and `crates/tiny-garden-games` (memory, rps, search,
  bounce, ball games). See `PHONE.md`, `WATCH.md`, `ACTIVITIES.md`, `MECH_MOVES.md`.
