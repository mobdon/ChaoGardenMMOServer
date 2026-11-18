# iOS App Store Submission Checklist — Chao Garden MMO

Owner: release manager. Every item needs evidence (screenshot, test run ID, or
documented answer) recorded in the submission ticket before upload.

## 1. App Store Connect metadata

- [ ] App name: `Chao Garden MMO` (verify no third-party trademark conflict in
      App Store Connect name search; if blocked use alternate name from
      `compliance/review-gate.md` §6).
- [ ] Subtitle: "Raise, train, and race original creatures with friends".
- [ ] Primary category: Games → Simulation; secondary: Games → Role Playing.
- [ ] Keywords: no `Sonic`, `SEGA`, `Chao`-trademark claims beyond descriptive
      use of the generic word "chao" is disallowed — use "creature", "pet",
      "garden". (Keyword list lives in App Store Connect; copy placeholder in
      `compliance/support.md`.)
- [ ] Bundle ID matches provisioning profile; Team ID verified.
- [ ] Privacy policy URL points at the published
      `compliance/privacy-data-inventory.md`-derived policy page.
- [ ] Support URL = `compliance/support.md` support site; Support email routed.
- [ ] Marketing URL optional; review before adding.

## 2. Age rating & content

- [ ] Complete the App Store age-rating questionnaire using
      `compliance/age-rating.md` answers (no unrestricted user-generated text;
      moderated actions/reports only).
- [ ] Export compliance: verify app uses no proprietary encryption beyond
      HTTPS/WSS (Apple standard crypto exemption — answer "uses standard
      encryption only").
- [ ] Kids category: **not applicable** (not submitted to Kids category).

## 3. Privacy

- [ ] App Privacy "nutrition label" filled from
      `compliance/privacy-data-inventory.md` (profile name, token hash,
      progression, cosmetic ownership — no third-party analytics in MVP).
- [ ] Privacy manifest (`PrivacyInfo.xcprivacy`) lists required-reason APIs used
      by the widget (UserDefaults, file timestamps for save staleness).
- [ ] Account deletion available **in-app** per
      `compliance/account-deletion.md` (required by App Store guideline 5.1.1(v)).
- [ ] ATT prompt: not used (no tracking) — confirm `NSUserTrackingUsageDescription`
      absent.

## 4. Widget extension

- [ ] Widget extension target builds and is included in the archive
      (disclosures in `compliance/widget-disclosure.md`).
- [ ] Widget shows the stale-state notice (§5 of widget disclosure) after the
      configured staleness threshold instead of rendering stale creature state.
- [ ] Widget memory budget respected; no server network calls from widget
      process (data from app-group shared container written by the main app).
- [ ] Screenshots include at least one widget screenshot set.

## 5. Build & review artifacts

- [ ] Release build from tagged commit (`git tag vX.Y.Z`); Rust server version
      pinned by `Cargo.lock` (`cargo test --locked` green on CI
      `.github/workflows/check.yml`).
- [ ] Server deployment for the review environment reachable; reviewer
      instructions in App Review notes describe loopback/local behavior and the
      hosted demo garden.
- [ ] App Review demo notes: `CHAO_OWNER_BOOTSTRAP` is disabled on the review
      server; reviewer gets a standard profile via the support flow.
- [ ] TestFlight build tested on: iPhone (widget host), iPad (if universal).
- [ ] Screenshot set (6.7", 6.1", 5.5" if supported, iPad) from
      `compliance/support.md` placeholders.
- [ ] Export/localization: English verified; other locales marked
      untranslated.

## 6. Post-submission

- [ ] Record submission date, build number, and reviewer rejection reasons in
      the submission ticket; update this checklist with any new requirement.
