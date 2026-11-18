# Race and Karate authority

The server loads the user's locally extracted activity catalogs from
`CHAO_DATA_DIR/activities/races.json` and `karate.json`. A source checkout can run
without either file. The extraction preserves original names, configurations,
opponent appearances and raw skills. Browser movement, combat timing, payouts
and control rules below are adaptations; original native game routines are not
executed by this server.

## Catalog and scenery

`GET /api/activities` returns the detailed catalog and source provenance once.
Snapshots carry compact race/tier descriptors and the active player's activity.
Raw source records, all opponent appearance rows and all skill arrays are not
repeated in each snapshot.

The four exact model routes are `/api/assets/race-normal.glb`,
`race-dark.glb`, `race-hero.glb` and `karate-arena.glb`, mapped only to the
corresponding `imports/activities/` file. Native scenery uses uniform scale 0.1
and retains its original origin. Each course owns its bounds; garden bounds
must not shrink native race scenery. Collision JSON and terrain samples already
apply scale 0.1 and must not be scaled a second time.

## Native race entry and progress

- `race-enter` accepts only a catalog `courseId`. Entry requires an enabled
  course, explicitly validated geometry/gates/spawn, and a matching server-owned
  heightfield in `activities/terrain/{worldId}.json`.
- Entry starts a three-second countdown and holds horizontal movement. Progress
  begins only when the countdown ends.
- The authority advances one ordered gate at a time using actual simulated X/Z
  proximity and authoritative ground height. Client time, coordinates, winner,
  checkpoint indexes, damage and reward values have no effect.
- Completion records per-course `completions` and `bestMs`, grants the fixed
  catalog payout and adapted Run/happiness reward, and returns the player home.
- `race-leave`, returning home or the final socket disconnect cancels the race.
  A second socket for one profile does not start another race or simulation.
- Mounting a vehicle is refused during a native race; entry also requires
  dismounting. The existing prototype checkpoint circuit remains a separate
  activity and retains its older behavior.

All 62 extracted original configurations remain unavailable: genuine ordered
route/start/finish definitions and obstacle rules have not been validated.
Scenery, camera points, decorative SET objects and rasterized collision are not
proof of a faithful playable course. The synthetic validated course used by the
test suite is explicitly a test fixture.

## Adapted native-roster Karate

`karate-enter {tier}` starts a per-profile tournament using the catalog's joined
native names, appearances and raw skills. The four proposed five-opponent retail
groups retain their extraction caveat: the native tier-selection callsite has
not been independently traced. Raw skills are normalized by `value / 2500`
clamped to `[0,1]` for this adapted browser combat; this is not a claim about
native stat arithmetic.

The player starts at `(0,0.75,0.8)` and the CPU at `(0,0.75,2.7)` on the actual
raised ring. Safe adapted bounds are X ±1.35 and Z 0.4–3.1.

| Action | Intent and adapted authority rule |
| --- | --- |
| `karate-attack {move:"punch"}` | Costs 10 stamina; 180 ms windup, 400 ms recovery |
| `karate-attack {move:"kick"}` | Costs 20 stamina; 320 ms windup, 650 ms recovery |
| `attack` in Karate | Same as a punch |
| `karate-block` | Costs 6 stamina; 850 ms guard; a hit costs another 8 and deals one-quarter damage |
| `karate-dodge {direction:"left" or "right"}` | Costs 18 stamina; bounded lateral step, 450 ms invulnerability, 1.1 s cooldown |
| `karate-focus` | 900 ms focus, regenerates 32 stamina per second |
| `karate-next` | Advances only after a genuine non-final round victory |
| `karate-leave` | Cancels the bout and returns home |

Punches/kicks damage only a target within 1.5 world units when their server
windup completes. CPUs approach and telegraph attacks. Recovery and stamina
prevent instant attack spam. Normal idle/recovery regeneration is 9 per second.
Vehicle entry/mounting is refused in Karate.

Each round lasts up to 60 seconds. KO resolves it; timeouts compare remaining HP
ratios, with a tie counted as a loss. Only a final tournament victory records a
completion/best time and pays its fixed catalog reward once, plus adapted
Power/Stamina/happiness progression. A client cannot submit a result. Runtime
bouts/results are not restored after disconnect/restart; completed records are.

## Validation

Rust and retained Node tests independently exercise physical gate order,
countdown/native bounds, missing-collision rejection, duplicate connections,
mount restrictions, damage range/windup/recovery, guard/dodge/focus/stamina,
round advancement, KO/timeout, single payouts and saved records. HTTP tests
verify exact nested asset mappings, catalog loading and compact snapshots.
Tests use synthetic source data and temporary profiles.

The client tool `scripts/build-activity-terrain.js` rasterizes actual solid,
upward-facing collision triangles with barycentric interpolation and preserves
holes. It uses the highest static surface at each sample; water, overpasses,
native path/obstacle rules and full original race play validation remain work.
