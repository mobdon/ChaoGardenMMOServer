/**
 * Deterministic creature simulation for ChaoGarden.
 *
 * Pure TypeScript domain library: no network, database, React, or wall-clock
 * dependency. Time and randomness are injected; all output is typed domain
 * events so persistence stays in the callers.
 */

export * from './types';
export * from './constants';
export * from './fixed';
export * from './random';
export * from './growth';
export * from './commands';
export * from './training';
export * from './simulation';
export * from './checkpoint';

import { CreatureSnapshot, SimCommand, SimResult } from './types';
import { replayFrom, ReplayOptions } from './simulation';

/**
 * Primary entry point: replay a checkpoint plus ordered commands.
 * `options.offline` applies the offline progress caps (see docs/game-design).
 */
export function runSimulation(
  snapshot: CreatureSnapshot,
  commands: readonly SimCommand[],
  options: ReplayOptions = {},
): SimResult {
  return replayFrom({ snapshot, commandIndex: 0 }, commands, options);
}
