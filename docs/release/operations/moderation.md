# Moderation Workflow — Chao Garden MMO

Unrestricted chat is **excluded from the MVP**, but social surfaces still need
moderation: display names, in-game actions, shared-garden behavior, and
scheduled competitions.

## Surfaces and controls

| Surface | Risk | Control |
| --- | --- | --- |
| Display names (`POST /api/session` `name`) | Impersonation, slurs, trademark misuse | Name blocklist + manual review; rename enforced by operator |
| Emotes/actions (`type:"action"`) | Spam | 80 msg/s / 2 KiB caps already enforced in `src/service.rs`; action rate tuning in liveops |
| Shared gardens | Griefing (train theft, race interference) | Owner-role controls via server-stored role (`CHAO_OWNER_BOOTSTRAP`, request fields never grant ownership); kick/ban reserved to owner role |
| Competition scheduling | Organizer abuse | `operations/liveops-runbooks.md` §3 approval workflow |
| Player reports | False reports | Evidence-based triage below |

## Report and block workflow

1. **Capture** — a report records: reporter profile ID, subject profile ID,
   surface, timestamp, and a server-side snapshot reference (server is
   authority; no client screenshots needed for automated evidence).
2. **Auto-triage** — blocklisted name strings auto-flag; repeat reporters and
   targets are weighted to detect abuse of the report system.
3. **Human review** — moderation lead reviews within targets (table below),
   consulting `profiles.json` progression logs for the incident window.
4. **Actions available** — warning, forced rename, temporary suspension,
   permanent suspension, competition ban. Only the owner/admin role can apply
   suspensions.
5. **Notify** — outcome emailed to reporter and subject (no other player's
   personal data disclosed).

## Response targets

| Severity | Examples | Target |
| --- | --- | --- |
| P0 | Impersonation of staff, safety issue, exploitable role escalation | 4 hours |
| P1 | Slur/abusive name visible to many players | 1 business day |
| P2 | Spam, griefing report | 3 business days |
| P3 | Appeal, false-report review | 7 days |

## Evidence retention

- Reports, moderator notes, and outcomes retained **12 months** in the
  moderation log (append-only, separate from `CHAO_DATA_DIR` profiles).
- Server snapshots used as evidence retained 30 days.
- Personal data in retained reports is minimized to profile IDs and names.
- Deletion of a player profile does **not** purge moderation evidence needed
  for appeals (documented in privacy policy).

## Appeals

- One appeal per enforcement, submitted to `moderation@chaogarden.example`
  (`compliance/support.md`) within 30 days.
- Reviewed by a moderator **other than** the one who applied the action.
- Outcome within 7 days; reversal restores progression from the server's
  authoritative profile (suspensions do not delete data unless the player also
  requests deletion via `compliance/account-deletion.md`).

## Administrator access controls

- Roles live in the server-stored profile role field; request fields can never
  grant ownership (`README.md`: "Request fields never grant ownership").
- Owner bootstrap: `CHAO_OWNER_BOOTSTRAP=1` grants the first profile owner
  powers, loopback-only, for new private data directories; **must be disabled**
  in production and on any review server.
- Admin actions (ban, rename, catalog change, kill switch) require: (a) admin
  role on the server host, (b) entry in the admin action log with actor, time,
  reason, and (c) for P0/P1, a second-person confirmation.
- Server host access limited to operators; `CHAO_SHUTDOWN_STDIN` used only via
  the launcher for clean saves.

## Pre-launch checks

- [ ] Name blocklist loaded and tested.
- [ ] Report submission path live and rate-limited.
- [ ] Moderation log storage created and access-controlled.
- [ ] On-call moderation rota published for launch week.
