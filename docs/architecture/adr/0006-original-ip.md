# ADR-0006 — Original IP Only

**Status:** Accepted

## Context
ChaoGarden is inspired by the creature-raising genre but must ship as
original, distributable IP. Sega/Sonic assets are proprietary and cannot be
bundled, distributed, or referenced in production.

## Decision
- **All production names are original:** creature species, fruit, stats,
  items, locations, lore, and branding are invented for ChaoGarden. The word
  "Chao" as a creature name is replaced in production data by original species
  names (working internal codename: `mossling`, `emberwing`, `puffleaf`).
- **All visuals, audio, and models are original:** commissioned or in-house
  artwork, CC0/created sounds and music, original logo and typography. No
  ripped or extracted game files are ever bundled.
- The legacy prototype tooling that imports an owner's own installed game
  files lives in separate client-tools repositories, is clearly labeled as
  non-production, and is excluded from release builds, stores, and CI
  artifacts of production clients.
- CI includes an **asset provenance check**: every shipped asset has a
  `provenance` entry (license/author) in a manifest; unattributed assets fail
  the build. A blocklist of trademarked terms guards strings in production
  config and copy.
- Trademark-safe naming review happens before store submission; the wordmark,
  icon, and store listing use original branding only.

## Consequences
- No dependency on proprietary file formats in production paths.
- Slower art pipeline (original work) but unrestricted distribution and
  monetization.
