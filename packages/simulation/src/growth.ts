/**
 * Time advancement: need decay, mood, sleeping, offline caps, neglect,
 * growth stages and evolution. All math is integer fixed-point.
 */

import {
  ALIGNMENT_THRESHOLD, ENERGY_CRITICAL, EVOLUTION_FLY_GATE, ENERGY_DECAY_PER_HOUR, ENERGY_RECOVER_PER_HOUR,
  HUNGER_CRITICAL, HUNGER_DECAY_PER_HOUR, HYGIENE_CRITICAL, HYGIENE_DECAY_PER_HOUR,
  MAX_NEED, MOOD_CARED, MOOD_DECAY_PER_HOUR, MOOD_HUNGER_LOW, MOOD_SICK_CAP,
  NEGLECT_GRACE_MS, NEGLECT_SICK_MS, OFFLINE_CAP_MS, OFFLINE_HARD_CAP_MS,
  OFFLINE_MOOD_FLOOR, OFFLINE_REDUCED_RATE, SLEEP_HUNGER_DECAY_DIVISOR, STAGES,
  STAGE_HOUR_LIMITS,
} from './constants';
import { clampBig, scalePerHour } from './fixed';
import { CreatureSnapshot, CreatureStage, DomainEvent, EvolutionTrait, StatName } from './types';

export const STAT_ORDER: readonly StatName[] = ['swim', 'fly', 'run', 'power', 'stamina'];

export const TYPE_ADJECTIVES: Record<StatName, string> = {
  swim: 'Aqua', fly: 'Sky', run: 'Swift', power: 'Brute', stamina: 'Tough',
};

export function isNeglected(s: CreatureSnapshot): boolean {
  return s.hunger < HUNGER_CRITICAL || s.energy < ENERGY_CRITICAL || s.hygiene < HYGIENE_CRITICAL;
}

/** Advance decay + mood + neglect for the interval, honouring offline caps. */
export function advanceTime(
  snapshot: CreatureSnapshot,
  toMs: number,
  offline: boolean,
  events: DomainEvent[],
): CreatureSnapshot {
  const span = BigInt(toMs - snapshot.lastTickedAt);
  const cappedSpan = offline && span > OFFLINE_HARD_CAP_MS ? OFFLINE_HARD_CAP_MS : span;
  const fullMs = offline ? (cappedSpan > OFFLINE_CAP_MS ? OFFLINE_CAP_MS : cappedSpan) : cappedSpan;
  const reducedMs = cappedSpan - fullMs;

  const hungerRate = snapshot.sleeping ? HUNGER_DECAY_PER_HOUR / SLEEP_HUNGER_DECAY_DIVISOR : HUNGER_DECAY_PER_HOUR;
  const energyRate = snapshot.sleeping ? -ENERGY_RECOVER_PER_HOUR : ENERGY_DECAY_PER_HOUR;

  let hunger = snapshot.hunger;
  let energy = snapshot.energy;
  let hygiene = snapshot.hygiene;
  let mood = snapshot.mood;

  const applyWindow = (ms: bigint, rateDiv: bigint) => {
    hunger = clampBig(hunger - scalePerHour(hungerRate * rateDiv, ms), 0n, MAX_NEED);
    energy = clampBig(energy - scalePerHour(energyRate * rateDiv, ms), 0n, MAX_NEED);
    hygiene = clampBig(hygiene - scalePerHour(HYGIENE_DECAY_PER_HOUR * rateDiv, ms), 0n, MAX_NEED);
    // Mood decays at full rate in the first capped window only.
    const moodRate = rateDiv === 1n ? MOOD_DECAY_PER_HOUR : MOOD_DECAY_PER_HOUR;
    mood = clampBig(mood - scalePerHour(moodRate, ms), offline ? OFFLINE_MOOD_FLOOR : 0n, MAX_NEED);
  };
  applyWindow(fullMs, 1n);
  if (reducedMs > 0n) applyWindow(reducedMs, OFFLINE_REDUCED_RATE);

  // Mood drift from care state.
  const cared = !isNeglected(snapshot);
  mood = clampBig(
    mood + scalePerHour(cared ? MOOD_CARED : MOOD_HUNGER_LOW, cappedSpan),
    offline ? OFFLINE_MOOD_FLOOR : 0n,
    snapshot.sick ? MOOD_SICK_CAP : MAX_NEED,
  );

  let neglectAccumMs = snapshot.neglectAccumMs;
  let sick = snapshot.sick;
  if (isNeglected(snapshot)) {
    neglectAccumMs += cappedSpan;
    if (neglectAccumMs >= NEGLECT_SICK_MS && !sick) {
      sick = true;
      events.push({ kind: 'neglectSick', atMs: toMs });
    } else if (neglectAccumMs >= NEGLECT_GRACE_MS && neglectAccumMs - cappedSpan < NEGLECT_GRACE_MS) {
      events.push({ kind: 'neglectWarning', atMs: toMs });
    }
  } else {
    if (sick) events.push({ kind: 'recovered', atMs: toMs });
    neglectAccumMs = 0n;
    sick = false;
  }

  // Stage progression uses real elapsed life, never regresses.
  const stage = stageFor(toMs, snapshot.createdAt, snapshot.stage);

  return {
    ...snapshot,
    hunger, energy, hygiene, mood,
    neglectAccumMs,
    sick,
    stage,
  };
}

/** Stage derived from age, monotonically non-decreasing. */
export function stageFor(nowMs: number, createdAt: number, current: CreatureStage): CreatureStage {
  const hours = (nowMs - createdAt) / 3_600_000;
  let idx = 0;
  while (idx < STAGE_HOUR_LIMITS.length && hours >= STAGE_HOUR_LIMITS[idx]) idx++;
  const curIdx = STAGES.indexOf(current);
  return STAGES[Math.max(curIdx, idx)] as CreatureStage;
}

/**
 * Evolution: first time the creature reaches `adult` we compute traits from
 * lifetime stats. Deterministic; runs at most once.
 */
export function evolveIfNeeded(snapshot: CreatureSnapshot, events: DomainEvent[]): CreatureSnapshot {
  if (snapshot.evolved || snapshot.stage !== 'adult') return snapshot;
  const alignment = snapshot.heroPoints - snapshot.darkPoints;
  const trait: EvolutionTrait =
    alignment >= ALIGNMENT_THRESHOLD ? 'hero'
    : alignment <= -ALIGNMENT_THRESHOLD ? 'dark'
    : 'neutral';

  let dominant: StatName = STAT_ORDER[0];
  let best = -1;
  for (const stat of STAT_ORDER) {
    const pts = snapshot.stats[stat].points;
    if (pts > best) { best = pts; dominant = stat; }
  }
  if (dominant === 'fly' && snapshot.stats.fly.points < EVOLUTION_FLY_GATE) {
    // Fly gate not met: fall back to next-best stat.
    let altBest = -1;
    let alt: StatName = 'swim';
    for (const stat of STAT_ORDER) {
      if (stat === 'fly') continue;
      const pts = snapshot.stats[stat].points;
      if (pts > altBest) { altBest = pts; alt = stat; }
    }
    dominant = alt;
  }

  events.push({ kind: 'evolved', atMs: snapshot.lastTickedAt, trait, typeAdjective: TYPE_ADJECTIVES[dominant] });
  return { ...snapshot, evolved: true, trait, typeAdjective: TYPE_ADJECTIVES[dominant] };
}

export function maybeBecomeSick(snapshot: CreatureSnapshot, events: DomainEvent[]): CreatureSnapshot {
  return snapshot; // sickness handled inside advanceTime; kept as a hook.
}
