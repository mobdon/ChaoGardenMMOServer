/**
 * Property-style and table-driven tests for the deterministic simulation.
 * Run with: npx vitest packages/simulation
 */
import { describe, expect, it } from 'vitest';

import { createCreature } from '../src/factories';
import { runSimulation, replayFrom } from '../src/simulation';
import { createRandomSource } from '../src/random';
import { toFixed, FIXED_POINT_SCALE, divRound } from '../src/fixed';
import { checkpoint, loadCheckpoint } from '../src/checkpoint';
import {
  SIMULATION_VERSION, SimCommand, MAX_NEED, OFFLINE_CAP_MS, OFFLINE_HARD_CAP_MS, OFFLINE_MOOD_FLOOR,
} from '../src';

const HOUR = 3_600_000;

function base(id = 'chao-1', now = 1_000_000) {
  return createCreature({ creatureId: id, nowMs: now });
}

describe('fixed point helpers', () => {
  it('round-trips values deterministically', () => {
    for (const v of [0, 0.5, 0.745, 10, 123.456]) {
      expect(Number(toFixed(v))).toBe(Math.round(v * FIXED_POINT_SCALE));
    }
  });

  it('divRound rounds half away from zero', () => {
    expect(divRound(5n, 2n)).toBe(3n);
    expect(divRound(-5n, 2n)).toBe(-3n);
    expect(divRound(4n, 2n)).toBe(2n);
  });
});

describe('seeded randomness', () => {
  it('produces identical sequences for identical seeds', () => {
    const a = createRandomSource('seed-a');
    const b = createRandomSource('seed-a');
    const seqA = Array.from({ length: 20 }, () => a.nextInt(100));
    const seqB = Array.from({ length: 20 }, () => b.nextInt(100));
    expect(seqA).toEqual(seqB);
  });

  it('different seeds diverge', () => {
    const a = createRandomSource('a');
    const b = createRandomSource('b');
    expect(Array.from({ length: 10 }, () => a.nextInt(1 << 30)))
      .not.toEqual(Array.from({ length: 10 }, () => b.nextInt(1 << 30)));
  });

  it('bounds respect maxExclusive', () => {
    const r = createRandomSource(7n);
    for (let i = 0; i < 500; i++) {
      const v = r.nextInt(37);
      expect(v).toBeGreaterThanOrEqual(0);
      expect(v).toBeLessThan(37);
    }
  });
});

describe('needs decay', () => {
  it('decays hunger over time deterministically', () => {
    const a = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + HOUR }]);
    const b = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + HOUR }]);
    expect(a.snapshot.hunger).toBe(b.snapshot.hunger);
    expect(a.snapshot.hunger).toBeLessThan(7000n);
    expect(a.events.filter((e) => e.kind === 'needsTicked').length).toBeGreaterThanOrEqual(0);
  });

  it('sleeping restores energy and slows hunger', () => {
    const asleep = runSimulation(base(), [
      { type: 'sleep', atMs: 1_000_000 },
      { type: 'check', atMs: 1_000_000 + HOUR },
    ]);
    expect(asleep.snapshot.energy).toBeGreaterThan(7000n);
    const awake = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + HOUR }]);
    expect(asleep.snapshot.hunger).toBeGreaterThan(awake.snapshot.hunger);
  });

  it('needs never leave [0, MAX]', () => {
    const r = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + 10_000 * HOUR }]);
    expect(r.snapshot.hunger).toBe(0n);
    expect(r.snapshot.energy).toBe(0n);
    expect(r.snapshot.hygiene).toBe(0n);
    expect(r.snapshot.mood).toBeGreaterThanOrEqual(0n);
    expect(r.snapshot.mood).toBeLessThanOrEqual(MAX_NEED);
  });
});

describe('offline caps', () => {
  it('caps mood at OFFLINE_MOOD_FLOOR after a very long absence', () => {
    const drained = { ...base(), mood: 0n };
    const r = runSimulation(drained, [{ type: 'check', atMs: 1_000_000 + 400 * HOUR }], { offline: true });
    expect(r.snapshot.mood).toBeGreaterThanOrEqual(OFFLINE_MOOD_FLOOR);
  });

  it('stops simulating needs past the hard cap but still counts age', () => {
    const long = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + 1_000 * HOUR }]);
    const hard = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + Number(OFFLINE_HARD_CAP_MS) + 10 }]);
    // Age (stage) keeps progressing; needs identical to the hard-cap run.
    expect(long.snapshot.stage).toBe('elder');
    // Needs identical at and beyond the hard cap (extra time is discarded).
    expect(long.snapshot.hunger).toBe(hard.snapshot.hunger);
    expect(long.snapshot.hunger).toBeGreaterThan(0n);
  });

  it('reduced-rate window decays slower than full rate', () => {
    const withinCap = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + Number(OFFLINE_CAP_MS) }], { offline: true });
    const pastCap = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + Number(OFFLINE_CAP_MS) + Number(OFFLINE_CAP_MS) }], { offline: true });
    const fullDelta = 7000n - withinCap.snapshot.hunger;
    const extraDelta = withinCap.snapshot.hunger - pastCap.snapshot.hunger;
    expect(extraDelta).toBeLessThan(fullDelta);
    expect(extraDelta).toBeGreaterThan(0n);
  });
});

describe('clock rollback protection', () => {
  it('rejects commands stamped before the watermark', () => {
    const r = runSimulation(base(), [
      { type: 'check', atMs: 1_000_000 + HOUR },
      { type: 'feed', atMs: 1_000_000 - 5 },
    ]);
    expect(r.events.some((e) => e.kind === 'actionRejected' && e.reason === 'invalid')).toBe(true);
    expect(r.snapshot.hunger).toBe(7000n);
  });

  it('growth never regresses with a rolled-back clock', () => {
    const r = runSimulation(base(), [
      { type: 'check', atMs: 1_000_000 + 30 * HOUR }, // child
      { type: 'check', atMs: 1_000_000 + 2 * HOUR }, // rollback
    ]);
    expect(r.snapshot.stage).toBe('child');
  });
});

describe('commands', () => {
  it('feed raises hunger and mood and pays care rings', () => {
    const r = runSimulation(base(), [{ type: 'feed', atMs: 1_000_000 }]);
    expect(r.snapshot.hunger).toBe(7000n + 4000n > MAX_NEED ? MAX_NEED : 11000n);
    expect(r.events.some((e) => e.kind === 'reward' && e.reason === 'care')).toBe(true);
  });

  it('enforces the action cooldown', () => {
    const r = runSimulation(base(), [
      { type: 'feed', atMs: 1_000_000 },
      { type: 'feed', atMs: 1_000_000 + 1_000 },
    ]);
    expect(r.events.some((e) => e.kind === 'actionRejected' && e.reason === 'cooldown')).toBe(true);
  });

  it('applies simultaneous commands in a stable order', () => {
    const cmds: SimCommand[] = [
      { type: 'feed', atMs: 1_000_000 },
      { type: 'train', stat: 'run', minutes: 60, atMs: 1_000_000 },
    ];
    const a = runSimulation(base(), cmds);
    const b = runSimulation(base(), [...cmds].reverse());
    // Different order => different clamping outcomes, yet each replay is reproducible.
    expect(a.snapshot.hunger).not.toBe(b.snapshot.hunger);
    expect(runSimulation(base(), cmds).snapshot.hunger).toBe(a.snapshot.hunger);
    expect(runSimulation(base(), [...cmds].reverse()).snapshot.hunger).toBe(b.snapshot.hunger);
  });
});

describe('training', () => {
  it('gains scale with minutes and respects level thresholds', () => {
    const r = runSimulation(base(), [
      { type: 'feed', atMs: 1_000_000 },
      { type: 'train', stat: 'run', minutes: 60, atMs: 1_000_000 },
    ]);
    expect(r.snapshot.stats.run.points).toBe(600);
    expect(r.snapshot.stats.run.level).toBe(7);
    expect(r.events.some((e) => e.kind === 'levelUp' && e.stat === 'run')).toBe(true);
  });

  it('session is cut short before energy hits zero', () => {
    const low = { ...base(), energy: 1000n };
    const r = runSimulation(low, [{ type: 'train', stat: 'power', minutes: 60, atMs: 1_000_000 }]);
    expect(r.snapshot.energy).toBeGreaterThanOrEqual(0n);
    expect(r.snapshot.stats.power.points).toBe(40 * 10); // 1000/25 = 40 minutes
  });

  it('caps minutes at TRAIN_MAX_MINUTES', () => {
    const r = runSimulation(base(), [
      { type: 'feed', atMs: 1_000_000 },
      { type: 'train', stat: 'swim', minutes: 100000, atMs: 1_000_000 },
    ]);
    expect(r.snapshot.stats.swim.points).toBe(600);
  });

  it('sickness halves training gains', () => {
    const sick = { ...base(), sick: true };
    const r = runSimulation(sick, [{ type: 'train', stat: 'fly', minutes: 60, atMs: 1_000_000 }]);
    expect(r.snapshot.stats.fly.points).toBe(300);
  });
});

describe('growth and evolution', () => {
  const stages = [
    [0.5, 'egg'], [1, 'baby'], [4.9, 'baby'], [5, 'child'], [19.9, 'child'],
    [20, 'adult'], [99.9, 'adult'], [100, 'elder'],
  ] as const;

  it.each(stages.map((s) => [s[0], s[1]]))('age %i hours -> %s', (hours, expected) => {
    const r = runSimulation(base(), [{ type: 'check', atMs: 1_000_000 + Math.round(hours * HOUR) }]);
    expect(r.snapshot.stage).toBe(expected);
  });

  it('evolves once at adulthood with hero trait and Aqua type', () => {
    const hero = { ...base(), heroPoints: 800, stats: { ...base().stats, swim: { grade: 1, level: 5, points: 900 } } };
    const r = runSimulation(hero, [{ type: 'check', atMs: 1_000_000 + 20 * HOUR }]);
    expect(r.snapshot.evolved).toBe(true);
    expect(r.snapshot.trait).toBe('hero');
    expect(r.snapshot.typeAdjective).toBe('Aqua');
    expect(r.events.filter((e) => e.kind === 'evolved')).toHaveLength(1);
  });

  it('dark alignment yields dark trait', () => {
    const dark = { ...base(), darkPoints: 900 };
    const r = runSimulation(dark, [{ type: 'check', atMs: 1_000_000 + 20 * HOUR }]);
    expect(r.snapshot.trait).toBe('dark');
  });

  it('balanced alignment yields neutral trait', () => {
    const r = runSimulation({ ...base(), heroPoints: 100, darkPoints: 100 }, [{ type: 'check', atMs: 1_000_000 + 20 * HOUR }]);
    expect(r.snapshot.trait).toBe('neutral');
  });

  it('fly gate falls back to runner-up stat', () => {
    const stats = {
      ...base().stats,
      swim: { grade: 1, level: 1, points: 100 },
      fly: { grade: 1, level: 1, points: 250 },
      run: { grade: 1, level: 1, points: 50 },
    };
    const r = runSimulation({ ...base(), stats }, [{ type: 'check', atMs: 1_000_000 + 20 * HOUR }]);
    // fly below the gate, so the runner-up (swim, 100) wins over run (50).
    expect(r.snapshot.typeAdjective).toBe('Aqua');
  });
});

describe('replay determinism', () => {
  it('checkpoint + ordered commands is reproducible', () => {
    const cmds: SimCommand[] = [
      { type: 'feed', atMs: 1_000_000 + 1_000 },
      { type: 'train', stat: 'stamina', minutes: 30, atMs: 1_000_000 + 2_000 },
      { type: 'play', atMs: 1_000_000 + 3_000 },
      { type: 'check', atMs: 1_000_000 + 26 * HOUR },
    ];
    const first = runSimulation(base(), cmds);
    const cp = checkpoint(first.snapshot, cmds.length);
    const second = replayFrom({ snapshot: cp.snapshot, commandIndex: cp.commandIndex }, []);
    expect(second.snapshot).toEqual(first.snapshot);
    const again = runSimulation(base(), cmds);
    expect(again.snapshot).toEqual(first.snapshot);
  });

  it('checkpoints migrate forward and reject future versions', () => {
    const cp = checkpoint(base(), 0);
    const loaded = loadCheckpoint({ version: cp.version, snapshot: cp.snapshot as unknown as Record<string, unknown>, commandIndex: 0 });
    expect(loaded.version).toBe(SIMULATION_VERSION);
    expect(() => loadCheckpoint({ version: SIMULATION_VERSION + 1, snapshot: {}, commandIndex: 0 })).toThrow();
  });
});

describe('neglect', () => {
  it('emits neglectSick after sustained critical needs', () => {
    const starving = { ...base(), hunger: 1000n, energy: 1000n, hygiene: 1000n };
    const r = runSimulation(starving, [{ type: 'check', atMs: 1_000_000 + 7 * HOUR }]);
    expect(r.snapshot.sick).toBe(true);
    expect(r.events.some((e) => e.kind === 'neglectSick')).toBe(true);
  });

  it('recovers when cared for', () => {
    const starving = { ...base(), hunger: 1000n, energy: 1000n, hygiene: 1000n, sick: true };
    const r = runSimulation(starving, [
      { type: 'feed', atMs: 1_000_000 },
      { type: 'clean', atMs: 1_000_000 + 11_000 },
      { type: 'sleep', atMs: 1_000_000 + 22_000 },
      { type: 'check', atMs: 1_000_000 + 3 * HOUR },
    ]);
    expect(r.snapshot.sick).toBe(false);
    expect(r.events.some((e) => e.kind === 'recovered')).toBe(true);
  });
});
