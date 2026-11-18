/**
 * Discrete care/training commands. Pure: snapshot in, snapshot + events out.
 */

import {
  ACTION_COOLDOWN_MS, CARE_REWARD_RINGS, CLEAN_MOOD, FEED_HUNGER, FEED_MOOD,
  HERO_POINTS_PER_FEED, MAX_NEED, MILESTONE_REWARD_RINGS, PET_MOOD,
  PLAY_ENERGY, PLAY_HUNGER, PLAY_MOOD, TRAIN_ENERGY_PER_MINUTE,
  TRAIN_HUNGER_PER_MINUTE, TRAIN_MAX_MINUTES, TRAIN_MOOD_PER_MINUTE,
} from './constants';
import { clampBig } from './fixed';
import { applyTraining } from './training';
import { CreatureSnapshot, DomainEvent, SimCommand } from './types';

function clampNeed(v: bigint): bigint {
  return clampBig(v, 0n, MAX_NEED);
}

function withinCooldown(snapshot: CreatureSnapshot, cmd: SimCommand): boolean {
  const last = snapshot.lastActionAt[cmd.type];
  return last !== undefined && BigInt(cmd.atMs - last) < ACTION_COOLDOWN_MS;
}

export function applyCommand(
  snapshot: CreatureSnapshot,
  cmd: SimCommand,
): { snapshot: CreatureSnapshot; events: DomainEvent[] } {
  const events: DomainEvent[] = [];

  if (cmd.type !== 'check' && withinCooldown(snapshot, cmd)) {
    events.push({ kind: 'actionRejected', atMs: cmd.atMs, type: cmd.type, reason: 'cooldown' });
    return { snapshot, events };
  }

  switch (cmd.type) {
    case 'feed': {
      const item = cmd.item ?? 'basic';
      let heroPoints = snapshot.heroPoints;
      let darkPoints = snapshot.darkPoints;
      if (item === 'hero') heroPoints += HERO_POINTS_PER_FEED;
      if (item === 'dark') darkPoints += HERO_POINTS_PER_FEED;
      return {
        snapshot: {
          ...snapshot,
          hunger: clampNeed(snapshot.hunger + FEED_HUNGER),
          mood: clampNeed(snapshot.mood + FEED_MOOD),
          heroPoints,
          darkPoints,
          lastActionAt: { ...snapshot.lastActionAt, feed: cmd.atMs },
        },
        events: [...events, { kind: 'reward', atMs: cmd.atMs, rings: CARE_REWARD_RINGS, reason: 'care' }],
      };
    }
    case 'sleep':
      return { snapshot: { ...snapshot, sleeping: true, lastActionAt: { ...snapshot.lastActionAt, sleep: cmd.atMs } }, events };
    case 'wake':
      return { snapshot: { ...snapshot, sleeping: false, lastActionAt: { ...snapshot.lastActionAt, wake: cmd.atMs } }, events };
    case 'clean':
      return {
        snapshot: {
          ...snapshot,
          hygiene: MAX_NEED,
          mood: clampNeed(snapshot.mood + CLEAN_MOOD),
          lastActionAt: { ...snapshot.lastActionAt, clean: cmd.atMs },
        },
        events: [...events, { kind: 'reward', atMs: cmd.atMs, rings: CARE_REWARD_RINGS, reason: 'care' }],
      };
    case 'play':
      return {
        snapshot: {
          ...snapshot,
          mood: clampNeed(snapshot.mood + PLAY_MOOD),
          energy: clampNeed(snapshot.energy - PLAY_ENERGY),
          hunger: clampNeed(snapshot.hunger - PLAY_HUNGER),
          lastActionAt: { ...snapshot.lastActionAt, play: cmd.atMs },
        },
        events: [...events, { kind: 'reward', atMs: cmd.atMs, rings: CARE_REWARD_RINGS, reason: 'care' }],
      };
    case 'pet':
      return {
        snapshot: {
          ...snapshot,
          mood: clampNeed(snapshot.mood + PET_MOOD),
          lastActionAt: { ...snapshot.lastActionAt, pet: cmd.atMs },
        },
        events,
      };
    case 'train': {
      const stat = cmd.stat;
      if (!stat) {
        events.push({ kind: 'actionRejected', atMs: cmd.atMs, type: 'train', reason: 'invalid' });
        return { snapshot, events };
      }
      const minutes = Math.max(0, Math.min(TRAIN_MAX_MINUTES, Math.floor(cmd.minutes ?? 0)));
      const result = applyTraining(snapshot, stat, minutes);
      return {
        snapshot: {
          ...result.snapshot,
          lastActionAt: { ...result.snapshot.lastActionAt, train: cmd.atMs },
        },
        events: [...events, ...result.events],
      };
    }
    case 'check':
      return { snapshot, events };
    default:
      return { snapshot, events };
  }
}

export { MILESTONE_REWARD_RINGS };
