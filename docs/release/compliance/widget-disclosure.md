# Widget Disclosure Notes — Tiny Garden Phone Widget

Draft disclosure and implementation notes. The current browser widget preview
is not an installed Android widget, and these notes are not submission or
platform-approval evidence. Android is the selected first native platform;
see the [coordination briefs](../../coordination/WORKSTREAMS.md).

## Current browser preview

- `widget.html` and `src/widget.js` in the client repository show the same saved
  Chao's identity, needs, rings and resident activity in compact/wide/large
  preview layouts using the project's locally converted native Chao art.
- The preview resumes an existing browser save using `POST /api/session` with
  `resumeOnly:true`, then receives authenticated WebSocket snapshots. It does
  make network requests; it sends no gameplay input or care/training intents
  and does not create another Chao.
- Selecting care opens Tiny Garden. On a dropped connection, the preview
  labels retained last-received values as last seen/last sync. This is not proof
  of a persistent native cache, cross-device pairing or an installed OS widget.

## Planned Android widget

The requested native widget checks on the same canonical Chao and offers quick
pet/care interactions through authenticated, server-validated intents. The full
Tiny Garden app handles minigames and AFK training controls. Native UI/glue,
revocable device pairing, background synchronization and on-device APK/widget
acceptance remain implementation work. A standalone training core is tested;
durable training service integration is not connected yet.

The eventual native implementation may use app-managed cached snapshots and
platform-scheduled network work. Its actual architecture must determine the
disclosure; there is no agreed blanket ban on widget networking. Respect OS
refresh limits, label cached/stale values and require a server connection for
authoritative care. Do not invent local rewards, replay arbitrary offline care
later or imply a continuously running home-screen game.

## Draft user-facing description

Use wording that matches the build being demonstrated. For today's browser
preview:

> This browser preview connects to your saved Chao and displays live server
> updates. If the connection pauses, it shows the last received state and its
> last sync time. Open Tiny Garden for care. This preview does not install an
> Android home-screen widget.

Write native-store disclosure only after the native implementation and its
network/cache behavior have been verified. Do not submit the preview wording
as evidence that native widget features have shipped.

## Native acceptance work

- [ ] Secure, revocable pairing and capability checks connect the widget to the
      same Chao; no reusable credentials appear in links, logs or screenshots.
- [ ] Quick pet/care sends bounded intents and displays the server-confirmed
      result; failures/retries cannot duplicate effects. Minigames and training
      controls open the full Tiny Garden app.
- [ ] Fresh, stale, offline, revoked/unavailable and empty states are visible and
      tested. Freshness uses last confirmed synchronization and connection
      status; a displayed `serverTime` alone is not proof of a current sync.
- [ ] Local cached state is cleared or made unavailable when the implemented
      account/profile lifecycle requires it. Account deletion itself is not
      claimed as implemented by this checklist.
- [ ] Storage, network access and any platform privacy declarations describe
      the actual Android implementation; Apple app-group/UserDefaults behavior
      is not assumed for the Android design.
- [ ] Screenshots and device tests demonstrate each supported state, accessible
      touch targets, localized strings and the chosen OS refresh/resource budget.

Resident Watch continues on the server while it runs even if the owner closes
the app. A stale widget is a stale view, not evidence that the resident stopped.
Service downtime creates no simulated Watch activity or rewards (`WATCH.md`).
