# Data handling boundaries

Updated November 18, 2026. Engineering requirements and current source notes;
this is not a completed privacy policy, legal review or store approval.

## Current canonical data

The Rust service stores profile identifiers, token hashes, owner role,
progression/rings, care state, gear, records and resident opt-in in the local
profile array. The public player projection exposes gameplay presentation
fields and omits the bearer credential/hash. Session responses provide the
credential to its client. Native assets, ROMs and save data stay local/ignored.

Protect the runtime directory and backups as private data. Redact credentials,
query tokens and profile contents from logs/test artifacts. Source publication
must exclude live saves and credentials. Current bearer authentication is not
an implemented account-deletion, refresh or revocable-device service.

## Adapter and migration requirements

Widget snapshots/cache must contain only required sanitized presentation data
and freshness information. Keep device credentials in the agreed secure
adapter; never include them in a public widget image/deep link. Partition and
clear identity-specific caches on signout, revocation and replacement.

Pairing/leases/deletion/retention changes require real Rust handlers, agreed
contracts and cross-client tests. A future deletion operation must define
canonical profiles, device credentials, cached views, receipts and backup
retention deliberately; generated policy/checklist text does not implement it.

New schema fields need explicit preservation/migration tests. Ignoring unknown
JSON on load is not a promise that the current known-field save projection
retains it. Do not run an older writer against newer live data without proven
compatibility.

References: [profile/public projections](../../src/game.rs),
[persistence/session service](../../src/service.rs),
[protocol proposal](../coordination/PROTOCOL.md),
[release draft limitations](../release/README.md).
