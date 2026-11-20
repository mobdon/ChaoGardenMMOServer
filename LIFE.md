# Authoritative Chao life contract

This is the server-side handoff for Chao stat growth, age, the evolution and
end-of-life cocoons, reincarnation, breeding, eggs, needs, mood, illness,
personality and social knowledge. It covers gap-report items RPG-04 (stat
curves), LIFE-01 (breeding, genetics and life cycle) and LIFE-02 (needs,
emotions, personality and social behavior). The code is `src/life.rs`.

## Evidence boundary

The state layout and names follow the reverse-engineered `CHAO_PARAM_GC`,
`AL_EMOTION`, `AL_KNOWLEDGE` and `AL_GENE` structures in X-Hax
`SA2Structs.h` (client `docs/MECHANICS.md` source E-SDK). Those names tell us
where the original game keeps a value. They do not tell us its formula.

Every rate, threshold, range scale and inheritance rule below is **MMO tuning**
and lives in `life::tuning`. None of it is claimed as a verified native rule.
Two values follow widely repeated community notes about the original and are
still unverified: happiness 50 decides a pink (reborn) or grey cocoon, and
Chaos Chao never age out. When a native constant is traced from the retail
executable, replace the tuning value and record the trace here.

| Native field | Server field | Range | Status |
| --- | --- | --- | --- |
| `Exp[8]` | `life.exp` (seven stats) | 0..99 | Layout native, curve tuning |
| `Abl[8]` / `Lev[8]` / `Skill[8]` | `stats.*.grade/level/points` | E..S = 0..5, 0..99, 0..9999 | Existing fields |
| `age`, `old` | `life.age`, `life.adultAge` | rollovers | Clock length tuning |
| `life`, `LifeMax` | `life.life`, `life.lifeMax` | rollovers | Tuning |
| `nbSucceed` | `life.reincarnations` | count | Native meaning |
| `emotion.Mood[8]` | `mood` | 0..100 | Range tuning |
| `emotion.State[11]` | `needs` | 0..10000 | Range tuning |
| `emotion.Personality[13]` | `personality` | -100..100 | Range tuning |
| `emotion.Taste/Tv/Music` | `taste/tv/music` | taste = fruit index 0..23 | Tuning |
| `emotion.IllState[6]` | `illness` | 0..100 | Tuning |
| `knowledge.chao[20]` | `knowledge` (20 entries) | like 0..255, fear, meet | Slot count native |
| `gene` | `gene` (alleles, parents, grandparents) | — | Inheritance tuning |

`needs.hunger` and `needs.energy` are projections of the existing
`Player.hunger` (satiety 0..100) and `Player.energy`. Hunger 10000 means
starving (satiety 0). There is one source of truth for each.

## MMO policies

- **Aging uses connected time only.** A Chao ages, gets hungry and can fall ill
  only while its owner is connected. Offline profiles and Watch residents are
  frozen, so nobody returns to a dead or sick Chao.
- **Clock.** One rollover is 30 connected minutes. A child enters the evolve
  cocoon at 4 rollovers (2 hours) and is an adult 60 seconds later. The
  evolution thread's body module picks the adult type at that moment.
- **Lifespan.** `lifeMax` is 200 rollovers plus the mean of the two LifeTime
  alleles. An adult loses one per rollover, two while ill or with happiness
  under 20. Chaos form never loses life.
- **End of life.** At zero life with happiness 50 or more the Chao enters a pink
  cocoon and is reborn: same gene, personality and friends, levels reset to 0,
  points kept at one tenth, needs refreshed. Owner perfect stats never change.
  Below 50 it enters a grey cocoon.
- **Death is behind a server switch** (mob's decision, November 20, 2026).
  `CHAO_LIFE_DEATH` unset or `disabled` (default): the grey cocoon waits until
  happiness reaches 50 and then reborn. `enabled`: the grey cocoon ends the Chao
  after 60 seconds. Any other value fails startup.
- **Successor eggs.** In a grey cocoon, or after death, the owner may hatch an
  egg from their nest as the next generation. After death, hatching without an
  egg makes a fresh one. A hatched egg resets levels and points to 0, sets each
  grade to the higher of its two alleles, expresses its genes and clears
  friendships. Name, rings, cosmetics and records stay with the profile.
- **Breeding is mutual.** Both Chao must be connected adults in the same hub or
  garden within 4 units, with happiness at least 50 and mate desire at least
  5000, outside any other activity. The first `breed` request is an offer that
  lasts 60 seconds; the partner's matching `breed` accepts. The offerer is the
  mother and keeps the egg (nest of 3). Both parents' mate desire resets and
  they wait 30 minutes. Each child allele is a random one of each parent's two.
- **Stat growth.** `life::grant_stat_progress(stat, exp)` adds Exp; every 100
  Exp is a level that adds `[12,15,18,21,24,27][grade] + 0..3 + level/2`
  points, capped at 9999. Negative amounts remove points only. The existing
  adapted rewards that call `Player::gain` are unchanged.
- **Needs and mood.** Sleepiness, tiredness (faster while moving), boredom,
  loneliness and adult mate desire rise; nourish decays; satiety falls once a
  minute (full to empty in two hours). Stress rises while starving, exhausted,
  lonely or ill. Condition combines stress, hunger and nourish. Joy follows
  happiness and condition, relax follows low stress, and the other moods fade.
  Personality scales rates: curiosity (boredom), solitude (loneliness),
  vitality (tiredness), glutton (hunger), regain (recovery).
- **Illness.** Once a connected minute, a Chao in poor condition (under 3000)
  may catch one of cough, cold, rash, runny nose, hiccups or stomach ache. Good
  condition (over 6000) cures it gradually. Illness costs happiness and life.
  `life::is_ill` is ready for a future Doctor service.

## Wire contract

The authenticated WebSocket accepts a strict `life` envelope, shaped like
`care`:

```json
{
  "type": "life",
  "requestId": "00000000-0000-4000-8000-000000000002",
  "expectedRevision": 0,
  "intent": { "kind": "breed", "partnerId": "<player id>" }
}
```

| Intent | Fields | Meaning |
| --- | --- | --- |
| `greet` | `partnerId` | Meet a nearby Chao. Both sides gain like and meet count and lose loneliness; 10 s cooldown per pair. |
| `breed` | `partnerId` | Offer, or accept the partner's open offer. |
| `hatch` | optional `eggId` | Hatch a nest egg as the next generation (grey cocoon or dead only). |
| `release-egg` | `eggId` | Remove one egg from the nest. |

Unknown fields are rejected. The client never sends needs, stats, genes,
alleles, ages or clocks. The response is
`{type:"event", kind:"life-result", requestId, accepted, replayed, code,
revision, rulesVersion, effects}`. Codes include `offer-sent`, `egg-laid`,
`greeted`, `hatched`, `released`, `invalid-request`, `stale-revision`,
`request-id-conflict`, `activity-conflict`, `partner-busy`, `stage-conflict`,
`out-of-range`, `not-ready`, `partner-not-ready`, `partner-nest-full`,
`egg-unavailable`, `cooldown` and `save-failed`.

A retry of the same request returns the committed result with `replayed:true`.
The last 256 receipts are kept; an older retry fails `stale-revision` rather
than applying twice. Breeding and greeting can change the partner too, so the
service rolls back both profiles if the save fails.

Private events: `life-breed-offer` (to the invited partner), `life-egg-laid`
(to the mother), `life-cocoon`, `life-evolved`, `life-reincarnated`,
`life-hatched` and `life-ended`.

## Snapshots

The owner-only top-level `snapshot.life` holds `stage`, `cocoon`
(`kind`, `remainingSeconds`), `age`, `adultAge`, `life`, `lifeMax`,
`immortal`, `reincarnations`, `generation`, `deathPolicy`, `exp`, `mood`,
`needs`, `personality`, `taste`, `tv`, `music`, `illness`, `knowledge`,
`gene` (parents, grandparents, grade alleles, LifeTime), `nest`, `breed`
(`offer`, `readyAt`, `ready`), `eligibility` (`canSocialize`, `canHatch`) and
`provenance`. Every public `players[]` entry has a small `life` summary:
`stage`, `cocoon`, `age`, `reincarnations`, `generation`, `joy`, `total` and
`ill`. Nest, genes and knowledge are never public.

While in a cocoon or dead the Chao cannot move; the server clears its input.

## Module boundaries

Agreed with the parallel server threads on November 20, 2026:

- **Evolution/appearance** (`src/body.rs`, profile key `body`) owns
  `AL_BODY_INFO`, the adult type, the expressed look and alignment.
  `life::apply_events` is the single hook point: `StatProgress` →
  `body::on_stat_progress`, `Evolved` → `body::evolve`, `Reincarnated` →
  `body::reincarnate`, `Hatched { look }` → `body::express`. Whichever PR merges
  second wires those calls.
- **Animals, fruit and trees** own items and catalogs and apply changes through
  `life::grant_stat_progress` and `life::feed(Food { fruit, nourish, joy })`.
- **Chao AI** reads `life::needs_view` and writes only through
  `life::adjust_need` and `life::adjust_mood`.

## Save compatibility

`life` is a strict, versioned profile object replaced as a whole on save.
Profiles without it migrate as a seeded first-generation Chao: hero, dark and
Chaos forms start as adults, everything else as a child. Malformed or newer
life data fails loading instead of resetting. No live save is migrated by this
change and the preview is not restarted.

## Validation

- `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked --all-targets`: includes 14 life unit tests and the
  `life_protocol` WebSocket test (offer, failed save rolling back both parents,
  durable egg, replay, restart replay).
