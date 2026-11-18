/**
 * Factory for fresh creature snapshots (used by servers spawning a new chao
 * and by tests). Deterministic: identical seed -> identical snapshot.
 */

import { createRandomSource } from './random';
import { AllStatName, CreatureSnapshot, StatValue } from './types';

const ALL_STATS: readonly AllStatName[] = ['swim', 'fly', 'run', 'power', 'stamina', 'luck', 'intelligence'];

export interface NewCreatureOptions {
  creatureId: string;
  nowMs: number;
  seed?: string;
}

export function makeStats(): Record<AllStatName, StatValue> {
  return Object.fromEntries(
    ALL_STATS.map((name) => [name, { grade: 1, level: 1, points: 0 }]),
  ) as Record<AllStatName, StatValue>;
}

export function createCreature(opts: NewCreatureOptions): CreatureSnapshot {
  const rng = createRandomSource(opts.seed ?? `${opts.creatureId}:${opts.nowMs}`);
  return {
    creatureId: opts.creatureId,
    createdAt: opts.nowMs,
    bornAt: opts.nowMs,
    highWatermark: opts.nowMs,
    lastTickedAt: opts.nowMs,
    hunger: 7000n,
    energy: 7000n,
    hygiene: 7000n,
    mood: 5000n,
    sleeping: false,
    neglectAccumMs: 0n,
    sick: false,
    stage: 'egg',
    evolved: false,
    trait: 'neutral',
    typeAdjective: null,
    stats: makeStats(),
    heroPoints: 0,
    darkPoints: 0,
    lastActionAt: {},
    rngState: rng.state,
  };
}
