/**
 * Balancing constants. Every value is an integer (fixed-point where noted).
 * Changing any value is a balancing change: bump SIMULATION_VERSION and add a
 * migration so existing checkpoints keep their meaning.
 */

export const MAX_NEED = 10_000n;

// Per-hour decay rates (fixed-point units per hour).
export const HUNGER_DECAY_PER_HOUR = 125n;
export const ENERGY_DECAY_PER_HOUR = 90n;
export const HYGIENE_DECAY_PER_HOUR = 70n;
export const MOOD_DECAY_PER_HOUR = 40n;

// Sleeping recovery.
export const ENERGY_RECOVER_PER_HOUR = 2500n;
export const SLEEP_HUNGER_DECAY_DIVISOR = 2n;

// Mood drivers.
export const MOOD_HUNGER_LOW = -20n;
export const MOOD_ENERGY_LOW = -15n;
export const MOOD_HYGIENE_LOW = -18n;
export const MOOD_CARED = 5n;

export const HUNGER_CRITICAL = 2_000n;
export const ENERGY_CRITICAL = 1_500n;
export const HYGIENE_CRITICAL = 2_500n;
export const MOOD_SICK_CAP = 3_000n;

// Actions.
export const FEED_HUNGER = 4_000n;
export const FEED_MOOD = 600n;
export const CLEAN_MOOD = 400n;
export const PLAY_MOOD = 1_500n;
export const PLAY_ENERGY = 800n;
export const PLAY_HUNGER = 400n;
export const PET_MOOD = 500n;

export const ACTION_COOLDOWN_MS = 10_000n;

// Training.
export const TRAIN_POINTS_PER_MINUTE = 10;
export const TRAIN_POINTS_PER_LEVEL = 100;
export const MAX_LEVEL = 99;
export const GRADE_THRESHOLDS = [25, 50, 75] as const;
export const TRAIN_ENERGY_PER_MINUTE = 25n;
export const TRAIN_HUNGER_PER_MINUTE = 20n;
export const TRAIN_MOOD_PER_MINUTE = 2n;
export const TRAIN_MAX_MINUTES = 60;

// Growth.
export const STAGE_HOUR_LIMITS = [1, 5, 20, 100] as const;
export const STAGES = ['egg', 'baby', 'child', 'adult', 'elder'] as const;

// Evolution.
export const ALIGNMENT_THRESHOLD = 500;
export const EVOLUTION_FLY_GATE = 300;
export const HERO_POINTS_PER_FEED = 5;

// Neglect.
export const NEGLECT_GRACE_MS = 30 * 60_000n;
export const NEGLECT_SICK_MS = 6 * 3_600_000n;
export const NEGLECT_STAT_DECAY_PER_HOUR = 5n;

// Offline caps.
export const OFFLINE_CAP_MS = 12n * 3_600_000n;
export const OFFLINE_REDUCED_RATE = 4n; // quarter rate (divide by this)
export const OFFLINE_HARD_CAP_MS = 48n * 3_600_000n;
export const OFFLINE_MOOD_FLOOR = 1_000n;

// Rewards.
export const CARE_REWARD_RINGS = 1;
export const MILESTONE_REWARD_RINGS = 25;
