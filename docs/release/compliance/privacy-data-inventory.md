# Privacy Data Inventory — Chao Garden MMO

Feeds the App Store "App Privacy" label and Play "Data safety" form.
Source of truth: profile fields written by the authority in `src/service.rs`
(`profiles.json` under `CHAO_DATA_DIR`) and the Node reference `game.js`.

## Data we collect and store

| Data | Field / source | Purpose | Transmitted | Stored | Retention | Third parties |
| --- | --- | --- | --- | --- | --- | --- |
| Display name | `POST /api/session` `name` | Identify player in garden | Yes (snapshots) | Server `profiles.json` | Until deletion | None |
| Session token | `/api/session` response; stored as SHA-256 token hash server-side | Bearer credential for `/ws?token=...` and API calls | Token (TLS/WSS only) | Only SHA-256 hash (`profiles.json`) | Until deletion/logout | None |
| Progression | Rings, stat growth, race/karate results, alignment, cosmetic ownership, gear slots (`gear.footwear`, `gear.hands`), vehicle selection | Gameplay | Snapshots | Server `profiles.json` | Until deletion | None |
| Creature state | Chao stats, form, animal history, genetic data (server-timed idle training, `crates/tiny-garden-core` adaptation) | Gameplay | Snapshots | Server `profiles.json` | Until deletion | None |
| Tiny Watch opt-in | `WATCH.md` durable opt-in flags; garden residency while disconnected | Gameplay continuity | Snapshots | Server | Until deletion | None |
| Widget snapshot cache | App-group shared container written by the main app | Render the phone widget offline | No network | Device only | Overwritten each app sync | None |
| Local game data (device) | Widget/game save records (`crates/tiny-garden-core/src/native_save.rs` reads only) | Local gameplay | No network | Device only | Until cleared | None |

## Data we do NOT collect

- No email, phone number, or third-party account sign-in (name + token model).
- No analytics, crash-reporting, or advertising SDKs in the MVP.
- No precise location, contacts, photos, microphone, or camera access.
- No payment instrument data (store billing only if SKUs ship).
- Unrestricted chat is excluded from the MVP (see `operations/moderation.md`),
  so no message content is collected.

## Server-only surface (hosted gardens)

- Connection metadata (WebSocket peer address) exists only in server memory for
  rate limiting (2 KiB / 80 messages per second caps in `src/service.rs`) and is
  not persisted.
- Terrain and activity catalogs are server data, not personal data.

## Privacy settings offered

- Widget opt-out: user can disable/refresh/remove the widget; cached state is
  device-local and cleared on removal.
- Watch opt-in is explicit and durable per `WATCH.md`; revoking restores
  direct-control behavior only.
- Data directory backups are operator responsibility (see
  `operations/release-checklist.md` §backups); personal data lives only in
  `CHAO_DATA_DIR`.

## Privacy policy copy placeholder

> Chao Garden MMO collects only the display name and gameplay progression you
> create. Credentials are stored as salted SHA-256 token hashes. Data is stored
> on the garden server you connect to and is deleted when you delete your
> profile (in-app or via the deletion request URL). We do not sell data, run
> ads, or embed analytics. Contact support@chaogarden.example for privacy
> questions.
