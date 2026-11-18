/**
 * Stat training: converts minutes into stat points with sickness reduction,
 * energy/hunger cost, and a hard cap so a session can never push energy
 * negative. All integer math.
 */

import {
  MAX_NEED, TRAIN_ENERGY_PER_MINUTE, TRAIN_HUNGER_PER_MINUTE,
  TRAIN_MOOD_PER_MINUTE, TRAIN_POINTS_PER_LEVEL, TRAIN_POINTS_PER_MINUTE,
} from './constants';
import { clampBig } from './fixed';
import { AllStatName, CreatureSnapshot, DomainEvent } from './types';

export function applyTraining(
  snapshot: CreatureSnapshot,
  stat: AllStatName,
  minutes: number,
): { snapshot: CreatureSnapshot; events: DomainEvent[] } {
  const events: DomainEvent[] = [];
  const energyCostPerMin = TRAIN_ENERGY_PER_MINUTE;
  const hungerCostPerMin = TRAIN_HUNGER_PER_MINUTE;

  // Largest whole-minute session that never drives energy or hunger below 0.
  const energyBudget = Number(snapshot.energy / energyCostPerMin);
  const hungerBudget = Number(snapshot.hunger / hungerCostPerMin);
  const affordable = Math.min(minutes, energyBudget, hungerBudget);
  if (affordable <= 0) return { snapshot, events };

  // Sickness halves gains (integer, floor).
  const baseGain = affordable * TRAIN_POINTS_PER_MINUTE;
  const gain = snapshot.sick ? Math.floor(baseGain / 2) : baseGain;

  const statValue = snapshot.stats[stat];
  const newPoints = statValue.points + gain;
  const newLevel = Math.min(99, 1 + Math.floor(newPoints / TRAIN_POINTS_PER_LEVEL));

  if (newLevel > statValue.level) {
    events.push({ kind: 'levelUp', atMs: snapshot.lastTickedAt, stat, level: newLevel });
    events.push({ kind: 'reward', atMs: snapshot.lastTickedAt, rings: 25, reason: 'milestone' });
  }
  events.push({ kind: 'reward', atMs: snapshot.lastTickedAt, rings: 1, reason: 'training' });

  return {
    snapshot: {
      ...snapshot,
      stats: { ...snapshot.stats, [stat]: { ...statValue, points: newPoints, level: newLevel } },
      energy: clampBig(snapshot.energy - BigInt(affordable) * energyCostPerMin, 0n, MAX_NEED),
      hunger: clampBig(snapshot.hunger - BigInt(affordable) * hungerCostPerMin, 0n, MAX_NEED),
      mood: clampBig(snapshot.mood - BigInt(affordable) * TRAIN_MOOD_PER_MINUTE, 0n, MAX_NEED),
    },
    events,
  };
}
