import test from 'node:test';
import assert from 'node:assert/strict';
import { Game } from '../../game.js';

function fixture(owner) {
  const game = new Game({ now: () => 1000, allowOwnerBootstrap: owner });
  const session = game.createSession({ name: 'Gear test', admin: true, gear: { footwear: 'shadow-air-shoes', hands: 'knuckles-hammer-gloves' } });
  game.connect(session.player.id); const player = game.players.get(session.player.id);
  const action = (name, fields = {}) => { game.time += 100; game.action(player.id, { action: name, ...fields }); };
  return { game, player, session, action };
}

test('owner gear uses independent persistent slots without changing Chao stats or active mech state', () => {
  const { game, player, session, action } = fixture(true);
  assert.deepEqual(player.gear, { footwear: null, hands: null });
  action('mount-dark-walker'); game.setInput(player.id, { boost: true }); game.tick(0.05);
  const before = game.publicPlayer(player);
  action('equip-owner-gear', { item: 'shadow-air-shoes' }); action('equip-owner-gear', { item: 'knuckles-hammer-gloves' });
  assert.deepEqual(player.gear, { footwear: 'shadow-air-shoes', hands: 'knuckles-hammer-gloves' });
  const after = game.publicPlayer(player);
  for (const key of ['stats', 'rings', 'form', 'alignment', 'vehicle', 'mech', 'x', 'y', 'z']) assert.deepEqual(after[key], before[key]);
  const restored = new Game({ profiles: game.exportProfiles() });
  assert.deepEqual(restored.playerForToken(session.token).gear, player.gear);
  action('unequip-owner-gear', { slot: 'footwear' });
  assert.deepEqual(player.gear, { footwear: null, hands: 'knuckles-hammer-gloves' });
  after.gear.hands = 'unknown'; assert.equal(player.gear.hands, 'knuckles-hammer-gloves');
});

test('gear rejects forged ownership, unknown items or slots, and corrupt persisted choices', () => {
  const regular = fixture(false);
  regular.action('equip-owner-gear', { item: 'shadow-air-shoes', admin: true });
  regular.action('unequip-owner-gear', { slot: 'admin', admin: true });
  assert.deepEqual(regular.player.gear, { footwear: null, hands: null });
  const profiles = regular.game.exportProfiles(); profiles[0].gear = { footwear: 'shadow-air-shoes', hands: 'knuckles-hammer-gloves' };
  assert.deepEqual(new Game({ profiles }).players.get(regular.player.id).gear, { footwear: null, hands: null });
  const owner = fixture(true);
  for (const item of ['unknown', '__proto__', 'footwear', null]) owner.action('equip-owner-gear', { item });
  for (const slot of ['admin', '__proto__', 'unknown', null]) owner.action('unequip-owner-gear', { slot });
  assert.deepEqual(owner.player.gear, { footwear: null, hands: null });
  const corrupt = owner.game.exportProfiles(); corrupt[0].gear = { footwear: 'knuckles-hammer-gloves', hands: 'shadow-air-shoes', admin: true };
  assert.deepEqual(new Game({ profiles: corrupt }).players.get(owner.player.id).gear, { footwear: null, hands: null });
});
