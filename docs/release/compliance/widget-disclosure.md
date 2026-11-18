# Widget Disclosure Notes — Tiny Garden Phone Widget

App-review-facing notes plus internal requirements for the home-screen widget.

## What the widget does

- Renders a compact view of the player's original creature and garden state so
  the garden feels present on the phone home screen ("Tiny Garden" widget).
- Data source: the **main app** writes the latest snapshot into an app-group
  shared container after each successful session sync. The widget itself makes
  **no network calls**.

## Reviewer-visible disclosure (include in App Review notes / Play description)

> The widget displays your creature using data cached locally by the app. If
> the data is older than the freshness threshold, the widget shows a "stale —
> open the app to refresh" notice instead of stale gameplay numbers. The
> widget works offline and never contacts the network by itself.

## Internal requirements

- [ ] Stale state: widget renders the stale notice once
      `serverTime` in the cached snapshot exceeds the staleness threshold
      (aligns with `WATCH.md` offline residency rules — no simulated activity
      while disconnected).
- [ ] Offline behavior: widget shows last cached state + stale notice; no
      error dialogs.
- [ ] Deleted profile: after account deletion, widget must show "Open app to
      set up your garden" (empty state), never a deleted creature.
- [ ] Privacy: widget reads only the app-group container; no shared-device
      identifiers, no tracking.
- [ ] Privacy manifest: widget declares `UserDefaults` / file-timestamp
      required-reason APIs it uses.
- [ ] Screenshot evidence: fresh state, stale state, and empty state captured
      for store screenshots (`compliance/support.md`).
- [ ] Localizations: stale/empty strings present in every shipped locale.
- [ ] Memory/CPU budget: refresh path profiled on oldest supported OS version.
