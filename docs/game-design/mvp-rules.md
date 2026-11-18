# Game Design — MVP Rules

All creature species, fruit, stat names, moods, and lore are **original IP**.
No Sega/Sonic terms or assets appear in production data or client bundles.

## Core loops

### Adopt
- Player may own at most `MAX_CHAO = 6` chao (MVP cap).
- Adoption costs `ADOPT_COST = 100` heartberries (starter grant: 500).
- New chao: species from the starting pool (3 original species), all stats
  `20`, stage `egg` for `EGG_HATCH_HOURS = 8` real-time.

### Feed
- Fruits consumed from inventory; each fruit grants hunger + stat points.
- Feeding cooldown per chao: `FEED_COOLDOWN_S = 10`.
- Overfeeding: if hunger ≥ 9000, feeding gives mood −200 and no hunger gain.
- Fruit effects are data-driven (`packages/config` fruits table), not hardcoded.

### Train
- Session of `minutes ∈ [1, 60]`; costs energy `30/min` and hunger `20/min`.
- Stat gain = `floor(minutes * sessionMultiplier * (1 + mood/10000))`
  where `sessionMultiplier` is 1.0 base, 1.2 if the chao's matched mood is good.
- Session cooldown `TRAIN_COOLDOWN_S = 300`; chao asleep → rejected.
- Diminishing returns: gain × `0.9^floor(stat/500)` per session.

### Rest / Sleep
- Sleeping recovers energy `+2500/h` and halves hunger decay (see
  `simulation.md`). Player may set an alarm; the server wakes the chao on tick.
- Sleeping chao cannot train, play, or compete.

### Evolve
- Evolution triggers automatically at thresholds; driven by dominant trained
  stat + care history (original three-stage lifecycle: egg → juvenile → adult,
  with adult forms selected by top stat and alignment score `care − neglect`).
- Evolution is **deterministic** from server history; the client renders the
  same result via the shared simulation package.

### Decorate
- Decorations purchased with shellshards; placed on a fixed garden grid
  (16×16 MVP). Each placed item stores `{itemId, x, y, rotation}`.
- Some decorations give small passive buffs (e.g. fountain: hygiene decay −10%),
  capped at 3 active buff items per garden.

### Visit gardens
- Visiting loads a **snapshot** (garden layout + chao summary) cached in Redis
  (`garden:{userId}:snapshot`, TTL 1 h). Visitors can leave one "wave" reaction
  per visit; rate limited to 20/day per visitor per garden.
- No live presence in MVP.

### Asynchronous competitions
- Daily scheduled events: server runs each enrolled chao through the
  deterministic simulation with its stats/mood/energy; results ranked and
  rewarded once per run.
- Entry costs 1 `competeticket` (max 3 held); enrollment closes at run time.
- Rewards: shellshards by placement; top-10 recorded on a per-season ledger.

## Currencies

| Currency | Source | Sink | Cap |
| --- | --- | --- | --- |
| Heartberries | daily care, visits | adoption, fruit | 99,999 |
| Shellshards | competitions, milestones | decorations, cosmetics | 99,999 |
| Compete tickets | daily login (1/day) | competition entry | 3 |

All currency mutations go through server-side ledger events (append-only,
`currency_ledger` table) — balances are derived and auditable.

## Progression limits

- Level cap per stat (MVP): 1000.
- One evolution event per chao per stage; maximum stage: adult.
- Daily care action cap per chao: 50 feed/train commands (anti-grinding).

## Anti-cheat summary (details in `anti-cheat.md`)

- Server is authoritative for all state; clients only submit intents.
- All commands rate-limited per user + per chao in Redis token buckets.
- Timestamps are server time; client clock never trusted.
- Currency and stat changes written as ledger events with idempotency keys.
