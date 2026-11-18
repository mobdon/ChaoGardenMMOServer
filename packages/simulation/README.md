# @chaogarden/simulation

Deterministic, pure-TypeScript creature simulation shared by client and server.

- **No network, database, React, or wall-clock access.** Timestamps and the
  seeded RNG are always injected by the caller.
- **Fixed-point arithmetic** (`src/fixed.ts`) keeps every calculation exactly
  reproducible across JS runtimes.
- **Typed domain events** are returned to the caller; persistence never happens
  inside the package.
- **Replay-based authority**: `runSimulation(snapshot, commands, options)`
  advances a creature from a checkpoint plus an ordered command list, capping
  offline progress per `docs/game-design/simulation.md`.
- **Versioned metadata**: `SIMULATION_VERSION` must be checked (and migrated)
  before applying a snapshot; see `src/checkpoint.ts` and `SIMULATION_MIGRATIONS`.

Run tests with `pnpm vitest run` inside this package.
