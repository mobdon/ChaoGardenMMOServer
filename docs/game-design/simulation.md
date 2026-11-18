# Game Design — Deterministic Creature Simulation

This document is the single source of truth for the chao simulation. All
formulas below are implemented in `packages/simulation` using **fixed-point
integer arithmetic** so every server produces identical results for identical
inputs.

## Representation

All fractional quantities are stored as integers scaled by `FIXED_POINT_SCALE
= 1000` (three decimal digits of precision). Example: a hunger of `0.745`
is stored as `745`.

Ranges are `[0, 10000]` unless stated otherwise (i.e. `[0.0, 10.0]` in game
units). Reaching `0` means empty; reaching `MAX` means full.

## Needs decay

Every need decays per elapsed hour (scaled linearly, prorated to the exact
millisecond):

| Need    | Decay per hour (scaled) |
|---------|------------------------|
| hunger  | `HUNGER_DECAY_PER_HOUR = 125`  |
| energy  | `ENERGY_DECAY_PER_HOUR = 90`   |
| hygiene | `HYGIENE_DECAY_PER_HOUR = 70`  |

`mood` decays slowly: `MOOD_DECAY_PER_HOUR = 40`.

Notes:
- While the chao is **asleep** (see below) energy *increases* by
  `ENERGY_RECOVER_PER_HOUR = 2500` and hunger decays at only 50%.
- Needs never fall below `0` or above `MAX`.

## Mood

Mood is recomputed from needs after each tick:

```
moodShift =
  + 12 * (hunger is FULL ? 1 : hunger/THRESHOLD ratio beyond thresholds)
  ...
```

Simplified authoritative rule:

- Each need above its comfort threshold contributes `+mood` per hour;
- Each need below its critical threshold contributes `-mood` per hour.

| Need    | Comfort threshold | Critical threshold | +/- per hour |
|---------|-------------------|--------------------|--------------|
| hunger  | 7000 | 2000 | +8 / -20 |
| energy  | 4000 | 1500 | +4 / -15 |
| hygiene | 6000 | 2500 | +5 / -18 |

Mood itself is clamped to `[0, MAX]`.

## Care actions

Commands (server-validated) apply instant deltas:

| Command     | Effect                                                        |
|-------------|---------------------------------------------------------------|
| `feed`      | hunger += `FEED_HUNGER = 4000`, mood += `FEED_MOOD = 600`     |
| `sleep`     | toggles sleeping state                                        |
| `wake`      | clears sleeping state                                         |
| `clean`     | hygiene = MAX, mood += `CLEAN_MOOD = 400`                     |
| `play`      | mood += `PLAY_MOOD = 1500`, energy -= `PLAY_ENERGY = 800`, hunger -= `PLAY_HUNGER = 400` |
| `pet`       | mood += `PET_MOOD = 500`                                      |
| `train`     | see below                                                     |

Actions are rate-limited by the sim: repeating the same action within
`ACTION_COOLDOWN_MS = 10_000` is rejected with a typed event.

## Training

`train { stat, minutes }`:

- Gains are proportional to elapsed training minutes, capped at
  `TRAIN_MINUTES_PER_SESSION = 60`.
- `points += minutes * TRAIN_POINTS_PER_MINUTE` (base 10) for the stat.
- Every `POINTS_PER_LEVEL = 100` points raises level by 1; level caps at
  `MAX_LEVEL = 99`. Overflow points carry over.
- Grades rise when level reaches thresholds 25 / 50 / 75 (grade 1..5).
- Training costs energy: `energy -= minutes * TRAIN_ENERGY_PER_MINUTE (25)`,
  hunger -= `minutes * TRAIN_HUNGER_PER_MINUTE (20)`, and mood -= 2 per
  minute. If energy would hit 0, the session is cut short to the largest
  whole-minute count that keeps energy >= 0.

## Growth

Age is derived from wall-clock elapsed time since `bornAt`, in hours:

- stage `egg`     : age < 1 hour
- stage `baby`    : age < 5 hours
- stage `child`   : age < 20 hours
- stage `adult`   : age < 100 hours
- else            : stage `elder`

Growth **never regresses**: the stage is stored as `minStage` and only moves
forward, protecting against clock rollback.

## Evolution

At the moment the chao first becomes an `adult`, evolution traits are
computed from lifetime statistics:

- **alignment** = `heroPoints - darkPoints`
  - alignment >= `ALIGNMENT_THRESHOLD (500)`  → `hero` trait
  - alignment <= -`ALIGNMENT_THRESHOLD`        → `dark` trait
  - otherwise → `neutral` trait
- The dominant stat (highest current points, ties broken by stat order
  swim, fly, run, power, stamina) chooses the *type* adjective:

  | Dominant stat | Adult name suffix |
  |---------------|-------------------|
  | swim   | `Aqua`    |
  | fly    | `Sky`     |
  | run    | `Swift`   |
  | power  | `Brute`   |
  | stamina| `Tough`   |

- Flying type may only evolve once `fly >= EVOLUTION_FLY_GATE (300)` points;
  otherwise the runner-up stat is used.
- Evolution is deterministic given the stat history — the same checkpoint
  and commands always produce the same traits.

Hero/dark stat drift: feeding hero fruit (`feed { item: 'hero' }`) adds
`HERO_POINTS_PER_FEED = 5` to heroPoints; `feed { item: 'dark' }` adds to
darkPoints.

## Neglect

If any need stays below its critical threshold for longer than
`NEGLECT_GRACE_MS = 30 minutes`:

- mood drains an extra `-30/hour`,
- stat points decay `NEGLECT_STAT_DECAY_PER_HOUR = 5` from the lowest stat,
- a `neglectWarning` event is emitted once, then `neglectSick` after
  `NEGLECT_SICK_MS = 6 hours` of continued neglect.

While `sick`, training gains are halved and mood cannot exceed 3000 until
cared for (all needs above critical clears sickness).

## Offline progress

Offline time is capped:

- `OFFLINE_CAP_MS = 12 hours` of full-rate simulation,
- beyond that, decay continues at a reduced `OFFLINE_REDUCED_RATE = 0.25`
  up to a hard ceiling of `OFFLINE_HARD_CAP_MS = 48 hours`.
- Mood decays at full rate for the first capped window only; after that it
  clamps at `OFFLINE_MOOD_FLOOR = 1000` (the chao never sulks to zero from
  absence alone).

No rewards (stat points, evolution) are granted while fully offline beyond
the hard cap; elapsed time beyond `OFFLINE_HARD_CAP_MS` is simply discarded
for need simulation but still counts toward age.

## Determinism rules

1. Time enters the simulation only as `nowMs` injected per tick/command.
2. Any randomization uses a seeded deterministic PRNG (`randomSource`),
   seeded from creature + checkpoint seed, never from wall clock entropy.
3. All arithmetic is integer fixed-point; no `Math.random`, no floats.
4. Commands are applied in a strict ordered list; replaying a checkpoint
   plus the same commands yields the same state and events.

## Versioning

`SIMULATION_VERSION = 1`. Any balancing change bumps the version and
provides a migration from the prior checkpoint shape. The sim refuses to
load a checkpoint with an unknown future version.
