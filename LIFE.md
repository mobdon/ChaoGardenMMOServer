# Authoritative Chao life contract

Server-side Chao stat growth, aging, cocoons, rebirth, breeding, eggs, needs,
moods, illness, personality and owner knowledge. Code: `src/life.rs`. Gap-report
items RPG-04, LIFE-01 and LIFE-02.

## Source

Every game rule here is ported from the GameCube SA2B (GSNE8P) `ChaoMain.rel`,
SHA-1 `e7e24b8b6f4305d95531c9d898d566ead79ddc8d`, read-only from the owner's own disc
on November 20, 2026. That REL is byte-identical to the one the roeming/SA2B
decomp is built from, so its symbol names apply. `al_gene.c` in that decomp is
marked Matching. The full report with addresses stays local on the owner's PC
(`.local/research/chao-life-native-2026-11-20.md`). No game data, tables or
assets are in this repository; only the scalar `ChaoGlobal` constants that the
formulas need, each commented with its source in `life::native`.

The standing rule is native porting only, with no assumed or made-up game
data. Anything without a native source is either an explicit MMO policy below
or left out.

## Native rules

| System | Rule | Source |
| --- | --- | --- |
| Clock | Each Chao gets one `AL_CalcParameter` call every 96 frames (about 1.60 s at 59.94 Hz). Life tick every 121 calls (about 193.8 s), moods every 8 (12.8 s), needs every 11 (17.6 s; 10 while sleeping, sitting or walking), illness every 81 (129.7 s), knowledge every 11. | `AL_ParameterControl` 0x20CCC, `AL_CalcParameter` 0x213CC; frame rate: main 0x800D3AD0, `fn_80117DF8` N = 1 (one tick per NTSC field) |
| Stat level-up | `Exp += gain` (minimum 0). At most one level per call: Exp > 99 and Lev < 99 gives Exp −100, Lev +1, points `+= 11 + 3×grade + 0..4`, capped at 4000. At Lev 99, Exp is set to 99. | `AL_ParameterGrow` 0x21148, `fn_60_2131C` |
| Fruit | Four bites. Bites 1–3 add `trunc(v/4)`, bite 4 adds the remainder, so a whole fruit gives exactly `v`. Each bite: hunger −2400, life +3, and LifeMax +5 for a child. | `ALO_FruitHeldC`, `fn_60_2BD7C` |
| Lifespan | `LifeMax = 3000 + 100 × max(LifeTime alleles)`. Each life tick an adult (not Chaos) gets `old += 1` and `LifeMax −= (old + 100) / 15`, and `life` clamps to LifeMax. Children and Chaos Chao do not age. | `AL_GeneAnalyzeCommon` 0x1002C, `AL_CalcParameter` |
| Evolution cocoon | Starts when the body module reports growth ≥ 1.0 and the Chao is grounded. Type decision at 2000 frames (body::evolve, with like and nbSucceed for the Chaos check), adult at 3900 frames. | `fn_60_1531C`, `ALBHV_Mayu` |
| End of life | An adult at life 0 on the ground cocoons. Pink when `like > 50` and stress < 700, checked at the start and again at frame 2000; the outcome comes at frame 3830. | `fn_60_15060`, `fn_60_3ADE0`, `ALBHV_Dead` |
| Rebirth | nbSucceed +1; skills `min(points, 5000) / 10`, Lev 1, Exp 0; grades, like and knowledge kept; each personality gene gets one random copy overwritten with `P/40 + 2`, then personality is re-derived; `LifeMax = 3000 + 100 × max(LifeTime) + oldLifeMax/10`; age 0. | `CreateEggSucceed` 0x3B864, `AL_SucceedGeneParam` 0x111C0 |
| Needs (0..10000) | Per needs tick, with `f(b, ±P) = b(300 ± P)/300`: sleepy `+f(90,+SleepyHead) × (1 + 0.00006 × tire)`; hunger `+f(70,+Glutton)`; breed `+f(12,+Vitality)` (adults, not Chaos); tedious `+f(60,+Curiosity) + f(30,+Aggressive) + f(30,−Calm)`, then 10% reset to 6000 above 9999; lonely `+f(60,−Solitude)`; tire `+f(10,−Vitality)`; nourishment −36; condition +80 above 9000 nourishment, −120 below 1000, −60 when stress > 8000, −300 when pain > 100; thirsty +150. | `AL_CalcParameter` |
| Moods (0..200) | Per mood tick: pleasure −4; anger `−f(4,−Aggr) − f(4,+Kind) − f(4,+Calm)`; sorrow and fear `−f(4,+Aggr) − f(4,+Calm)`; surprise `−f(4,+Calm)`; pain `−f(4,+Regain)`; relax `−f(4,+Curiosity) − f(4,+Fickle)`; slot 7 untouched. | `AL_CalcParameter` |
| Illness | Each life tick, chance 1/6000 to catch slot `trunc(r × 5.9999)` (never slot 3, runny nose) if that slot is 0; it is set to −100. Each illness tick, every slot moves 1 toward 0, so about 3.6 h ill. Cure sets the first ill slot to +100 (immunity). Illness only changes animations. | life tick 0x2153C, `fn_60_160D8` |
| Breeding | A mate call needs breed ≥ 10000 and fewer than 8 Chao in the garden; otherwise breed resets to 0. The answer needs breed > 5000. The caller is the mother. One random allele from each parent, no mutation. The mother's breed goes to 0, and the father's when he is on the mother's memory card (natively both parents'; MMO policy: another player's Chao keeps his, so he can breed again). | `fn_60_15214`, `fn_60_150FC`, `fn_60_40130`, `AL_BlendGene` |
| Expression | Grade: the higher allele with chance 0.7. Personality `(allele − 2) × 40`. Taste, TV, music and monotone: a random allele. Color and jewel: the non-normal allele dominates. Shiny if either allele. | `AL_GeneAnalyzeCommon` |
| New eggs | Grades from one of three schemes (shuffle of 0..4; all 1..3; 14 random increments), LifeTime 0..9, personality alleles 0..4, taste/TV/music 0..5. | `AL_GeneCreate` 0x1218C |
| Happiness and knowledge | `like` −100..100. Petting, holding, a fruit handed over and an animal each give +1. A positive gain lowers fear by `f(10,+Aggr) + f(10,+Calm)` and counts a meeting (`distance = distance/3 × 2`). Distance +16 per knowledge tick, max 50000. | `AL_ParameterAddUserLike` 0x20D7C, `AL_KW_*` |

### Native bugs (fixed, decided November 20, 2026)

These are fixed by default (`nativeBugs` off); the native behaviour is kept
behind the policy flag and covered by a test. With the fix, hatch and rebirth
use the intended `AL_EmotionStateInit` values (sleepy `rand×5000`, hunger
`3000+rand×6000`, breed `rand×999.9`, tedious `rand×5000`, tire `rand×8000`,
nourishment and condition 10000, the rest 0), rebirth resets moods, and taste,
TV and music are re-derived from the genes. The 8-Chao garden limit is native
and stays on.


- `AL_EmotionStateInit` writes the wrong State slots, so after hatch or rebirth
  only nourishment (`rand × 5000`), condition (0) and thirsty
  (`3000 + rand × 6000`) are set.
- Rebirth keeps moods, because the reset loop writes past `Mood[7]` into the
  high byte of sleepy. It also sets taste to 16 and TV and music to 0.
- The shuffle scheme of `AL_GeneCreate` reads past its 5-entry array for the
  last skills; those grades are undefined natively and stay 0 here (no native
  value exists to copy).

## MMO policies

- **Owner = the one character.** The native knowledge keeps one record per
  playable character (Sonic, Shadow and so on). Here the Chao's owner is its only
  character, so there is one owner record.
- **Frozen while offline** (decided November 20, 2026). Needs, moods,
  illness and aging only run while the owner is connected; Watch residents and
  offline profiles are frozen. `residentNeeds` stays off.
- **Death switch.** `CHAO_LIFE_DEATH` unset or `disabled` (default): a grey
  cocoon keeps checking and turns pink once `like > 50` and stress < 700.
  `enabled`: a grey cocoon ends the Chao, like native `KillChao`. Any other value
  fails startup.
- **Consent and nest.** Breeding is a mate call (`breed` offer, 60 s) answered by
  the partner's matching `breed`, both in the same garden within 4 units. The
  egg goes to the caller's nest (3 eggs). A grey-cocoon or dead Chao may hatch a
  nest egg as the next generation (generation +1; levels and points 0, grades
  from the egg, like and knowledge reset).

## Left out (no native source)

- Chao-to-Chao knowledge: nothing in ChaoMain writes those slots. The earlier
  `greet` intent is removed.
- What raises stress: nothing in ChaoMain adds to it.
- The disliked-fruit check (taste to fruit mapping is not traced) and the meaning
  of the two header numbers in each Grow table row.
- The cadence of walk, swim, eat and rest need changes: the behaviour module
  applies them through `adjust_need` on `take_need_update`.
- X grade.

## Wire contract

Strict `life` WebSocket envelope:

```json
{"type":"life","requestId":"<uuid>","expectedRevision":0,
 "intent":{"kind":"breed","partnerId":"<player id>"}}
```

| Intent | Fields | Meaning |
| --- | --- | --- |
| `breed` | `partnerId` | Mate call, or the answer to the partner's open call. A Chao on the caller's own memory card needs no answer: the egg is laid at once in the caller's nest. |
| `hatch` | `eggId` | Hatch a nest egg (grey cocoon after its check, or dead). |
| `release-egg` | `eggId` | Remove an egg from the nest. |

Responses are `{type:"event", kind:"life-result", requestId, accepted,
replayed, code, revision, rulesVersion, effects}`. Codes: `offer-sent`,
`garden-full`, `egg-laid`, `hatched`, `released`, `invalid-request`,
`stale-revision`, `request-id-conflict`, `activity-conflict`, `partner-busy`,
`stage-conflict`, `out-of-range`, `not-ready`, `invalid-partner`,
`partner-unavailable`, `partner-nest-full`, `nest-full`, `egg-unavailable`, `save-failed`.
Retries replay the stored result. The last 256 receipts are kept; an older
retry fails `stale-revision`. A save failure rolls back both partners.

Private events: `life-breed-offer`, `life-egg-laid`, `life-evolved`,
`life-reincarnated`, `life-hatched`, `life-ended`.

Owner-only `snapshot.life` has the stage, cocoon frames, age, life, like,
policy, Exp, moods, needs, personality, taste/TV/music, illness, owner
knowledge, gene summary, nest and eligibility. Public `players[].life` has only
the stage, cocoon kind, age, reincarnations, generation and an ill flag.

## Module API

- **Behaviour (AI):** `needs_view`, `adjust_need`, `adjust_mood`,
  `set_need_cadence_busy`, `take_need_update`, `can_wake`.
- **Animals and fruit:** `eat_fruit_bite(kind, row, bite)`, `capture_animal(row)`,
  `fruit_handed_over`, `pet`, `grant_stat_progress`. The Grow rows stay in the
  local native catalog.
- **Body (evolution):** `LifeTick` → growth, `start_evolution_cocoon` when
  `body::cocoon_due` and grounded, `EvolutionDecision` → `body::evolve`,
  `Reincarnated` → `body::reincarnate`, `Hatched` → `body::express`,
  `FruitBite { kind, child, last }` → `body::apply_fruit`. Native stat
  level-ups do not move the body. The like attribute shift (APos ± attr) depends
  on the native player character's hero/dark side, which has no MMO equivalent,
  so it is unresolved and not applied.
- **Kindergarten doctor:** `cure`.

## Save compatibility

`life` is a strict, versioned (v2) profile object replaced as a whole on save.
Profiles without it migrate as a new first-generation egg's genes and emotion
that keep the profile's grades, levels and points; hero, dark and Chaos forms
start as adults. Malformed or other-version life data fails loading. No live
save is migrated by this change and the preview is not restarted.

## Validation

`cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and
`cargo test --locked --all-targets` (19 life unit tests plus the
`life_protocol` WebSocket test).
