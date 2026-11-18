# Native movement integration handoff

Updated November 18, 2026. This records the native movement chat's presentation-only proposal and the integration boundaries. It is not acceptance of an unseen patch or permission to replace/restart the live runtime. Coordination is through the repositories.

Client paths in this handoff are relative to `ChaoGardenMMOClient`; the server coordination copy is a reference for the separate server repository.

## Source and shared-file ownership

- Current client baseline is `b42f165`, containing the implementation published at `279a7b7`: `src/chao-locomotion.js`, its `src/scene.js` hooks, pace controls, native Run119 conversion/verifiers and focused tests. At this handoff the integration checkout is clean and has no pending `scene.js` edits.
- The reported `codex/native-chao-movement` worktree may prepare the small isolated patch. Extend the existing helper rather than introduce a parallel implementation. Preserve existing helper exports/call sites or migrate their tests and consumers in the same reviewable change.
- Proposed new helper API: `attachNativeLocomotion`, `createNativeMotionState`, `updateNativeLocomotion`. Proposed scene changes: existing import/bind-pose attachment, retained authoritative observation fields, replacement of only the per-player native gait block, and read-only DOM diagnostics. Provide the actual diff and proof before integration.
- Keep input/camera/pacing, server physics, inventory, gear ownership, rewards, saves and networking unchanged. The Walker/pilot path remains separate: mounted Chao uses native Idle and the existing Walker animation path continues unchanged.

## Verified native sources

These are original animation-state table records in the local hashed activity manifest, not three independent model/rig replacements:

| State | Source | Relevant original fields |
| --- | --- | --- |
| 119 | `alm_run` | Existing native Run at base `PlaySpeed=0.21` |
| 132 | `alm_jump_up`, `animation_002B0300` | Canonical132; `EndFrame=6`, `PlaySpeed=0.2`, `NextAnimation=133` |
| 133 | Same JumpUp source | `StartFrame=6`, `EndFrame=7`, `PlaySpeed=0.2` |
| 134 | Same JumpUp source, reverse phase | `StartFrame=6`, `PlaySpeed=-0.2` |
| 135 | `alm_jump_down`, `animation_002AFC88` | Canonical135; `EndFrame=4`, `PlaySpeed=0.2`, `NextAnimation=136` |
| 136 | Same JumpDown source | `StartFrame=4`, `EndFrame=5` |
| 137 | Same JumpDown source, reverse phase | Canonical135; `StartFrame=4`, `PlaySpeed=-0.2` |

The proposed reverse137 visual landing recovery uses a genuine reverse source phase. Calling that phase the retail game's landing transition requires tracing the original state-selection code; otherwise label the landing mapping as a presentation adaptation. Preserve source frame ranges/rates, prove reverse playback terminates correctly and distinguish holding poses from restarting the complete source loop.

The currently compacted `chao-locomotion.glb` contains **only** `alm_run`. JumpUp/JumpDown require an explicit bounded extraction/compaction extension with original source hashes and exact-byte verification. Update the generator, native verifier and runtime proof together: current assertions intentionally expect one animation/Run119 and cannot prove newly added clips. Keep converted data ignored/local.

The exact locomotion asset route works in the current copied local runtime. Its four-file server source patch remains separately unpublished while the Vibe server build is blocked. Do not claim a fresh server checkout can serve the expanded asset or repair unrelated Rust failures inside this presentation patch.

## Authoritative observation boundary

The real Rust public player snapshot supplies `jumpOffset`, `groundY`, position and server time. Ordinary Chao grounding is currently derived from `jumpOffset`; a distinct ordinary-player `grounded` wire field is not present. The mech has its own `mech.grounded` field.

`observeSkateMotion` currently retains horizontal speed, derived grounding, timestamps/zone and the existing swimming heuristic. It does **not** retain `jumpOffset`. Explicitly retain/validate the real offset in the observation path or pass the snapshot field through the item; a helper cannot consume a field that the scene discarded.

Derive ascent/descent only from consecutive valid authoritative offset/time samples. Do not integrate browser gravity or write positions to manufacture a fall/landing. Treat first samples, missing/nonfinite values, reversed/equal time, zone changes, teleports, stale snapshots and reconnects as resets. Reset the previous airborne/landing state rather than manufacturing a contact transition from unrelated samples. Retain the current stale-motion cutoff and teleport plausibility guard.

Deduplicate observation updates by `serverTime`: render frames can reuse one snapshot many times. Preserve direction through an apex epsilon, and trigger visual landing once per continuous valid airborne-to-grounded edge. Mounting/swimming also clears that history. Authoritative mounted state takes priority even while the Walker asset is loading.

The current swimming observation is a limited Neutral garden zone/radius heuristic. Do not present it as general authoritative water detection for all native terrains; broader water mechanics need a separate Rust contract. Until then retain the existing guarded fallback rather than infer new movement authority.

The existing motion observer and scene fallback currently use different swimming conditions. Define one effective presentation swimming flag and pass it consistently to skating and native locomotion; record its current heuristic limitation.

## Rig and rendering constraints

- Capture target bind transforms before asynchronous attachment. Ignore load completions after disposal/appearance replacement; dispose all new groups with the owning instance.
- Preserve original Chao parents, scales, mesh, weights and proportions across Neutral/Hero/Dark Chaos. Use the existing bind-relative native retargeting approach; never copy another character's limb lengths or world translations.
- Preserve body opacity, eye shells/pupils, original flame alpha, native emerald transparency and depth-occluded glow. Gear must follow actual animated sockets. Keep normal native walk/run and Shadow threshold/hysteresis/cadence behavior.
- Bound airborne/landing blends and recover to the correct existing gait/Idle. Stale, unavailable-asset, swimming and mounted paths require explicit fallback tests; presentation must not keep a stale landing/skate pose alive.
- Keep reverse137 playback once-only: no double reversal, unintended loop or generic walk/run cadence assignment overwriting its negative source rate. Preserve `imported.run`, `imported.current` and the existing animation-entry disposal ownership.

## Review and proof

1. Supply the small `scene.js` diff plus helper/tool/test changes and original-versus-adapted motion notes. Preserve pace controls and all earlier exports or migrate consumers deliberately.
2. Run meaningful transition tests: idle/walk/run, ascent/apex/descent/contact, interrupted/repeated jumps, freshness/teleport/zone reset, missing asset/disposal and mounted/swimming fallbacks. Verify original transforms and attached props on all three native Chaos forms.
3. Drive the actual `createGarden` path in the proof fixture. Share the production code path; do not substitute a separate demonstration renderer or manipulate hidden live app state.
4. Exercise the actual supported Rust snapshot protocol using disposable synthetic data and isolated ports. A read-only snapshot subscription does not imply permission to use the owner's credential, write its profile or restart its preview. New Vibe Rust source is currently broken; select an explicit known-tested isolated runtime/baseline for the fixture and record it.
5. Return changed paths, exact source/runtime hashes, test results, captured evidence and unresolved contact/terrain limitations. No automatic merge, live data migration or runtime replacement follows from a proof fixture.
