# Asset & Dependency License Inventory

Update on every release; block release (via `compliance/review-gate.md`) on
unknown-provenance assets.

## 1. Bundled runtime assets (shipped in store builds)

| Asset | Source | License | Provenance evidence |
| --- | --- | --- | --- |
| App icon / logo | In-house designer | Original, all rights owned by project | Design file link `[fill]` |
| UI art | In-house | Original | `[fill]` |
| Creature models/animations | In-house | Original | `[fill]` |
| Music tracks | `[fill: composer]` | `[fill: license]` | `[fill]` |
| Sound effects | `[fill]` | `[fill]` | `[fill]` |
| Fonts | `[fill]` | OFL/Apache if Google Fonts | `[fill]` |

**Not shipped, ever:** native engine models, motions, textures, music, terrain,
GBA program bytes, save ROMs, or anything under Git-ignored `data/imports/`,
`data/activities/`, and client `.local/gba/`. These exist only as local
development data referenced by the import tools described in `README.md`
("The runtime data directory defaults to `./data` …").

## 2. Rust crates (from `Cargo.lock` at tag vX.Y.Z)

Crate list regenerated with: `cargo tree --locked` (run at release tag; paste
output into the release ticket). Key licenses:

| Crate | Version | License | Notes |
| --- | --- | --- | --- |
| axum | 0.8.9 | MIT | HTTP/WS server (`src/service.rs`) |
| tokio | 1.x | MIT | Runtime |
| serde / serde_json | 1.x | MIT/Apache-2.0 | Serialization |
| sha2, sha1 | 0.10.x | MIT/Apache-2.0 | Token hashing, fingerprints |
| futures-util | 0.3 | MIT/Apache-2.0 | WS streams |
| url, uuid | 2 / 1 | MIT/Apache-2.0 | Origins, IDs |
| tiny-garden-games | workspace | Project proprietary | Original game logic (memory, rps, search, bounce, ball) |
| tiny-garden-core | workspace | Project proprietary | Original adaptation + read-only format tooling |
| dev: tempfile, tower, http-body-util, tokio-tungstenite | — | MIT/Apache-2.0 | Test-only; not shipped |

Action: run `cargo about`/manual audit to confirm no copyleft runtime
dependency before sign-off.

## 3. Node reference packages (from `package-lock.json`, dev/reference only)

The Node implementation is a retained reference (`npm test`, `npm start`) and
is **not** bundled in store builds. Audit `npm ls --all` at release; record
third-party licenses in the release ticket. Reference tests in
`tests/reference/` use synthetic asset bytes only.

## 4. Third-party references (documentation only, not shipped)

- SAT-R/sa2 repository — cited as source evidence in
  `crates/tiny-garden-core/README.md`; facts cited, no code copied.
- mGBA LZ77 implementation — used as a *check reference* only; decoder written
  independently (`crates/tiny-garden-core/src/gba.rs`).

## 5. Sign-off

- [ ] Table 1 complete and licenses attached.
- [ ] `cargo tree --locked` audit attached; no copyleft runtime deps.
- [ ] No native/trademarked assets in build artifacts (grep of shipped bundle
      for known fingerprints/hash `2c7da8e1c9af85986eecf4effae39197cdac9abf`
      must be negative).
