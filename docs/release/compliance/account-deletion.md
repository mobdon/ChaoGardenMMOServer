# Account Deletion Flow Requirements — Chao Garden MMO

Stores require a complete, discoverable deletion path for account-linked data.

## Requirements

1. **In-app entry point** — Settings → "Delete profile & data". Delete removes
   the profile row (display name, token hash, progression, cosmetics, gear,
   watch opt-in, creature state) from `profiles.json` on the connected server
   via an authenticated API call, then clears the device-side token and widget
   app-group cache.
2. **Server behavior** — deletion uses the same atomic temporary-file
   replacement write path as every profile write (`src/service.rs`), so a crash
   mid-delete cannot resurrect a half-deleted profile.
3. **Confirmation UX** — two-step confirm dialog stating what is removed
   (progression, creatures, cosmetics) and that the action is irreversible.
4. **Web fallback URL** — published "delete my data" page that lets a player
   submit a deletion request for a hosted server they can no longer reach in
   app (required by Play Console; verify URL linked in both store consoles).
5. **Support SLA** — deletion requests handled within 7 days
   (`compliance/support.md`).
6. **No cross-device residue** — other connected clients holding the deleted
   token must receive a resume failure (`resumeOnly` → 404, no new profile
   creation) and be logged out.
7. **Backups** — operator backups of `CHAO_DATA_DIR` purge deleted profiles on
   the next backup rotation or within 30 days; document rotation in
   `operations/release-checklist.md` §backups.

## Test evidence (link in release ticket)

- [ ] Delete in-app → profile gone from `profiles.json`; widget shows stale
      notice, not ghost data.
- [ ] Old token reuse rejected; no silent re-creation of a profile.
- [ ] Web deletion request processed end-to-end in staging.
- [ ] Node reference parity: same deletion semantics on `npm start` server.
