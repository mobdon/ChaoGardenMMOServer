import test from 'node:test';
import assert from 'node:assert/strict';
import { Game, MAIN_STATS, ALL_STATS, RACE_CHECKPOINTS, SHOP_CATALOG } from '../../game.js';

function fixture(options = {}) {
  const game = new Game({ now: () => 1000, ...options });
  const session = game.createSession({ name: 'Test Chao' });
  game.connect(session.player.id);
  return { game, player: game.players.get(session.player.id), session };
}

function advance(game, milliseconds) {
  for (let elapsed = 0; elapsed < milliseconds; elapsed += 50) game.tick(0.05);
}

function act(game, player, action, extra = {}) {
  advance(game, 100);
  game.action(player.id, { action, ...extra });
}

test('only the first local-bootstrap profile becomes owner, with S and maximum hidden grades', () => {
  const { game, player, session } = fixture({ allowOwnerBootstrap: true });
  assert.equal(player.admin, true);
  for (const stat of MAIN_STATS) assert.equal(player.stats[stat].grade, 5);
  for (const stat of ALL_STATS) assert.equal(player.stats[stat].level, 99);
  assert.equal(player.stats.luck.grade, 255);
  assert.equal(player.stats.intelligence.grade, 255);
  const second = game.createSession({ name: 'Imposter', owner: true, admin: true, stats: player.stats });
  assert.equal(second.player.admin, false);
  assert.equal(second.player.stats.run.level, 1);
  assert.notEqual(second.token, session.token);
  const resume = game.createSession({ token: session.token, name: 'Ignored' });
  assert.equal(resume.player.id, player.id);
  assert.equal(resume.player.name, 'Test Chao');
});

test('only the owner can mount a persisted Dark Chao Walker without changing stats', () => {
  const { game, player, session } = fixture({ allowOwnerBootstrap: true });
  const before = game.publicPlayer(player);
  assert.equal(before.vehicle, null);
  act(game, player, 'mount-dark-walker', { vehicle: 'other' });
  assert.equal(player.vehicle, 'dark-chao-walker');
  for (const field of ['stats', 'rings', 'form', 'alignment', 'x', 'y', 'z']) {
    assert.deepEqual(game.publicPlayer(player)[field], before[field]);
  }
  const restored = new Game({ profiles: game.exportProfiles(), now: () => 1000 });
  const owner = restored.playerForToken(session.token); restored.connect(owner.id);
  assert.equal(owner.vehicle, 'dark-chao-walker');
  act(restored, owner, 'dismount-vehicle'); assert.equal(owner.vehicle, null);
  const regular = fixture(); act(regular.game, regular.player, 'mount-dark-walker', { admin: true });
  assert.equal(regular.player.vehicle, null);
  const profiles = regular.game.exportProfiles(); profiles[0].vehicle = 'dark-chao-walker';
  const cleaned = new Game({ profiles }); assert.equal(cleaned.players.get(regular.player.id).vehicle, null);
});

test('owner bootstrap is disabled in the pure model unless explicitly enabled', () => {
  assert.equal(fixture().player.admin, false);
});

test('resume-only sessions reject missing/unknown tokens without creating another Chao', () => {
  const { game, session, player } = fixture();
  const count = game.players.size;
  for (const token of [undefined, null, '', 'unknown', 'x'.repeat(257)]) {
    assert.throws(() => game.createSession({ token, resumeOnly: true }), /Saved Chao was not found/);
    assert.equal(game.players.size, count);
  }
  assert.equal(game.createSession({ token: session.token, resumeOnly: true }).player.id, player.id);
  assert.equal(game.createSession({ token: 'legacy-unknown' }).player.name, 'Chao');
});

test('profiles preserve owner/progression without storing raw session tokens', () => {
  const { game, player, session } = fixture({ allowOwnerBootstrap: true });
  player.rings = 123; player.missionsCompleted = 2;
  const profiles = game.exportProfiles();
  assert.equal(JSON.stringify(profiles).includes(session.token), false);
  const reloaded = new Game({ profiles, now: () => 1000, allowOwnerBootstrap: true });
  assert.equal(reloaded.playerForToken(session.token).rings, 123);
  assert.equal(reloaded.playerForToken(session.token).missionsCompleted, 2);
  assert.equal(reloaded.playerForToken(session.token).admin, true);
  assert.equal(reloaded.createSession({ name: 'New Chao', admin: true }).player.admin, false);
});

test('normal Hero/Dark forms are restored from persisted alignment', () => {
  const { game, player, session } = fixture();
  player.alignment = -80;
  const reloaded = new Game({ profiles: game.exportProfiles(), now: () => 1000 });
  assert.equal(reloaded.playerForToken(session.token).form, 'dark');
});

test('older reference preserves Rust Tiny receipts across profile writes without hosting minigames', () => {
  const {game,session}=fixture();const profiles=game.exportProfiles();
  profiles[0].tinyRecords={memory:{plays:1,completions:1,ringsEarned:86,lastResultId:'01234567-89ab-4def-8123-456789abcdef'}};
  const reference=new Game({profiles});assert.deepEqual(reference.exportProfiles()[0].tinyRecords,profiles[0].tinyRecords);
  assert.deepEqual(reference.playerForToken(session.token).tinyRecords,profiles[0].tinyRecords);
});

test('client-forged admin, coordinates, rewards, and stats never enter authoritative state', () => {
  const { game, player } = fixture();
  const initialRings = player.rings;
  game.setInput(player.id, { dx: 0, dz: 0, x: 9999, z: 9999, admin: true, rings: 99999 });
  act(game, player, 'reward', { amount: 99999, admin: true });
  act(game, player, 'set-stats', { stats: { run: { grade: 5, level: 99 } } });
  act(game, player, 'mission-complete', { rings: 99999 });
  assert.equal(player.admin, false);
  assert.equal(player.rings, initialRings);
  assert.equal(player.stats.run.level, 1);
  assert.equal(player.x, 0);
  assert.equal(player.z, 6);
});

test('movement normalizes input and bounds speed; stale input stops movement', () => {
  const { game, player } = fixture();
  player.x = 0; player.z = 0;
  game.setInput(player.id, { dx: 100000, dz: 100000 });
  game.tick(0.1);
  assert.ok(Math.hypot(player.x, player.z) <= 0.473);
  const beforeStale = player.x;
  advance(game, 1000);
  const stopped = player.x;
  advance(game, 1000);
  assert.equal(player.x, stopped);
  assert.ok(stopped > beforeStale);
  player.x = 21.99; player.z = 21.99;
  game.setInput(player.id, { dx: 1, dz: 1 }); game.tick(0.1);
  assert.equal(player.x, 22); assert.equal(player.z, 22);
  game.setInput(player.id, { dx: NaN, dz: Infinity }); game.tick(0.1);
  assert.ok(Number.isFinite(player.x) && Number.isFinite(player.z));
});

test('rings require real proximity, cannot be collected twice, and regenerate after twenty seconds', () => {
  const { game, player } = fixture();
  act(game, player, 'garden');
  const ring = game.rings[0]; const initial = player.rings;
  act(game, player, 'collect', { id: ring.id });
  assert.equal(player.rings, initial);
  player.x = ring.x; player.z = ring.z;
  game.tick(0.05);
  assert.equal(player.rings, initial + 1);
  game.tick(0.05);
  assert.equal(player.rings, initial + 1);
  advance(game, 20100);
  assert.equal(player.rings, initial + 2);
});

test('hero and dark gardens enforce alignment thresholds and alignment stays bounded', () => {
  const { game, player } = fixture();
  act(game, player, 'hero'); assert.equal(player.zone, 'hub');
  act(game, player, 'dark'); assert.equal(player.zone, 'hub');
  for (const action of ['align-good', 'align-dark']) { act(game, player, action); assert.equal(player.alignment, 0); }
  player.alignment = 60;
  act(game, player, 'hero'); assert.equal(player.zone, 'hero');
  act(game, player, 'garden');
  player.alignment = -100;
  act(game, player, 'dark'); assert.equal(player.zone, 'dark');
});

test('training consumes energy, respects cooldowns and the swimming pond', () => {
  const { game, player } = fixture();
  player.x = 7; player.z = 2;
  act(game, player, 'train-run');
  assert.equal(player.stats.run.level, 3);
  const afterTraining = player.energy;
  act(game, player, 'train-run');
  assert.equal(player.stats.run.level, 3);
  assert.ok(player.energy >= afterTraining);
  advance(game, 1000);
  act(game, player, 'garden');
  act(game, player, 'train-swim'); assert.equal(player.stats.swim.level, 1);
  advance(game, 1000); player.x = -8; player.z = 0;
  act(game, player, 'train-swim'); assert.equal(player.stats.swim.level, 3);
  assert.ok(player.energy < 100);
});

test('attack range and cooldown apply; clearing a mission pays each participant once', () => {
  const { game, player } = fixture({ allowOwnerBootstrap: true });
  const otherSession = game.createSession({ name: 'Friend' }); game.connect(otherSession.player.id);
  const other = game.players.get(otherSession.player.id);
  act(game, player, 'mission'); act(game, other, 'mission');
  const missionId = game.mission.id;
  const firstEnemy = game.mission.enemies[0];
  const initialRings = player.rings;
  act(game, player, 'attack', { enemyId: firstEnemy.id, damage: 999999 });
  assert.equal(firstEnemy.hp, 100);
  advance(game, 600);
  player.x = firstEnemy.x; player.z = firstEnemy.z;
  act(game, player, 'attack'); assert.equal(firstEnemy.hp, 0);
  const secondEnemy = game.mission.enemies[1];
  player.x = secondEnemy.x; player.z = secondEnemy.z;
  act(game, player, 'attack'); assert.equal(secondEnemy.hp, 100);
  for (const enemy of game.mission.enemies.filter((entry) => entry.hp > 0)) {
    advance(game, 600); player.x = enemy.x; player.z = enemy.z;
    act(game, player, 'attack');
  }
  assert.equal(game.mission.completed, true);
  assert.equal(player.missionsCompleted, 1);
  assert.equal(other.missionsCompleted, 1);
  assert.ok(player.rings >= initialRings + 25 + 50 && player.rings <= initialRings + 25 + 50 + 5);
  const paid = player.rings; const friendPaid = other.rings;
  game.finishMission(); game.finishMission();
  act(game, player, 'mission-complete', { reward: 99999 });
  assert.equal(player.rings, paid); assert.equal(other.rings, friendPaid);
  assert.equal(game.mission.id, missionId);
  act(game, player, 'garden'); act(game, other, 'garden');
  act(game, player, 'mission'); assert.notEqual(game.mission.id, missionId);
});

test('race requires ordered physical checkpoints and rewards once per completion', () => {
  const { game, player } = fixture();
  act(game, player, 'race');
  const initialRings = player.rings;
  act(game, player, 'race-complete', { time: 1, reward: 10000 });
  assert.equal(player.racesCompleted, 0);
  player.x = 0; player.z = 12; game.tick(0.05);
  assert.equal(player.race.nextCheckpoint, 1);
  for (const target of RACE_CHECKPOINTS.slice(1)) {
    for (let i = 0; i < 1000 && player.race; i++) {
      const previousCheckpoint = player.race.nextCheckpoint;
      const dx = target.x - player.x; const dz = target.z - player.z;
      const length = Math.hypot(dx, dz);
      game.setInput(player.id, { dx: dx / length, dz: dz / length }); game.tick(0.05);
      if (!player.race || player.race.nextCheckpoint !== previousCheckpoint) break;
    }
  }
  assert.equal(player.racesCompleted, 1);
  assert.equal(player.zone, 'hub');
  assert.equal(player.rings, initialRings + 25);
  assert.ok(player.bestRaceMs > 8000);
  advance(game, 1000);
  assert.equal(player.racesCompleted, 1);
  assert.equal(player.rings, initialRings + 25);
});

test('Chaos evolution requires earned milestones even for the perfect-stat owner', () => {
  const { game, player } = fixture({ allowOwnerBootstrap: true });
  act(game, player, 'evolve', { missionsCompleted: 3, racesCompleted: 1 });
  assert.notEqual(player.form, 'chaos');
  player.missionsCompleted = 3; player.racesCompleted = 1; player.happiness = 80;
  act(game, player, 'evolve'); assert.equal(player.form, 'chaos');
});

test('snapshots expose no session secrets and disconnect cancels movement/race', () => {
  const { game, player, session } = fixture();
  act(game, player, 'race');
  const encoded = JSON.stringify(game.snapshot(player.id));
  assert.equal(encoded.includes(session.token), false);
  assert.equal(encoded.includes(player.tokenHash), false);
  game.disconnect(player.id);
  assert.equal(player.race, null);
  assert.equal(player.zone, 'hub');
  assert.equal(game.snapshot(player.id).players.length, 0);
});

test('only owner admin can choose Dark Chaos directly, without currency or stat changes', () => {
  const { game, player, session } = fixture({ allowOwnerBootstrap: true });
  const originalStats = structuredClone(player.stats); const originalRings = player.rings;
  const otherSession = game.createSession({ name: 'Regular' }); game.connect(otherSession.player.id);
  const other = game.players.get(otherSession.player.id);
  act(game, other, 'admin-dark-chaos', { admin: true, alignment: -100, form: 'chaos' });
  assert.equal(other.form, 'normal'); assert.equal(other.alignment, 0);
  act(game, player, 'admin-dark-chaos');
  assert.equal(player.form, 'chaos'); assert.equal(player.alignment, -100);
  assert.deepEqual(player.stats, originalStats); assert.equal(player.rings, originalRings);
  const restored = new Game({ profiles: game.exportProfiles(), now: () => 1000 });
  assert.equal(restored.playerForToken(session.token).form, 'chaos');
  assert.equal(restored.playerForToken(session.token).alignment, -100);
  assert.deepEqual(restored.playerForToken(session.token).stats, originalStats);
});

test('shop uses fixed server catalog prices/effects and refuses unknown items or overspending', () => {
  const { game, player } = fixture();
  player.x = -4; player.z = 8;
  assert.deepEqual(game.snapshot(player.id).shop.catalog, SHOP_CATALOG);
  player.energy = 20; player.hunger = 30; player.happiness = 40;
  act(game, player, 'buy', { item: 'garden-fruit', price: -999, energy: 99999, rings: 99999 });
  assert.equal(player.rings, 15);
  assert.ok(player.energy >= 45 && player.energy < 46);
  assert.equal(player.hunger, 55); assert.equal(player.happiness, 50);
  act(game, player, 'buy', { item: 'run-drive', price: 0, levels: 99 });
  assert.equal(player.rings, 3); assert.equal(player.stats.run.level, 3);
  const statsBefore = structuredClone(player.stats);
  act(game, player, 'buy', { item: 'power-drive', price: 0 });
  assert.equal(player.rings, 3); assert.deepEqual(player.stats, statsBefore);
  act(game, player, 'buy', { item: '../profiles.json', price: -100 });
  act(game, player, 'buy', { item: { id: 'garden-fruit', price: 0 } });
  assert.equal(player.rings, 3); assert.deepEqual(player.stats, statsBefore);
});

test('shop requires Amy in the hub and cosmetic ownership/equipment persist without duplicate charges', () => {
  const { game, player, session } = fixture();
  player.rings = 50;
  act(game, player, 'mission');
  act(game, player, 'buy', { item: 'eggshell-cap' });
  assert.equal(player.rings, 50); assert.equal(player.equipped, null);
  act(game, player, 'garden');
  act(game, player, 'buy', { item: 'eggshell-cap' });
  assert.equal(player.rings, 50); assert.equal(player.equipped, null);
  act(game, player, 'hub'); player.x = -4; player.z = 8;
  act(game, player, 'buy', { item: 'eggshell-cap' });
  assert.equal(player.rings, 25);
  assert.deepEqual(player.ownedCosmetics, ['eggshell']); assert.equal(player.equipped, 'eggshell');
  act(game, player, 'buy', { item: 'eggshell-cap' });
  assert.equal(player.rings, 25); assert.deepEqual(player.ownedCosmetics, ['eggshell']);
  const visible = game.publicPlayer(player);
  assert.deepEqual(visible.ownedCosmetics, ['eggshell']); assert.equal(visible.equipped, 'eggshell');
  const restored = new Game({ profiles: game.exportProfiles(), now: () => 1000 });
  assert.equal(restored.playerForToken(session.token).equipped, 'eggshell');
  assert.deepEqual(restored.playerForToken(session.token).ownedCosmetics, ['eggshell']);
});

test('profile restoration rejects unknown cosmetics, unowned equipment and duplicate ownership', () => {
  const { game, session } = fixture();
  const profiles = game.exportProfiles();
  profiles[0].ownedCosmetics = ['made-up', 'eggshell', 'eggshell']; profiles[0].equipped = 'made-up';
  let restored = new Game({ profiles, now: () => 1000 });
  assert.deepEqual(restored.playerForToken(session.token).ownedCosmetics, ['eggshell']);
  assert.equal(restored.playerForToken(session.token).equipped, null);
  profiles[0].ownedCosmetics = []; profiles[0].equipped = 'eggshell';
  restored = new Game({ profiles, now: () => 1000 });
  assert.equal(restored.playerForToken(session.token).equipped, null);
});

function terrain(rows, { minX = 0, minZ = 5, step = 1 } = {}) {
  return { minX, minZ, step, width: rows[0].length, depth: rows.length, heights: rows.flat() };
}

test('authoritative terrain interpolates gentle ground, preserves holes and rejects malformed maps', () => {
  const gentle = terrain([[0, 0.4, 0.8], [0, 0.4, 0.8]], { minX: 0, minZ: 0 });
  const { game } = fixture({ terrains: { hub: gentle } });
  assert.ok(Math.abs(game.sampleGround('hub', 0.5, 0.5) - 0.2) < 1e-9);
  assert.equal(game.sampleGround('hub', -1, 0), null);
  assert.equal(game.sampleGround('mission', -1, 0), 0);
  const withHole = terrain([[2, null, 2], [2, null, 2]], { minZ: 0 });
  const holeGame = new Game({ terrains: { hub: withHole }, now: () => 1000 });
  assert.equal(holeGame.sampleGround('hub', 1, 0.5), null);
  assert.equal(holeGame.sampleGround('hub', 0.1, 0.5), 2);
  assert.throws(() => new Game({ terrains: { hub: { ...gentle, heights: [NaN] } } }), /Invalid authoritative terrain/);
  assert.throws(() => new Game({ terrains: { hub: { ...gentle, heights: Array(6).fill(null) } } }), /Invalid authoritative terrain/);
});

test('terrain spawn searches valid ground and client-forged height/jump state is ignored', () => {
  const map = terrain([[null, null, 7], [null, null, null], [null, null, null]], { minZ: 0 });
  const { game, player } = fixture({ terrains: { hub: map } });
  assert.deepEqual({ x: player.x, z: player.z, y: player.y }, { x: 2, z: 0, y: 7 });
  game.setInput(player.id, { dx: 0, dz: 0, y: 99999, groundY: -9999, jumpOffset: 99999, vy: 99999 });
  game.tick(0.05);
  assert.equal(player.y, 7); assert.equal(player.groundY, 7); assert.equal(player.jumpOffset, 0);
  game.setInput(player.id, { dx: 0, dz: 0, jump: true, y: 99999 }); game.tick(0.05);
  assert.ok(player.y > 7 && player.y < 8);
  advance(game, 1500);
  assert.equal(player.y, 7); assert.equal(player.jumpOffset, 0);
  game.teleport(player, 'mission'); assert.equal(player.y, 0); assert.equal(player.groundY, 0);
  game.disconnect(player.id); assert.equal(player.y, 7);
});

test('server movement cannot cross missing terrain cells or high cliffs, including with forged input', () => {
  for (const rows of [
    [[0, null, 0, 0], [0, null, 0, 0], [0, null, 0, 0]],
    [[0, 4, 4, 4], [0, 4, 4, 4], [0, 4, 4, 4]],
  ]) {
    const { game, player } = fixture({ terrains: { hub: terrain(rows) }, allowOwnerBootstrap: true });
    for (let i = 0; i < 80; i++) {
      game.setInput(player.id, { dx: 999999, dz: 0, jump: i === 20, y: 99999 }); game.tick(0.05);
    }
    assert.ok(player.x < 0.5, `Crossed blocked cell at ${player.x}`);
    assert.equal(player.groundY, 0); assert.equal(player.y, 0);
  }
});

test('a real server-simulated jump can clear a low ledge while walking cannot', () => {
  const { game, player } = fixture({ terrains: { hub: terrain([
    [null, null, null, null], [0, 0.2, 0.2, 0.2], [null, null, null, null],
  ]) } });
  for (let i = 0; i < 15; i++) {
    game.setInput(player.id, { dx: 1, dz: 0 }); game.tick(0.05);
  }
  assert.ok(player.x < 0.5); assert.equal(player.groundY, 0);
  for (let i = 0; i < 24; i++) {
    game.setInput(player.id, { dx: 1, dz: 0, jump: i === 0 }); game.tick(0.05);
  }
  assert.ok(player.x > 1.5); assert.equal(player.groundY, 0.2);
  advance(game, 1000); assert.equal(player.y, 0.2); assert.equal(player.jumpOffset, 0);
});

test('native-scale movement and jumping stay within measured world distances', () => {
  for (const admin of [false, true]) {
    const { game, player } = fixture({ allowOwnerBootstrap: admin });
    player.x = 0; player.z = 0;
    for (let i = 0; i < 20; i++) {
      game.setInput(player.id, { dx: 999999, dz: 0 }); game.tick(0.05);
    }
    assert.ok(player.x > (admin ? 1.98 : 1.39) && player.x < (admin ? 2.01 : 1.42));
    let highest = 0;
    for (let i = 0; i < 30; i++) {
      game.setInput(player.id, { dx: 0, dz: 0, jump: i === 0 }); game.tick(0.05);
      highest = Math.max(highest, player.jumpOffset);
    }
    assert.ok(highest >= 0.235 && highest <= 0.245);
    assert.equal(player.jumpOffset, 0);
  }
});

test('native-scale melee rejects a robot just outside reach and hits one just inside', () => {
  const { game, player } = fixture({ allowOwnerBootstrap: true });
  act(game, player, 'mission'); advance(game, 600);
  const enemy = game.mission.enemies[0];
  player.x = 0; player.z = 0; player.y = 0; enemy.x = 0.81; enemy.z = 0;
  game.action(player.id, { action: 'attack', range: 9999 });
  assert.equal(enemy.hp, 100);
  advance(game, 600);
  player.x = 0; player.z = 0; player.y = 0; enemy.x = 0.79; enemy.z = 0;
  game.action(player.id, { action: 'attack' });
  assert.equal(enemy.hp, 0);
});

test('melee hits also require vertical proximity on the authoritative simulation', () => {
  const { game, player } = fixture({ allowOwnerBootstrap: true });
  act(game, player, 'mission');
  advance(game, 100);
  const enemy = game.mission.enemies[0]; player.x = enemy.x; player.z = enemy.z;
  // Simulate a server-owned elevated actor; action payload cannot choose target height.
  player.y = 5;
  game.action(player.id, { action: 'attack', y: 0, enemyId: enemy.id });
  assert.equal(enemy.hp, 100);
});

test('main hub is home, Neutral Garden is separate, and interactions require authoritative proximity', () => {
  const { game, player } = fixture();
  assert.equal(player.zone, 'hub');
  const initialRun = player.stats.run.level; const initialRings = player.rings;
  act(game, player, 'train-run', { x: 7, z: 2 });
  act(game, player, 'buy', { item: 'garden-fruit', x: -4, z: 8 });
  assert.equal(player.stats.run.level, initialRun); assert.equal(player.rings, initialRings);
  player.x = -1.99; player.z = 8; // Just outside Amy's two-unit radius.
  act(game, player, 'buy', { item: 'garden-fruit' }); assert.equal(player.rings, initialRings);
  player.x = -2.01;
  act(game, player, 'buy', { item: 'garden-fruit' }); assert.equal(player.rings, initialRings - 5);
  act(game, player, 'garden'); assert.equal(player.zone, 'garden');
  act(game, player, 'feed'); assert.equal(player.rings, initialRings - 5);
  player.x = -7; player.z = -5;
  act(game, player, 'feed'); assert.equal(player.rings, initialRings - 8);
  act(game, player, 'hub'); assert.equal(player.zone, 'hub');
  player.x = 7; player.z = 2;
  act(game, player, 'train-run'); assert.equal(player.stats.run.level, initialRun + 2);
  game.disconnect(player.id); assert.equal(player.zone, 'hub');
});
