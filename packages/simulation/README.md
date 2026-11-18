# @chaogarden/simulation

Deterministic chao creature simulation shared by the authoritative server and
offline tooling. Pure TypeScript: no network, database, React, or wall-clock
access. Time (`atMs`) and randomness (seeded `createRandomSource`) are injected.

- Formulas & balancing rules: `docs/game-design/simulation.md`
- All gameplay math uses integer fixed-point (`src/fixed.ts`, scale 1000).
- Output is typed domain events (`src/types.ts`); callers own persistence.
- Checkpoints: `src/checkpoint.ts` with version migrations (`SIMULATION_VERSION`).

Replay: `runSimulation(snapshot, commands, { offline })` advances from a
checkpoint across an ordered command list, rejecting clock-rollback commands
and capping offline progress. Tests: `npx vitest run` inside this package.
