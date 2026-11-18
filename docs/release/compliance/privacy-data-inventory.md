# Privacy Data Inventory — Chao Garden MMO

Draft implementation inventory for the current local prototype, not an approved
privacy policy or completed store-form answers. Reconcile any later release
against its actual build, deployment and data flows. Current profile fields are
defined by `Player::profile` in `src/game.rs`, written by `src/service.rs` to
`profiles.json` under `CHAO_DATA_DIR`; the Node reference is in `game.js`.

## Current data flows versus planned native behavior

| Data | Field / source | Purpose | Transmitted | Stored | Retention | Third parties |
| --- | --- | --- | --- | --- | --- | --- |
| Display name | `POST /api/session` `name` | Identify player in garden | Yes (snapshots) | Server `profiles.json` | No implemented deletion/retention workflow | No integration in current code |
| Session token | `/api/session` response; `hash_token` uses SHA-256 without a separate salt | Current development bearer credential for resume and `/ws?token=...` | HTTP/WS in loopback development; HTTPS/WSS only when the deployment provides it | Hash in server profile; raw token in browser save-slot storage | No device revocation/deletion workflow yet; clearing browser storage does not delete the server profile | No integration in current code |
| Progression | Rings, stats, race/karate/Tiny records, alignment, cosmetic ownership, gear slots and vehicle selection | Gameplay | Snapshots | Server `profiles.json` | No implemented deletion/retention workflow | No integration in current code |
| Creature state | Persisted form and happiness; energy/hunger/position also appear in runtime snapshots | Gameplay | Snapshots | Persisted subset in `profiles.json`; transient values in memory | No implemented deletion/retention workflow | No integration in current code |
| Tiny Watch opt-in | `resident:{enabled,zone,x,z}` per `WATCH.md` | Shared garden residency while disconnected | Public Watch state in snapshots | Server `profiles.json` | Explicit Watch stop clears opt-in; profile deletion is not implemented | No integration in current code |
| Browser widget preview | Client `widget.html` / `src/widget.js`; browser save selection and last-received runtime snapshot | Display the same saved Chao | Resume POST and WebSocket snapshots; no gameplay intents | Browser credentials/save summaries; live preview state in memory | Native persistent-cache/removal behavior is unimplemented | No integration in current code |
| Native Android widget cache/pairing | PLANNED; architecture and data fields require implementation | Status display and quick server-validated care | Planned authenticated synchronization and care intents | Planned device storage; no app-group architecture established | To define and verify with implementation | To verify with implementation |
| Local source/save inspection | Read-only tooling including `crates/tiny-garden-core/src/native_save.rs` | Inspect supplied local formats; not live widget/profile persistence | No automatic live profile import/upload | Local developer-controlled input/output files, ignored by Git | Managed separately from the canonical service profile | No integration in current tooling |

Animal history/genetic records and durable AFK jobs are not current persisted
profile fields. TrainingLedger is a tested standalone adaptation; its production
service/job storage is not wired. Native save-format readers do not establish
that those source fields exist in the live MMO schema.

## Current source observations (recheck for each release)

- No email, phone number, or third-party account sign-in (name + token model).
- No analytics, crash-reporting, or advertising SDKs in the MVP.
- No precise location, contacts, photos, microphone, or camera access.
- No implemented store billing or payment-instrument handling.
- No implemented unrestricted player-chat/reporting pipeline; moderation
  procedures remain drafts. This is not a completed production data inventory.

## Server-only surface (hosted gardens)

- Session/connection state and message-rate counters are in memory. Current
  `src/service.rs` applies 2 KiB message/frame and 80-message-per-second bounds
  per WebSocket; this does not establish a persisted peer-address inventory or
  the logging behavior of any future reverse proxy/hosting provider.
- Terrain and activity catalogs are server data, not personal data.

## Implemented controls and planned lifecycle work

- Installed-widget setup/removal, native cache clearing and revocable pairing
  remain planned. The browser preview can select an existing saved Chao; it is
  not proof of native lifecycle behavior.
- Watch opt-in is explicit and durable per `WATCH.md`; revoking restores
  direct-control behavior only.
- Server profiles live under `CHAO_DATA_DIR`; browser credentials/save summaries
  also live in browser storage. Backup, retention and deletion operations need
  implementation-specific review; the release checklist is not proof they run.

## Privacy policy placeholder — not ready for publication

Write final copy from the verified release inventory. The current token hash is
SHA-256 without a separate salt; in-app/web deletion and native device revocation
are not implemented. The example support contact elsewhere in this packet is a
placeholder, not a verified live privacy channel. Do not promise completed
deletion, retention or native-widget behavior from this draft.
