# Support Documentation — Chao Garden MMO

## Support contacts (replace placeholders before submission)

| Channel | Placeholder | Owner | SLA |
| --- | --- | --- | --- |
| Support email | `support@chaogarden.example` | Community lead | First response ≤ 2 business days |
| Privacy requests / data deletion | `privacy@chaogarden.example` | Operator | Deletion ≤ 7 days |
| Abuse / moderation escalations | `moderation@chaogarden.example` | Moderation lead | See `operations/moderation.md` targets |
| Bug reports (in-app) | Feedback form in Settings | Release manager | Triage ≤ 2 business days |
| Server incidents | Status page + incident channel | On-call | See `operations/liveops-runbooks.md` §4 |

## Store listing copy placeholders

**Short description (≤80 chars):**
> Raise, train, and race original creatures in a living garden with friends.

**Full description placeholder:**
> Chao Garden MMO is a peaceful creature-raising garden. Train your creatures
> with server-timed exercises, shop for cosmetics with in-game rings, enter
> scheduled races and tournaments, and let Tiny Watch keep your garden alive
> while you're away. Add the Tiny Garden widget to keep your creature on your
> home screen. No ads, no chat required to play, no data selling.
> Multiplayer gardens are run by their own communities; see the privacy policy.

**Keywords (iOS):** creature garden, pet raise, idle training, race, cozy,
multiplayer garden, widget pet

**Release notes placeholder:**
> — Balance and catalog updates; widget freshness improvements; stability fixes.
> (Trademark check required per `compliance/review-gate.md` before publishing.)

## Screenshot / media placeholders

| Asset | Store slot | Status | Capture notes |
| --- | --- | --- | --- |
| Garden hero shot (hub) | iOS 6.7"/6.1", Play feature graphic base | `[ ]` | Show original creature, no third-party art |
| Training view | iOS, Play phone | `[ ]` | Server-timed training UI |
| Race finish | iOS, Play | `[ ]` | Original course geometry only |
| Tiny Watch / widget set | iOS widget gallery, Play widget preview | `[ ]` | Fresh + stale states |
| Karate/activities | Play tablet if shipped | `[ ]` | Native-roster tournament per `ACTIVITIES.md` |
| Settings + delete profile | Not for store; internal evidence | `[ ]` | Proves deletion path |

## App Review notes template

- Multiplayer demo server URL (WSS over TLS) and a reviewer profile name.
- How to create a profile: open app → enter a name (`POST /api/session`).
- Widget: main app must sync once before widget shows data; stale notice is
  expected behavior when offline.
- No account or purchase required to review core gameplay.
