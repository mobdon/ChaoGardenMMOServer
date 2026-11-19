# ADR-0005 — Android Tiny Garden widget boundary

Status: proposed native adapter; Android selected first, November 18, 2026.

Mobile is strictly Tiny Garden: the same Chao, care/stats, AFK training and five
minigames. The Android home-screen widget provides viewing/quick care and opens
Tiny Garden for full games/training activities. Browser previews do not prove
an installed widget or OS background behavior.

Implement Android bindings in the client repository. Consume the agreed Rust
observation/care/identity contract. An observer does not implicitly acquire a
gameplay lease; quick care remains a server-authorized bounded intent. Cached
state must identify freshness and connectivity and cannot upload claimed gains.

A native cache should contain only the required sanitized presentation fields,
the observed server timestamp and credential-free app-entry information.
Credentials use the agreed revocable secure adapter, not the public widget
snapshot. Clear identity-specific cached state on signout/revocation. Respect
Android scheduling limits; do not claim an always-running widget simulation.

Acceptance requires an actual Android build/installation, process-death resume
of the canonical Chao, authenticated observation/care round trips, stale/offline
labels and lease/revocation checks. Generated TS bridges or native filenames in
a README do not supply those capabilities.

References: [workstream 2](../../coordination/WORKSTREAMS.md),
[protocol dependencies](../../coordination/PROTOCOL.md),
[current Watch](../../../WATCH.md). Exact widget contract remains coordinated
work; this record invents no new service endpoint.
