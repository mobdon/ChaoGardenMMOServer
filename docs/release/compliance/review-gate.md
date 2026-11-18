# Compliance Review Gate — Intellectual Property & Licensing (BLOCKING)

**This gate must be signed before any store build is uploaded or marketing
material is published. If any box cannot be checked, the release is blocked.**

Release version: ____________  Tag: ____________  Date: ____________

## 1. Names and branding

- [ ] Product name "Chao Garden MMO" cleared by a trademark search; no SEGA,
      Sonic Team, Nintendo, or third-party marks in the name, logo, or tagline.
- [ ] If any conflict exists, use the approved alternate name from §6 and
      re-run the search.
- [ ] Icon and logo are original artwork (designer + license recorded in
      `licensing/asset-inventory.md`).

## 2. Creatures, art, and models

- [ ] All creature designs, stat names shown in-game, animations, terrain, and
      environment art are original works.
- [ ] No native engine models, motions, textures, music, or terrain bytes are
      included in the store build. The import/inspection tooling
      (`crates/tiny-garden-core`, `data/imports/`, Git-ignored `.local/`) is a
      development tool that processes an owner's local files and is never
      redistributed.
- [ ] Fingerprint extraction paths (`tiny-garden-inspect`) are excluded from
      release builds or compiled out of the shipped client.
- [ ] No sprites, colors, or geometry copied from third-party games.

## 3. Music and sound effects

- [ ] All music tracks and SFX are original, commissioned, or licensed with
      license records in `licensing/asset-inventory.md`.
- [ ] No native garden audio or rips of any commercial soundtrack.

## 4. Code and dependencies

- [ ] Dependency licenses inventoried in `licensing/asset-inventory.md`
      (Cargo.lock crates and package-lock.json packages); no GPL/AGPL dependency
      ships in closed distribution without operator approval documented there.
- [ ] No copied third-party code without attribution where required.

## 5. Marketing materials

- [ ] Screenshots, trailer, feature graphic, and store copy contain only
      original or licensed assets (`compliance/support.md` placeholders).
- [ ] Copy makes no claim of affiliation with SEGA, Nintendo, Sonic Team, or
      the "Sonic"/"Chao" trademark families ("Chao Garden" used only after the
      §1 clearance).
- [ ] Store copy does not promise unshipped features (chat excluded from MVP).

## 6. Alternate name candidates (approved pool)

1. `Garden Pals MMO` 2. `Little Garden Creatures` 3. `Cozy Garden Online`

## Sign-off

| Role | Name | Date | Signature |
| --- | --- | --- | --- |
| Release manager | | | |
| Legal/IP owner | | | |
