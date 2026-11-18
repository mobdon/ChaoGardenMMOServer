/**
 * Checkpoint persistence helpers and version migrations.
 *
 * A checkpoint is an immutable snapshot of the creature at a known command
 * index. Future balancing changes bump SIMULATION_VERSION and register a
 * migration here so old checkpoints keep their meaning.
 */

import { SIMULATION_VERSION, CreatureCheckpoint, CreatureSnapshot } from './types';

type Migration = (snapshot: Record<string, unknown>) => Record<string, unknown>;

const MIGRATIONS: Record<number, Migration> = {
  // Example for v0 -> v1 (left as documentation):
  // 0: (old) => ({ ...old, trait: old.trait ?? 'neutral' }),
};

export function checkpoint(snapshot: CreatureSnapshot, commandIndex: number): CreatureCheckpoint {
  return { version: SIMULATION_VERSION, snapshot, commandIndex };
}

/**
 * Load a checkpoint, migrating it forward if needed.
 * Throws on checkpoints from an unknown future version.
 */
export function loadCheckpoint(raw: { version: number; snapshot: Record<string, unknown>; commandIndex: number }): CreatureCheckpoint {
  if (raw.version > SIMULATION_VERSION) {
    throw new Error(`simulation: checkpoint version ${raw.version} is newer than supported ${SIMULATION_VERSION}`);
  }
  let data = raw.snapshot;
  for (let v = raw.version; v < SIMULATION_VERSION; v++) {
    const migrate = MIGRATIONS[v];
    if (migrate) data = migrate(data);
  }
  return { version: SIMULATION_VERSION, snapshot: data as unknown as CreatureSnapshot, commandIndex: raw.commandIndex };
}
