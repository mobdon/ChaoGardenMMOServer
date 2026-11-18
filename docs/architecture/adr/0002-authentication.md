# ADR-0002 — Canonical identity and device ownership

Status: current bearer protocol; v2 pairing/leases proposed, November 18, 2026.

## Current behavior

`POST /api/session` resumes a bearer credential and returns `{token, player}`.
`resumeOnly:true` rejects unknown/missing credentials without creating another
Chao. Saved profiles contain token hashes. Raw WebSockets authenticate through
the current token query. Multiple sockets can share one profile and compete
for its controls; exclusive device ownership is not established by that flow.

## Required extension

Implement short-lived desktop-approved pairing, revocable device credentials,
passive observation and explicit activity takeover in Rust. Do not invent a
second user/wallet model or assume generated refresh/JWT routes are connected.
Apply the agreed authorization guard to every mutation, including separate
Save/Watch paths, and reject revoked already-connected devices.

Account metadata, leases/revisions and operation receipts must preserve the
canonical Chao and join its durable transaction. New fields cannot rely on the
current whitelist serializer to preserve them automatically. Legacy/unknown
field preservation and failure rollback need explicit migration proof.

The [v2 proposal](../../coordination/PROTOCOL.md) owns request/error details.
Migrate desktop, Tiny and widget together before enabling it; avoid permanent
tokens in future URL/QR payloads and redact current credentials from logs.

Evidence entry points: [protocol tests](../../../tests/protocol.rs),
[profile tests](../../../tests/game.rs), [service](../../../src/service.rs).
This record does not accept the isolated accounts work or run a migration.
