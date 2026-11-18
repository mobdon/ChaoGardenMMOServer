# ADR-0002 — Authentication

**Status:** Accepted

## Context
A mobile MMO for a general audience needs simple, safe sign-in, revocable
sessions, and no passwords in native widgets or logs.

## Decision
- Sign-in options at MVP: email + password (argon2id hashing) and Apple /
  Google sign-in. No anonymous accounts (app-store review friendliness).
- **JWT access token** (15 min TTL, asymmetric ES256, kid rotation) +
  **opaque rotating refresh token** (30 day TTL, stored hashed in Postgres,
  single-use — reuse of a rotated token revokes the family).
- Refresh endpoint issues new pairs; device-bound via a device ID recorded at
  refresh time (max 5 active devices, oldest evicted).
- WS gateway authenticates with the access token on connect and re-auths on
  token refresh via a `auth.rotate` client event.
- Password reset via signed, single-use, 15-minute email links.
- Rate limit login attempts in Redis: 5/15 min per account + per IP.
- Widgets never hold tokens; they read locally cached gameplay data written
  by the app (ADR-0005).

## Consequences
- No server-side session lookup per request (stateless access tokens), but
  revocation within 15 min requires a token denylist (Redis, keyed by jti).
- Key rotation is operational: JWKS published by the API, keys rotated ≥ 90 d.
