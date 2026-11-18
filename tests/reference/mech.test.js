import test from 'node:test';
import assert from 'node:assert/strict';
import { Game } from '../../game.js';

function fixture(owner = true) {
  const game = new Game({ now: () => 1000, allowOwnerBootstrap: owner });
  const session = game.createSession({ name: 'Mech test' }); game.connect(session.player.id);
  const player = game.players.get(session.player.id);
  const action = (name, fields = {}, elapsed = 100) => { game.time += elapsed; game.action(player.id, { action: name, ...fields }); };
  return { game, player, action };
}
function mountArena() {
  const fixtureState = fixture(); const { game, player, action } = fixtureState;
  action('mount-dark-walker'); action('mission');
  Object.assign(player, { x: 0, y: 0, z: 0, groundY: 0, jumpOffset: 0, facing: 0 });
  for (const enemy of game.mission.enemies) Object.assign(enemy, { x: 20, z: 20 });
  return fixtureState;
}

test('mounted cannon uses server aim, fixed damage, shared cooldown, and real kill rewards', () => {
  const { game, player, action } = mountArena();
  const [target, behind, far, outsideCone] = game.mission.enemies;
  Object.assign(target, { x: 0, z: 5 }); Object.assign(behind, { x: 0, z: -1 });
  Object.assign(far, { x: 0, z: 10 }); Object.assign(outsideCone, { x: 5, z: 1 });
  action('mech-fire', { targetId: far.id, damage: 999999, range: 999 });
  assert.equal(target.hp, 40); assert.equal(behind.hp, 100); assert.equal(far.hp, 100); assert.equal(outsideCone.hp, 100);
  const shot = game.events.find(event => event.message.kind === 'mech-shot').message;
  assert.equal(shot.targetId, target.id); assert.equal(shot.hit, true); assert.equal(shot.hitZ, 5); assert.equal(shot.y, 0.65);
  action('mech-fire', {}, 0); action('attack', {}, 100); assert.equal(target.hp, 40);
  action('attack', {}, 250); assert.equal(target.hp, 0); assert.equal(player.rings, 25);
  assert.equal(game.events.filter(event => event.message.kind === 'mech-shot').length, 2);
});

test('cannon respects three dimensional range and garden shots cannot hurt mission robots', () => {
  const { game, player, action } = mountArena();
  Object.assign(game.mission.enemies[0], { x: 0, z: 1 }); player.y = 20;
  action('mech-fire'); assert.equal(game.mission.enemies[0].hp, 100);
  assert.equal(game.events.at(-1).message.hit, false);
  action('hub'); action('mech-fire', {}, 400);
  const shot = game.events.at(-1).message;
  assert.equal(shot.kind, 'mech-shot'); assert.equal(shot.hit, false); assert.equal(shot.targetId, null);
  assert.equal(game.mission.enemies[0].hp, 100);
});

test('cannon mission clear pays each member once using actual destroyed robots', () => {
  const { game, player, action } = mountArena();
  const guest = game.createSession({ name: 'Coop guest' }); game.connect(guest.player.id);
  game.time += 100; game.action(guest.player.id, { action: 'mission' });
  game.mission.enemies.forEach((enemy, index) => Object.assign(enemy, { x: 0, z: 3 + index * 0.1 }));
  for (let shot = 0; shot < 10; shot++) action('mech-fire', {}, 400);
  assert.equal(game.mission.completed, true); assert.equal(player.rings, 95);
  assert.equal(game.players.get(guest.player.id).rings, 70);
  action('mech-fire', {}, 400); assert.equal(player.rings, 95);
});

test('boost is bounded by fuel, height and landing and cannot be forged into infinite hover', () => {
  const { game, player, action } = fixture(); action('mount-dark-walker');
  const stats = structuredClone(player.stats); let highest = 0; let boostTicks = 0;
  for (let tick = 0; tick < 150; tick++) {
    game.setInput(player.id, { boost: true, boostEnergy: 999999, y: 9999, vy: 9999, mech: { boosting: true } });
    game.tick(0.05); highest = Math.max(highest, player.jumpOffset); if (player.boosting) boostTicks++;
    assert.ok(player.boostEnergy >= 0 && player.boostEnergy <= 100);
  }
  assert.ok(highest > 2.9 && highest <= 3); assert.ok(boostTicks <= 50);
  assert.equal(player.boostEnergy, 0); assert.equal(player.jumpOffset, 0); assert.equal(player.boosting, false);
  assert.equal(game.publicPlayer(player).mech.grounded, true); assert.deepEqual(player.stats, stats);
  game.setInput(player.id, { boost: false });
  for (let tick = 0; tick < 40; tick++) game.tick(0.05);
  assert.equal(player.boostEnergy, 50);
});

test('boost stops on stale input, consumes fuel in air, and mount toggles cannot refill it', () => {
  const { game, player, action } = fixture(); action('mount-dark-walker');
  for (let tick = 0; tick < 10; tick++) { game.setInput(player.id, { boost: true }); game.tick(0.05); }
  assert.equal(player.boostEnergy, 80); assert.ok(player.jumpOffset > 0);
  action('dismount-vehicle'); action('mount-dark-walker'); assert.equal(player.boostEnergy, 80);
  game.setInput(player.id, { boost: false }); for (let tick = 0; tick < 3; tick++) game.tick(0.05);
  assert.equal(player.boostEnergy, 80); assert.ok(player.jumpOffset > 0);
  game.setInput(player.id, { boost: true }); for (let tick = 0; tick < 30; tick++) game.tick(0.05);
  assert.equal(player.boosting, false); assert.equal(player.jumpOffset, 0);
  assert.equal(game.exportProfiles()[0].boostEnergy, undefined);
});

test('unmounted and regular players cannot forge cannon, mount, thrust or booster fuel', () => {
  for (const owner of [true, false]) {
    const { game, player, action } = fixture(owner);
    action('mech-fire', { vehicle: 'dark-chao-walker', admin: true, damage: 999999 });
    game.setInput(player.id, { boost: true, vehicle: 'dark-chao-walker', admin: true, boostEnergy: 999999 }); game.tick(0.05);
    assert.equal(player.jumpOffset, 0); assert.equal(player.boosting, false); assert.equal(game.publicPlayer(player).mech, null);
    assert.equal(game.events.some(event => event.message.kind === 'mech-shot'), false);
  }
});
