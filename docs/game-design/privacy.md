# Privacy Requirements

## Data collected (MVP)

- Account: email or platform sign-in subject ID, hashed password (or OAuth
  subject), created/last-seen timestamps.
- Gameplay: chao state, inventory, garden layout, ledgers, visit/reaction
  metadata (user IDs + timestamps only — no content).
- Device: push token (optional), widget refresh requests (aggregated).

## Not collected

- Precise location, contacts, microphone/camera data, advertising IDs.

## Rules

1. **Minimal PII.** Email stored encrypted at rest (column-level); never sent
   to analytics or logs. Logs carry user IDs, never emails.
2. **Kids-adjacent care.** Audience includes younger players: no chat, no
   user-generated text sharing, no behavioral ad SDKs in MVP. COPPA/GDPR-K
   posture: no third-party trackers in the mobile build.
3. **Retention.** Accounts: retained while active; deleted on account-deletion
   request within 30 days (hard delete of PII, gameplay data anonymized to an
   opaque ID or purged). Backups roll off after 35 days.
4. **Deletion & export.** Self-service account deletion in-app; data export
   (JSON of account + gameplay data) on request, delivered within 30 days.
5. **Consents.** Push notifications and analytics (aggregate-only, first-party)
   are opt-in.
6. **Third parties.** Object storage and managed Postgres are subprocessors
   listed in the privacy policy; no other data sharing.
7. **Telemetry.** Observability metrics are pre-aggregated counters/histograms
   with user IDs salted and rotated daily; no raw payloads in traces beyond
   sampled request shapes.
