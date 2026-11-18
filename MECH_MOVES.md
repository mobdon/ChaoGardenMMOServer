# Dark Chao Walker moves and authority

The owner pilots a native Dark Chao Walker model with their own Chao in the seat.
The browser sends intent; the server owns mount permission, motion, fuel, shot
cooldowns, aim, damage, and rewards. The Chao's stored stats, alignment, form, and
cosmetics remain their existing values.

## Playable core

| Intent | Current authoritative behavior |
| --- | --- |
| `mount-dark-walker` | Only an owner profile can equip the known Walker. |
| `dismount-vehicle` | Clears the mount; airborne Chao fall under ordinary gravity. |
| `dx`, `dz` | Normalized, bounded movement; facing follows movement. Mounted movement retains the Chao's existing run speed. |
| `jump: true` | Mounted initial vertical speed 3.2; unmounted jump stays 2.4. |
| Held `boost: true` | Thrust rises at 1.8 world units/second, capped at 3 units above ground. |
| Release boost | Gravity resumes and the pilot lands. Stale input also stops thrust. |
| Fuel | 100 maximum; drain 40/second during thrust, recharge 25/second only on ground with boost released. |
| `mech-fire`, or mounted `attack` | 350 ms shared cooldown, fixed 60 damage, 9-unit 3D range. |
| Cannon aim | Nearest living mission robot within a 45-degree forward cone. The server derives facing and target. |
| Garden firing | Produces the shot event and native visual effect; damages no players or remote mission enemies. |
| Robot kill and mission clear | Existing server-owned kill and one-time cooperative mission rewards apply. |

Fuel is separate from Chao stamina, is runtime state, and does not enter stored
profiles. Toggling the mount cannot refill it. Holding boost after exhaustion
leaves the pilot grounded; release the control to recharge. The prototype keeps
existing terrain and world bounds, including the prohibition on moving over
missing terrain cells. It does not yet implement free flight across gaps.

Public player snapshots add `mech`, or `null` when unmounted:

```json
{"boosting":true,"boostEnergy":74,"grounded":false}
```

Accepted shots broadcast a server event:

```json
{
  "type":"event", "kind":"mech-shot", "playerId":"...", "zone":"mission",
  "x":0, "y":0.65, "z":0.35,
  "hitX":0, "hitY":0.35, "hitZ":5,
  "targetId":"...", "hit":true
}
```

Shot and boost effects follow these events/snapshots. Requests containing forged
roles, mount state, fuel, target IDs, hit endpoints, damage, range, coordinates,
or velocity cannot replace server values. No player-versus-player damage is
implemented.

## Native motion inventory

The following names and IDs come directly from SA Tools' local
`GameConfig/PC_SA2/MDL/DWALKMTN.ini`, applied to the owner's `dwalkmtn.prs`.
There are 35 Walker motion files, one companion motion file, and 14 alias rows.
Clip availability is distinct from a complete gameplay implementation.

| ID | Source label | Gameplay coverage |
| --- | --- | --- |
| 88 | Idle 2 (`dw_wait_d`) | Idle presentation candidate |
| 89 | Stand (`dw_wait_a`) | Standing state available |
| 94 | Spring Launch (`dw_baneup0`) | Spring mechanics pending |
| 95 | Begin Falling from Spring Launch (`dw_baneup1`) | Spring mechanics pending |
| 96 | Fall (`dw_baneup2`) | Authoritative falling state available |
| 97 | Landing, Low Altitude (`dw_baneup3`) | Authoritative grounded state available |
| 98 | Landing, High Altitude (`dw_baneup4`) | Landing severity selection pending |
| 99 | Brake (`dw_stop0`) | Dedicated braking mechanics pending |
| 100 | Damage (`dw_damage0`) | Existing robot damage; dedicated reactions pending |
| 102 | Caught in Wind (`dw_rollfly`) | Wind volumes pending |
| 105 | Turn Around (`dw_stop1`) | Facing updates; turn transition tuning pending |
| 106 | Skid on Upward Slope (`dw_waitmove0`) | Native slope/skid rules pending |
| 107 | Skid on Downward Slope (`dw_waitmove1`) | Native slope/skid rules pending |
| 108 | Victory Pose (`dw_kimepose0`) | Mission completion exists; pose selection separate |
| 113 | Death (`dw_die0`) | Safe recovery exists; native death sequence pending |
| 114 | Ledge Balance (`dw_oto0`) | Native ledge balance pending |
| 115 | Jump (`dw_jump0`) | Adapted server jump |
| 116 | Begin Falling from Jump (`dw_jump2`) | Authoritative airborne state available |
| 119 | Whistle (`dw_kutibue`) | Whistle action pending |
| 120 | Dash Panel Launch (`dw_zitabata`) | Dash panel mechanics pending |
| 121 | Running Jump (`dw_runjump`) | Moving jump exists; native running-jump rules pending |
| 123 | Hang From Pulley (`dw_kutamari0`) | Pulley traversal pending |
| 124 | Look at Mystic Melody Shrine (`dw_knudai1`) | Shrine interaction pending |
| 126 | Mystic Melody (`dw_knudai0`) | Melody ability and unlock requirements pending |
| 127 | Hover (`dw_hover0`) | Adapted bounded jet thrust/hover |
| 133 | Fire Power Laser (`dw_laser`) | Separate Power Laser technique pending |
| 134 | Losing Pose (`dw_losepose0`) | Match result animation pending |
| 135 | 2P Special Attack Pose (`dw_tokupose`) | Multiplayer special-attack rules pending |
| 190 | Idle 1 (`dw_wait_c`) | Idle presentation candidate |
| 192 | Activate Dark Omochao (`dw_punch`) | Companion activation pending |
| 196 | 2P Intro Pose (`dw_mpose1`) | Match introduction pending |
| 197 | 2P Intro, Beginning Loop (`dw_mpose0`) | Match introduction pending |
| 198 | Dark Omochao Attack (`dw_msger_punch`) | Separate companion motion; attack pending |
| 199 | Walk (`dw_walk0`) | Bounded movement |
| 200 | Jog (`dw_run1`) | Movement state available; native speed thresholds pending |
| 201 | Run (`dw_maxrun`) | Movement state available; native speed thresholds pending |

Alias rows contain no separate motion file: 101, 103, 104, 109, 110, 111, 112,
117, 118, 122, 125, 191, 194, 195. Their source labels include knockback,
capture, air bubble, drowning, midair death, jump landing, shrine inspection,
default stand, and unknown states. Preserve the source references when wiring
them instead of inventing replacement animations.

## Native effects and remaining combat techniques

`DWALKMDL.ini` identifies these actual effect models:

| ID | Model | Status |
| --- | --- | --- |
| 267 | Vulcan Cannon Shot | Imported effect for adapted server cannon |
| 283 | Booster Thrust | Imported effect for adapted server boost |
| 478 | Power Laser, outer layer | Native model available; laser technique pending |
| 479 | Power Laser, inner layer | Native model available; laser technique pending |
| 480 | Power Laser thrust | Native model available; laser technique pending |
| 481 | Rocket Launcher | Native model available; launcher/lock-on logic pending |
| 482 | Rocket | Native model available; rocket targeting/travel/damage pending |

The remaining implementation work includes lock-on and target selection UI,
missile/rocket travel, Power Laser timing, Dark Omochao companion activation and
attack, special attacks, stage interactions, and native acceleration/braking,
slope, air, collision, and recovery behavior. Native animation/effect presence
does not establish the original cooldowns, damage, unlocks, or targeting rules.
The source `Characters.ini` lists Dark Chao Walker AnimationList address
`6102E0` (202 entries) and PhysicsData address `1339920`; those gameplay tables
have not yet been decoded and validated. No claim of inherited Eggman physics
or a complete native engine port is made.

Tests cover authority, forward/3D range, cooldown sharing, real damage and
cooperative rewards, fuel exhaustion, maximum height, stale-input falling,
landing/recovery, mount-toggle refills, and forged client state. They preserve
the existing unmounted movement and jump checks.
