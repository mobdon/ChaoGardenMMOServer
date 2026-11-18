import test from 'node:test';
import assert from 'node:assert/strict';
import { Game } from '../../game.js';

function fixture(options = {}) {
  const game = new Game({ now: () => 1000, ...options });
  const session = game.createSession({ name: 'Resident' });
  game.connect(session.player.id);
  return { game, session, player: game.players.get(session.player.id) };
}
function advance(game, milliseconds) {
  for (let elapsed = 0; elapsed < milliseconds; elapsed += 50) game.tick(.05);
}
function act(game, player, action, fields = {}) {
  advance(game, 100);
  game.action(player.id, { action, ...fields });
}

test('disconnected reference resident stays visible and moves/rests without passive rewards', () => {
  const { game, player } = fixture();
  act(game, player, 'watch-start');
  const before = structuredClone({ stats: player.stats, rings: player.rings, happiness: player.happiness, energy: player.energy });
  game.rings[0].x = player.x; game.rings[0].z = player.z;
  game.disconnect(player.id);
  const states = new Set(); let moved = false;
  for (let tick = 0; tick < 500; tick++) {
    game.tick(.05); states.add(player.watch.activity);
    moved ||= Math.hypot(player.x, player.z - 6) > .1;
    const visible = game.snapshot('observer').players;
    assert.equal(visible.length, 1); assert.equal(visible[0].connected, false);
    assert.equal(visible[0].watch.ownerOnline, false);
  }
  assert.ok(moved); assert.ok(states.has('idle') && states.has('wander') && states.has('rest'));
  assert.deepEqual({ stats: player.stats, rings: player.rings, happiness: player.happiness, energy: player.energy }, before);
  assert.equal(game.rings[0].availableAt, 0);
  game.connect(player.id); game.connect(player.id);
  assert.equal(game.snapshot(player.id).players.length, 1);
});

test('reference residency persists opt-in and location only with fresh restart identity', () => {
  const { game, player, session } = fixture();
  act(game, player, 'watch-start'); advance(game, 3500);
  const previousId = player.watch.id;
  const profiles = game.exportProfiles();
  assert.deepEqual(Object.keys(profiles[0].resident).sort(), ['enabled', 'x', 'z', 'zone']);
  const restored = new Game({ profiles, now: () => 9999999999 });
  const resident = restored.playerForToken(session.token);
  assert.equal(resident.zone, 'garden'); assert.notEqual(resident.watch.id, previousId);
  assert.equal(resident.watch.activity, 'idle'); assert.equal(resident.rings, 20);
  restored.connect(resident.id);
  act(restored, resident, 'watch-stop', { sessionId: previousId });
  assert.ok(resident.watch);
  act(restored, resident, 'watch-stop', { sessionId: resident.watch.id });
  assert.equal(resident.watch, null);
  assert.equal(new Game({ profiles: restored.exportProfiles() }).players.get(resident.id).watch, null);
});

test('reference watch intents isolate profiles, reject forged fields and enforce care cooldown', () => {
  const { game, player } = fixture({ allowOwnerBootstrap: true });
  act(game, player, 'mount-dark-walker');
  act(game, player, 'watch-start', { rings: 999999 }); assert.equal(player.watch, null);
  act(game, player, 'watch-start'); assert.equal(player.vehicle, null);
  const run = player.watch.id; const stats = structuredClone(player.stats);
  const other = game.players.get(game.createSession({ name: 'Other' }).player.id); game.connect(other.id);
  act(game, other, 'watch-start');
  act(game, other, 'watch-stop', { sessionId: run }); assert.ok(other.watch);
  act(game, player, 'watch-care', { sessionId: run, kind: 'pet', happiness: 100 });
  assert.equal(player.happiness, 65);
  for (let i = 0; i < 5; i++) act(game, player, 'watch-care', { sessionId: run, kind: 'pet' });
  assert.equal(player.happiness, 67); assert.equal(player.rings, 20); assert.deepEqual(player.stats, stats);
  advance(game, 1500); act(game, player, 'watch-care', { sessionId: run, kind: 'rest' });
  assert.equal(player.watch.activity, 'rest');
  const x = player.x; game.setInput(player.id, { dx: 1, dz: 1, jump: true }); advance(game, 200);
  assert.equal(player.x, x);
  for (const action of ['mission', 'race', 'feed', 'train-run', 'mount-dark-walker']) act(game, player, action);
  assert.equal(player.zone, 'garden'); assert.equal(player.vehicle, null); assert.equal(player.race, null);
  act(game, player, 'watch-stop', { sessionId: run });
  for (let i = 0; i < 10; i++) { game.setInput(player.id, { dx: 1, dz: 0 }); game.tick(.05); }
  assert.ok(player.x > x + .4);
});
