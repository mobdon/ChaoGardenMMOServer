# ADR-0005 — Native Widget Data Sharing

**Status:** Accepted

## Context
The "tiny garden" widget must render chao state without network access from
the widget process, and iOS (WidgetKit) and Android (Glance/AppWidget) have
different storage constraints.

## Decision
- The RN app, after each sync/foreground refresh, serializes a **widget
  snapshot** (`chaoSummary[]`, garden thumbnail reference, currencies,
  `generatedAt`) into a shared app-group store:
  - **iOS:** App Group `group.net.chaogarden.widget`, file-based JSON plus
    UserDefaults for freshness timestamp; WidgetKit timeline reloaded via
    `WidgetCenter` from the app (and optionally a small App Intent refresh).
  - **Android:** app-group storage via `androidx.glance` state or a shared
    preferences file readable by the widget process; `AppWidgetManager`
    update after each sync.
- Widgets render **only** from local data and interpolate needs decay locally
  using the shared formulas (values copied into the snapshot); they show a
  "stale" indicator if `generatedAt` is older than 24 h.
- Server push notifications deep-link into the app, which refreshes the
  snapshot — the widget process itself never performs network calls or holds
  credentials.
- Snapshot schema versioned; widget tolerates unknown fields and falls back to
  a generic render on version mismatch.

## Consequences
- No widget-specific API surface; no auth risk on the widget side.
- Snapshot size capped (< 64 KB) to keep writes cheap.
- iOS background refresh is best-effort; the widget degrades gracefully to a
  last-known-state card.
