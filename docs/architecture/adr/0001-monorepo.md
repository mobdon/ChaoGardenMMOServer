# ADR-0001 — Monorepo Structure

**Status:** Accepted

## Context
The mobile client, API, shared contracts, and deterministic simulation must
evolve together; simulation code must be byte-identical on client and server.

## Decision
Use a pnpm + Turborepo TypeScript monorepo (already present) with:

- `apps/mobile` — Expo React Native client (including native widget code).
- `apps/api` — NestJS API + WS gateway.
- `packages/contracts` — Zod schemas for REST bodies, WS events, error codes.
- `packages/simulation` — pure, dependency-free, fixed-point game logic.
- `packages/config` — shared TS/ESLint/Prettier config.

The existing Rust prototype (crates, `src/`) remains as a separate authority
prototype; it is not part of the MVP production runtime.

## Consequences
- `packages/simulation` must have **zero** platform imports (enforced by an
  ESLint rule and a CI check importing it in a bare Node context).
- Contracts changes are atomic across client and server within one commit.
- Turborepo caches build/test per package; release via changesets or tags.
- Native widget code lives under `apps/mobile/ios` / `apps/mobile/android`
  but all data it reads comes from the shared app-group store (ADR-0005).
