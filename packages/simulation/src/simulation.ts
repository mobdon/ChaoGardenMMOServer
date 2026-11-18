import { CreatureSnapshot, DomainEvent, SimCommand, SimResult } from './types';
import { clampBig } from './fixed';
import { advanceTime, evolveIfNeeded, maybeBecomeSick } from './growth';
import { applyCommand } from './commands';

/** Ensure a rollback attempt never regresses growth or needs. */
function guardClock(snapshot: CreatureSnapshot, atMs: number, events: DomainEvent[]): CreatureSnapshot {
  const watermark = Math.max(snapshot.highWatermark, snapshot.lastTickedAt);
  if (atMs < watermark) {
    events.push({ kind: 'actionRejected', atMs, type: 'check', reason: 'invalid' });
    return snapshot;
  }
  return snapshot;
}

/**
 * Advance needs for [snapshot.lastTickedAt, toMs) and return the new snapshot.
 * Offline progress is capped in advanceTime.
 */
export function tickTo(snapshot: CreatureSnapshot, toMs: number, offline = false): { snapshot: CreatureSnapshot; events: DomainEvent[] } {
  const events: DomainEvent[] = [];
  if (toMs <= snapshot.lastTickedAt) return { snapshot, events };
  const next = advanceTime(snapshot, toMs, offline, events);
  const evolved = evolveIfNeeded(next, events);
  const sick = maybeBecomeSick(evolved, events);
  return {
    snapshot: {
      ...sick,
      lastTickedAt: toMs,
      highWatermark: Math.max(sick.highWatermark, toMs),
    },
    events,
  };
}

/**
 * Apply an ordered list of commands starting from a checkpoint.
 * Commands must be sorted by atMs; equal timestamps apply in given order.
 */
export interface ReplayOptions {
  /** True when simulating an absence (applies offline caps). */
  readonly offline?: boolean;
}

export function replayFrom(
  checkpoint: { snapshot: CreatureSnapshot; commandIndex: number },
  commands: readonly SimCommand[],
  options: ReplayOptions = {},
): SimResult {
  let snapshot = checkpoint.snapshot;
  const events: DomainEvent[] = [];
  let lastMs = snapshot.lastTickedAt;
  for (const cmd of commands) {
    const guard = guardClock(snapshot, cmd.atMs, events);
    if (guard !== snapshot) continue; // rejected rollback
    snapshot = guard;
    // Simulate elapsed time up to the command.
    const elapsed = tickTo(snapshot, cmd.atMs, options.offline ?? false);
    snapshot = elapsed.snapshot;
    events.push(...elapsed.events);
    // Now apply the discrete action.
    const applied = applyCommand(snapshot, cmd);
    snapshot = applied.snapshot;
    events.push(...applied.events);
    lastMs = cmd.atMs;
  }
  return { snapshot: { ...snapshot, lastTickedAt: Math.max(snapshot.lastTickedAt, lastMs) }, events };
}

/** Pure helper for tests: clamp all needs into valid range. */
export function sanitize(snapshot: CreatureSnapshot): CreatureSnapshot {
  const MAX = 10_000n;
  return {
    ...snapshot,
    hunger: clampBig(snapshot.hunger, 0n, MAX),
    energy: clampBig(snapshot.energy, 0n, MAX),
    hygiene: clampBig(snapshot.hygiene, 0n, MAX),
    mood: clampBig(snapshot.mood, 0n, MAX),
  };
}

