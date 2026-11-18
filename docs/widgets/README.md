# Garden Widget — Native iOS + Android Home-Screen Widgets

## Overview

Tiny Garden ships native home-screen widgets (iOS WidgetKit/SwiftUI + Android
Glance/Kotlin) that display a compact garden scene: creature thumbnail, mood,
key need indicators, and the snapshot timestamp. The widget is **read-only and
offline**: it renders a sanitized snapshot the app wrote after a successful
server sync and deep-links back into the app for every action.

## Architecture

```
App (sync success)
  └─ widgetSnapshot.ts (sanitize + version + write)
       ├─ iOS:    App Group UserDefaults  (group.dev.chaogarden.tinygarden)
       └─ Android: DataStore (chaogarden_widget.preferences_pb)

Widget (render only, no network, no state mutation)
  ├─ iOS:    ios/TinyGardenWidget/  (WidgetKit extension, timelines)
  └─ Android: android/.../widget/   (Glance AppWidget, workmanager refresh)
```

## Snapshot schema

Versioned JSON, `schemaVersion: 1` (`packages/contracts/src/widget-snapshot.ts`):

```jsonc
{
  "schemaVersion": 1,
  "writtenAt": 1730000000000,       // epoch ms
  "serverTime": 1730000000000,
  "garden": { "name": "Emerald Meadow", "themeColor": "#7ED957" },
  "creature": {
    "name": "Nimbus",
    "mood": "happy",                // happy|content|hungry|sleepy|lonely
    "moodLabelKey": "widget.mood.happy",
    "thumbnail": "creature/nimbus",
    "needs": [{ "kind": "hunger", "level": 0.8, "labelKey": "widget.need.hunger" }]
  },
  "actions": { "feed": "chaogarden://widget/feed", ... }
}
```

Sanitization guarantees (enforced in `sanitizeWidgetSnapshot`):

- never contains access/refresh tokens, session ids, or player ids;
- never contains private profile fields (email, display handle) or full
  inventory — only at most 4 aggregate need indicators;
- no free-form server strings reach the widget except the garden name, which
  is truncated to 32 characters.

## Update cadence & quotas

- **iOS:** the extension returns a timeline with entries at the freshness
  boundary plus one stale-state entry; WidgetKit decides actual redrawing.
  The app calls `WidgetCenter.shared.reloadAllTimelines()` only after a
  successful sync.
- **Android:** refresh is throttled to the Android widget update quota
  (min 30 minutes via WorkManager periodic work); the app triggers an
  opportunistic `updateAll` after sync. The receiver itself does no network
  or game-state mutation.

## Deep links

All widget taps route into the app via the `chaogarden://` scheme:
`chaogarden://widget/garden`, `chaogarden://widget/feed`,
`chaogarden://widget/train`. The app routes (`app/widget/*`) handle these,
require an authenticated session, and perform any state-changing call from
inside the app — never from the widget process.

## Files

| Path | Purpose |
| --- | --- |
| `packages/contracts/src/widget-snapshot.ts` | Versioned snapshot types + sanitizer + tests |
| `apps/mobile/src/widget/widgetSnapshot.ts` | Bridge: read game state → write native storage |
| `apps/mobile/src/widget/__tests__/widgetSnapshot.test.ts` | Bridge tests |
| `apps/mobile/plugins/withTinyGardenWidget.js` | Expo config plugin wiring native targets |
| `apps/mobile/ios/TinyGardenWidget/*` | SwiftUI WidgetKit extension |
| `apps/mobile/android/app/src/main/java/dev/chaogarden/tinygarden/widget/*` | Glance widget + receiver + refresh worker |
| `apps/mobile/app/widget/*` | Deep-link routes |

## Local validation

1. `npx expo prebuild -p ios` then open `ios/*.xcworkspace`, run the
   `TinyGardenWidgetExtension` scheme on a simulator, add the widget to the
   home screen.
2. `npx expo prebuild -p android`, deploy to a device/emulator, long-press
   launcher → Widgets → Tiny Garden (small/medium variants).
3. Exercise: fresh snapshot, stale snapshot (`serverTime` in the past), and
   empty/corrupted snapshot (write garbage JSON to the app group / DataStore)
   — widget must show stale notice or the empty setup state, never crash.
4. Dark mode: verify both variants use `widgetBackground` semantic colors.
5. Accessibility: VoiceOver / TalkBack read the creature name, mood, and
   needs via the `accessibilityLabel`s on the widget views.

## Expo Go limitations

Native widgets require a **development build**; Expo Go cannot load WidgetKit
extensions or Glance AppWidgets. Use `expo prebuild` + `expo run:ios` /
`expo run:android`. Deep-link routes still work in Expo Go for quick testing.
