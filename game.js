import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { INTERACTION_ANCHORS } from './constants.js';
import { restoreRecords, normalizeCatalog, karateAction, tickKarate, publicKarate, nativeCourse, publicActivities, enterRace, tickNativeRace } from './activities.js';
import { publicWatch, watchAction, tickWatch, restoreWatch } from './watch.js';

export const MAIN_STATS = ['swim', 'fly', 'run', 'power', 'stamina'];
export const ALL_STATS = [...MAIN_STATS, 'luck', 'intelligence'];
export const SHOP_CATALOG = Object.freeze([
  { id: 'garden-fruit', name: 'Garden Fruit', description: 'Restore 25 energy and fullness; gain 10 happiness.', price: 5, type: 'fruit' },
  { id: 'run-drive', name: 'Run Drive', description: 'Raise Run by 2 levels.', price: 12, type: 'drive' },
  { id: 'power-drive', name: 'Power Drive', description: 'Raise Power by 2 levels.', price: 12, type: 'drive' },
  { id: 'eggshell-cap', name: 'Eggshell Cap', description: 'Own and equip a little eggshell hat.', price: 25, type: 'cosmetic' },
].map((item) => Object.freeze(item)));
export const ZONES = ['hub', 'garden', 'hero', 'dark', 'mission', 'race', 'karate'];
export const RACE_CHECKPOINTS = [
  { x: 0, z: 12 }, { x: -12, z: 0 }, { x: 0, z: -12 },
  { x: 12, z: 0 }, { x: 0, z: 12 },
];
const WORLD_BOUND = 22;
const WALK_STEP_HEIGHT = 0.15;
const JUMP_SPEED = 2.4;
const GRAVITY = 12;
const ATTACK_RANGE = 0.8;
const RING_PICKUP_RADIUS = 0.55;
const RACE_CHECKPOINT_RADIUS = 0.8;
const MECH_FIRE_RANGE = 9;
const MECH_FIRE_DAMAGE = 60;
const MECH_FIRE_COOLDOWN = 350;
const MECH_BOOST_HEIGHT = 3;
const MECH_BOOST_SPEED = 1.8;
const MECH_BOOST_DRAIN = 40;
const MECH_BOOST_RECOVERY = 25;
const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const distance = (a, b) => Math.hypot(a.x - b.x, a.z - b.z);
const hashToken = (token) => createHash('sha256').update(token).digest('hex');
const cleanName = (value) => typeof value === 'string'
  ? value.replace(/[\u0000-\u001f\u007f<>]/g, '').trim().slice(0, 20) || 'Chao'
  : 'Chao';

function makeStats(admin = false) {
  return Object.fromEntries(ALL_STATS.map((name) => [name, {
    grade: admin ? (MAIN_STATS.includes(name) ? 5 : 255) : 1,
    level: admin ? 99 : 1,
    points: admin ? 9999 : 0,
  }]));
}

function restoreStats(value, admin) {
  const stats = makeStats(admin);
  if (admin) return stats;
  for (const name of ALL_STATS) {
    if (!value?.[name]) continue;
    const entry = value[name];
    for (const [key, max] of [['grade', MAIN_STATS.includes(name) ? 5 : 255], ['level', 99], ['points', 9999]]) {
      if (Number.isFinite(entry[key])) stats[name][key] = clamp(Math.floor(entry[key]), 0, max);
    }
  }
  return stats;
}

function validateTerrain(value, zone) {
  if (!value || !Number.isFinite(value.minX) || !Number.isFinite(value.minZ)
    || !Number.isFinite(value.step) || value.step <= 0
    || !Number.isInteger(value.width) || value.width < 1 || value.width > 4096
    || !Number.isInteger(value.depth) || value.depth < 1 || value.depth > 4096
    || !Array.isArray(value.heights) || value.heights.length !== value.width * value.depth
    || value.heights.some((height) => height !== null && !Number.isFinite(height))
    || !value.heights.some((height) => height !== null)) {
    throw new TypeError(`Invalid authoritative terrain for ${zone}.`);
  }
  return Object.freeze({
    minX: value.minX, minZ: value.minZ, step: value.step,
    width: value.width, depth: value.depth, heights: Object.freeze([...value.heights]),
  });
}

/** Authoritative in-memory simulation. Browser messages never contain rewards or stats. */
export class Game {
  constructor({ profiles = [], terrains = {}, activityTerrains = {}, raceCatalog = {}, karateCatalog = {}, allowOwnerBootstrap = false, now = () => Date.now() } = {}) {
    if (profiles.some(profile => profile && (profile.competition != null || profile.activityLease != null))) {
      throw new Error('Competition and activity-lease profiles require the Rust authority; the Node reference cannot safely write them.');
    }
    this.now = now;
    this.time = now();
    this.allowOwnerBootstrap = allowOwnerBootstrap;
    this.terrains = new Map(['hub', 'garden', 'hero', 'dark']
      .filter((zone) => Object.hasOwn(terrains, zone))
      .map((zone) => [zone, validateTerrain(terrains[zone], zone)]));
    this.players = new Map();
    this.tokens = new Map();
    this.events = [];
    this.dirty = false;
    this.mission = null;
    this.activities = normalizeCatalog(raceCatalog, karateCatalog);
    this.activityTerrains = new Map(Object.entries(activityTerrains).map(([id,field])=>[id,validateTerrain(field,id)]));
    this.rings = [
      [-6, 6], [6, 6], [-6, -6], [6, -6], [0, -9],
    ].map(([x, z], index) => ({ id: `garden-ring-${index}`, ...this.safePosition('garden', x, z), zone: 'garden', availableAt: 0 }));
    for (const data of profiles) {
      if (!data || typeof data.id !== 'string' || typeof data.tokenHash !== 'string') continue;
      const player = this.makePlayer(data);
      this.updateForm(player);
      this.players.set(player.id, player);
      this.tokens.set(player.tokenHash, player.id);
    }
  }

  makePlayer(data) {
    const admin = data.admin === true;
    const ownedCosmetics = Array.isArray(data.ownedCosmetics)
      ? [...new Set(data.ownedCosmetics.filter((id) => id === 'eggshell'))] : [];
    const player = {
      id: data.id, tokenHash: data.tokenHash, name: cleanName(data.name), admin,
      stats: restoreStats(data.stats, admin),
      rings: clamp(Number.isFinite(data.rings) ? Math.floor(data.rings) : 20, 0, 999999),
      alignment: clamp(Number.isFinite(data.alignment) ? data.alignment : 0, -100, 100),
      form: data.form === 'chaos' ? 'chaos' : 'normal',
      ownedCosmetics,
      equipped: data.equipped === 'eggshell' && ownedCosmetics.includes('eggshell') ? 'eggshell' : null,
      vehicle: admin && data.vehicle === 'dark-chao-walker' ? 'dark-chao-walker' : null,
      gear: { footwear: admin && data.gear?.footwear === 'shadow-air-shoes' ? 'shadow-air-shoes' : null,
        hands: admin && data.gear?.hands === 'knuckles-hammer-gloves' ? 'knuckles-hammer-gloves' : null },
      raceRecords: restoreRecords(data.raceRecords), karateRecords: restoreRecords(data.karateRecords), karate: null, activityResult: null,
      watch: null,
      // Forward-compatible receipts only; this older reference hosts no Tiny games.
      tinyRecords: structuredClone(data.tinyRecords && typeof data.tinyRecords === 'object' && !Array.isArray(data.tinyRecords) ? data.tinyRecords : {}),
      boosting: false, boostEnergy: 100,
      happiness: clamp(Number.isFinite(data.happiness) ? data.happiness : 65, 0, 100),
      energy: 100, hunger: 100, hp: admin ? 250 : 100, maxHp: admin ? 250 : 100,
      missionsCompleted: clamp(Number.isFinite(data.missionsCompleted) ? Math.floor(data.missionsCompleted) : 0, 0, 99999),
      racesCompleted: clamp(Number.isFinite(data.racesCompleted) ? Math.floor(data.racesCompleted) : 0, 0, 99999),
      bestRaceMs: Number.isFinite(data.bestRaceMs) && data.bestRaceMs > 0 ? data.bestRaceMs : null,
      x: 0, z: 6, y: 0, groundY: 0, jumpOffset: 0, vy: 0, facing: 0, zone: 'hub', connections: 0,
      input: { dx: 0, dz: 0, jump: false, boost: false }, lastInputAt: 0,
      cooldowns: new Map(), race: null,
    };
    this.teleport(player, 'hub');
    restoreWatch(this,player,data.resident);
    return player;
  }

  createSession({ name, token, resumeOnly } = {}) {
    if (typeof token === 'string' && token.length <= 256) {
      const existing = this.playerForToken(token);
      if (existing) return { token, player: this.publicPlayer(existing) };
    }
    if (resumeOnly === true) throw new Error('Saved Chao was not found.');
    const newToken = randomBytes(32).toString('hex');
    const player = this.makePlayer({
      id: randomUUID(), tokenHash: hashToken(newToken), name,
      // Local development bootstrap is a server option, never a request field.
      admin: this.allowOwnerBootstrap && this.players.size === 0,
    });
    this.updateForm(player);
    this.players.set(player.id, player);
    this.tokens.set(player.tokenHash, player.id);
    this.dirty = true;
    return { token: newToken, player: this.publicPlayer(player) };
  }

  playerForToken(token) {
    if (typeof token !== 'string' || token.length > 256) return null;
    return this.players.get(this.tokens.get(hashToken(token))) || null;
  }

  connect(id) {
    const player = this.players.get(id);
    if (!player) return false;
    player.connections++;
    player.input = { dx: 0, dz: 0, jump: false, boost: false };
    return true;
  }

  disconnect(id) {
    const player = this.players.get(id);
    if (!player) return;
    player.connections = Math.max(0, player.connections - 1);
    if (!player.connections && !player.watch) {
      this.teleport(player, 'hub');
    }
  }

  setInput(id, message = {}) {
    const player = this.players.get(id);
    if (!player || !player.connections) return;
    if (player.watch) return;
    let dx = Number.isFinite(message.dx) ? clamp(message.dx, -1, 1) : 0;
    let dz = Number.isFinite(message.dz) ? clamp(message.dz, -1, 1) : 0;
    const length = Math.hypot(dx, dz);
    if (length > 1) { dx /= length; dz /= length; }
    player.input = { dx, dz, jump: message.jump === true, boost: message.boost === true };
    player.lastInputAt = this.time;
  }

  requestSave(id) {
    const player = this.players.get(id);
    if (!player?.connections || !this.cooldown(player, 'save', 300)) return false;
    this.dirty = true; return true;
  }

  notice(id, text, kind = 'info') {
    this.events.push({ target: id, message: { type: 'notice', text, kind } });
  }

  cooldown(player, action, duration) {
    if ((player.cooldowns.get(action) ?? -Infinity) > this.time) return false;
    player.cooldowns.set(action, this.time + duration);
    return true;
  }

  gain(player, stat, points = 1) {
    const entry = player.stats[stat];
    entry.points = clamp(entry.points + points * 25, 0, 9999);
    entry.level = clamp(entry.level + points, 0, 99);
    if (MAIN_STATS.includes(stat) && !player.admin) entry.grade = Math.min(5, 1 + Math.floor(entry.level / 20));
    this.dirty = true;
  }

  updateForm(player) {
    if (player.form === 'chaos') return;
    player.form = player.alignment >= 60 ? 'hero' : player.alignment <= -60 ? 'dark' : 'normal';
  }

  sampleGround(zone, x, z) {
    if (zone === 'karate') return 0.75;
    const terrain = this.terrains.get(zone);
    if (!terrain) return 0; // Explicit development fallback; missions/races are flat.
    return this.sampleField(terrain, x, z);
  }

  playerTerrain(player) {
    const course = nativeCourse(this, player);
    return course ? this.activityTerrains.get(course.worldId || course.id) : this.terrains.get(player.zone);
  }

  playerGround(player, x, z) {
    if (player.zone === 'karate') return 0.75;
    const terrain = this.playerTerrain(player);
    return terrain ? this.sampleField(terrain, x, z) : 0;
  }

  sampleField(terrain, x, z) {
    const gx = (x - terrain.minX) / terrain.step;
    const gz = (z - terrain.minZ) / terrain.step;
    if (gx < 0 || gz < 0 || gx > terrain.width - 1 || gz > terrain.depth - 1) return null;
    const at = (column, row) => terrain.heights[row * terrain.width + column];
    const nearest = at(Math.round(gx), Math.round(gz));
    // A missing nearest cell remains a hole, even next to a valid triangle.
    if (nearest === null) return null;
    const x0 = Math.floor(gx); const z0 = Math.floor(gz);
    const x1 = Math.min(x0 + 1, terrain.width - 1); const z1 = Math.min(z0 + 1, terrain.depth - 1);
    const corners = [at(x0, z0), at(x1, z0), at(x0, z1), at(x1, z1)];
    if (corners.some((height) => height === null)) return nearest;
    // Do not smooth a cliff into a climbable artificial ramp.
    if (Math.max(...corners) - Math.min(...corners) > 0.75) return nearest;
    const tx = gx - x0; const tz = gz - z0;
    return (corners[0] * (1 - tx) + corners[1] * tx) * (1 - tz)
      + (corners[2] * (1 - tx) + corners[3] * tx) * tz;
  }

  safePosition(zone, requestedX = 0, requestedZ = 6) {
    const x = clamp(requestedX, -WORLD_BOUND, WORLD_BOUND);
    const z = clamp(requestedZ, -WORLD_BOUND, WORLD_BOUND);
    const y = this.sampleGround(zone, x, z);
    if (y !== null) return { x, z, y };
    const terrain = this.terrains.get(zone);
    let nearest = null; let nearestDistance = Infinity;
    for (let row = 0; row < terrain.depth; row++) {
      for (let column = 0; column < terrain.width; column++) {
        const height = terrain.heights[row * terrain.width + column];
        if (height === null) continue;
        const candidateX = terrain.minX + column * terrain.step;
        const candidateZ = terrain.minZ + row * terrain.step;
        if (Math.abs(candidateX) > WORLD_BOUND || Math.abs(candidateZ) > WORLD_BOUND) continue;
        const distanceSquared = (candidateX - x) ** 2 + (candidateZ - z) ** 2;
        if (distanceSquared < nearestDistance) {
          nearestDistance = distanceSquared; nearest = { x: candidateX, z: candidateZ, y: height };
        }
      }
    }
    if (!nearest) throw new Error(`No safe spawn within world bounds for ${zone}.`);
    return nearest;
  }

  nearInteraction(player, name) {
    const anchor = INTERACTION_ANCHORS[name];
    if (player.zone !== anchor.zone) return false;
    const ground = this.sampleGround(anchor.zone, anchor.x, anchor.z);
    return ground !== null && Math.hypot(player.x - anchor.x, player.z - anchor.z, player.y - ground) <= anchor.radius;
  }

  teleport(player, zone, x = 0, z = 6) {
    const position = this.safePosition(zone, x, z);
    player.zone = zone; player.x = position.x; player.z = position.z;
    player.groundY = position.y; player.y = position.y; player.jumpOffset = 0; player.vy = 0;
    player.input = { dx: 0, dz: 0, jump: false, boost: false }; player.boosting = false;
    if (zone !== 'race') player.race = null;
    if (zone !== 'karate') player.karate = null;
  }

  action(id, message = {}) {
    const player = this.players.get(id);
    if (!player || !player.connections || typeof message.action !== 'string') return;
    const action = message.action;
    if (!this.cooldown(player, 'message', 80)) return;
    if (action.startsWith('watch-')) return watchAction(this,player,message);
    if (player.watch) return this.notice(id,'Stop watching your Chao before starting another activity.','error');
    if (action.startsWith('karate-')) return karateAction(this,player,message);
    if (action === 'race-enter') return enterRace(this,player,message);
    if (action === 'race-leave') return this.teleport(player,'hub');
    if (action === 'attack' && player.zone === 'karate') return karateAction(this,player,{action:'karate-attack',move:'punch'});
    if (action === 'attack') return player.vehicle === 'dark-chao-walker' ? this.mechFire(player) : this.attack(player);
    if (action === 'mech-fire') return this.mechFire(player);
    if (action === 'admin-dark-chaos') {
      if (!player.admin) return this.notice(id, 'Only the owner admin can choose this form directly.', 'error');
      player.form = 'chaos'; player.alignment = -100; this.dirty = true;
      return this.notice(id, 'Your owner Chao is now a Dark Chaos Chao. Perfect stats preserved.', 'success');
    }
    if (action === 'mount-dark-walker') {
      if (player.zone === 'karate') return this.notice(id, 'Leave Chao Karate before mounting a vehicle.', 'error');
      if (player.race?.courseId) return this.notice(id, 'Leave your native Chao race before mounting a vehicle.', 'error');
      if (!player.admin) return this.notice(id, 'Only the owner admin can pilot the Dark Chao Walker.', 'error');
      player.vehicle = 'dark-chao-walker'; this.dirty = true;
      return this.notice(id, 'You are piloting the Dark Chao Walker.', 'success');
    }
    if (action === 'dismount-vehicle') {
      player.vehicle = null; player.boosting = false; this.dirty = true;
      return this.notice(id, 'You stepped out of your vehicle.');
    }
    if (action === 'equip-owner-gear' || action === 'unequip-owner-gear') {
      if (!player.admin) return this.notice(id, 'Only the owner admin can equip these extras.', 'error');
      if (action === 'equip-owner-gear') {
        if (message.item === 'shadow-air-shoes') player.gear.footwear = message.item;
        else if (message.item === 'knuckles-hammer-gloves') player.gear.hands = message.item;
        else return this.notice(id, 'Unknown gear item.', 'error');
      } else {
        if (!['footwear', 'hands'].includes(message.slot)) return this.notice(id, 'Unknown gear slot.', 'error');
        player.gear[message.slot] = null;
      }
      this.dirty = true; return this.notice(id, 'Your gear has been updated.', 'success');
    }
    if (action === 'rename') {
      player.name = cleanName(message.name); this.dirty = true;
      return this.notice(id, `Your name is now ${player.name}.`);
    }
    if (action === 'hub' || action === 'garden') {
      this.teleport(player, action); player.hp = player.maxHp;
      return this.notice(id, action === 'hub' ? 'Welcome back to the main hub.' : 'You entered the Neutral Garden.');
    }
    if (action === 'hero' || action === 'dark') {
      const allowed = action === 'hero' ? player.alignment >= 60 : player.alignment <= -60;
      if (!allowed) return this.notice(id, `${action === 'hero' ? 'Hero' : 'Dark'} Garden needs ${action === 'hero' ? '+60' : '−60'} alignment. Hero Fruit and Dark Fruit from the Black Market change your alignment.`, 'error');
      this.teleport(player, action);
      return this.notice(id, `You entered the ${action === 'hero' ? 'Hero' : 'Dark'} Garden.`);
    }
    if (action === 'mission') return this.enterMission(player);
    if (action === 'race') return this.startRace(player);
    if (action === 'evolve') {
      if (player.form === 'chaos') return this.notice(id, 'You have already become a Chaos Chao.');
      const requirements = this.evolutionRequirements(player);
      if (!requirements.ready) return this.notice(id, 'Chaos evolution: finish 3 missions and 1 race, reach Run/Swim/Power level 10, and happiness 80.', 'error');
      player.form = 'chaos'; this.dirty = true;
      return this.notice(id, 'Your adventures have awakened a Chaos Chao!', 'success');
    }
    if (!['hub', 'garden', 'hero', 'dark'].includes(player.zone)) return this.notice(id, 'Return to the hub or a garden for this activity.', 'error');
    if (action === 'buy') return this.buy(player, message.item);
    if (action === 'feed') {
      if (!this.nearInteraction(player, 'fruit')) return this.notice(id, 'Visit the Neutral Garden fruit grove, or buy Garden Fruit from Amy in the hub.', 'info');
      if (!this.cooldown(player, 'feed', 1200)) return;
      if (player.rings < 3) return this.notice(id, 'Fruit costs 3 rings. Explore to collect more.', 'error');
      player.rings -= 3; player.energy = Math.min(100, player.energy + 35);
      player.hunger = Math.min(100, player.hunger + 30);
      player.happiness = Math.min(100, player.happiness + 8);
      this.gain(player, 'stamina', 1); this.dirty = true;
      return this.notice(id, 'Yum! Fruit restored energy and raised Stamina.', 'success');
    }
    const stat = action.startsWith('train-') ? action.slice(6) : null;
    if (MAIN_STATS.includes(stat)) {
      if (stat === 'swim' ? !this.nearInteraction(player, 'swim') : !this.nearInteraction(player, 'trainer')) {
        return this.notice(id, stat === 'swim' ? 'Visit the Neutral Garden pond to practice swimming.' : 'Visit Knuckles in the hub for skill training.', 'info');
      }
      if (!this.cooldown(player, 'train', 900)) return;
      if (player.energy < 10) return this.notice(id, 'You need energy. Eat some fruit first.', 'error');
      player.energy -= 10; player.happiness = Math.min(100, player.happiness + 2);
      this.gain(player, stat, 2);
      return this.notice(id, `${stat[0].toUpperCase() + stat.slice(1)} training complete: +2 levels.`, 'success');
    }
  }

  buy(player, itemId) {
    if (!this.nearInteraction(player, 'shop')) return this.notice(player.id, 'Visit Amy in the main hub to buy from the Chao shop.', 'info');
    const item = typeof itemId === 'string' ? SHOP_CATALOG.find((entry) => entry.id === itemId) : null;
    if (!item) return this.notice(player.id, 'That item is not in the Chao shop.', 'error');
    if (item.id === 'eggshell-cap' && player.ownedCosmetics.includes('eggshell')) {
      return this.notice(player.id, 'You already own the Eggshell Cap.', 'info');
    }
    if (player.rings < item.price) return this.notice(player.id, `${item.name} costs ${item.price} rings. Collect a few more first.`, 'error');
    // Prices and effects come exclusively from this server catalog.
    player.rings -= item.price;
    if (item.id === 'garden-fruit') {
      player.energy = Math.min(100, player.energy + 25);
      player.hunger = Math.min(100, player.hunger + 25);
      player.happiness = Math.min(100, player.happiness + 10);
    } else if (item.id === 'run-drive') this.gain(player, 'run', 2);
    else if (item.id === 'power-drive') this.gain(player, 'power', 2);
    else if (item.id === 'eggshell-cap') {
      player.ownedCosmetics.push('eggshell'); player.equipped = 'eggshell';
    }
    this.dirty = true;
    this.notice(player.id, `Purchased ${item.name} for ${item.price} rings!`, 'success');
  }

  enterMission(player) {
    if (player.zone === 'mission') return this.notice(player.id, 'You are already in the arena. Defeat all five robots.');
    const occupied = [...this.players.values()].some((other) => other.connections && other.zone === 'mission');
    if (!this.mission || !occupied) {
      const id = randomUUID();
      this.mission = {
        id, completed: false, members: new Set(), rewarded: new Set(),
        enemies: [[-10, -7], [-4, -11], [4, -11], [10, -7], [0, -3]].map(([x, z], index) => ({
          id: `${id}-robot-${index}`, x, z, hp: 100, maxHp: 100, zone: 'mission', lastAttackAt: -Infinity,
        })),
        rings: [[-10, 6], [-5, 1], [0, -4], [5, 1], [10, 6]].map(([x, z], index) => ({
          id: `${id}-ring-${index}`, x, z, zone: 'mission', availableAt: 0,
        })),
      };
    }
    if (this.mission.completed) return this.notice(player.id, 'This arena is cleared. Everyone must return to a garden before a new mission.', 'info');
    this.mission.members.add(player.id);
    this.teleport(player, 'mission', 0, 15); player.hp = player.maxHp;
    this.notice(player.id, 'Mission: defeat 5 robots! Move close and attack. Clear reward: 50 rings.', 'success');
  }

  attack(player) {
    if (player.zone !== 'mission' || !this.mission || this.mission.completed) return this.notice(player.id, 'Attack robots inside the mission arena.');
    if (!this.cooldown(player, 'attack', 550)) return;
    this.events.push({ target: null, message: { type: 'event', kind: 'attack', playerId: player.id, x: player.x, z: player.z, zone: player.zone } });
    const enemy = this.mission.enemies.filter((item) => item.hp > 0
      && Math.hypot(player.x - item.x, player.z - item.z, player.y - (item.y || 0)) <= ATTACK_RANGE)
      .sort((a, b) => distance(player, a) - distance(player, b))[0];
    if (!enemy) return this.notice(player.id, 'Get close to a robot to hit it.');
    enemy.hp = Math.max(0, enemy.hp - (player.admin ? 100 : 35 + player.stats.power.level * 0.3));
    if (enemy.hp === 0) {
      player.rings += 5; this.gain(player, 'power', 1); this.dirty = true;
      this.notice(player.id, 'Robot defeated! +5 rings.', 'success');
    }
    if (this.mission.enemies.every((item) => item.hp <= 0)) this.finishMission();
  }

  mechFire(player) {
    if (!player.admin || player.vehicle !== 'dark-chao-walker') return this.notice(player.id, 'Mount your authorized Dark Chao Walker to fire.', 'error');
    if (!this.cooldown(player, 'mech-fire', MECH_FIRE_COOLDOWN)) return;
    const dx = Math.sin(player.facing); const dz = Math.cos(player.facing);
    const origin = { x: player.x + dx * 0.35, y: player.y + 0.65, z: player.z + dz * 0.35 };
    const targets = player.zone === 'mission' && this.mission && !this.mission.completed
      ? this.mission.enemies.filter(enemy => {
        const ex = enemy.x - player.x; const ez = enemy.z - player.z; const flat = Math.hypot(ex, ez);
        return enemy.hp > 0 && Math.hypot(ex, ez, 0.35 - origin.y) <= MECH_FIRE_RANGE
          && (flat < 0.001 || (ex * dx + ez * dz) / flat >= Math.SQRT1_2);
      }).sort((a, b) => distance(player, a) - distance(player, b)) : [];
    const target = targets[0];
    this.events.push({ target: null, message: { type: 'event', kind: 'mech-shot', playerId: player.id, zone: player.zone,
      ...origin, hitX: target ? target.x : origin.x + dx * MECH_FIRE_RANGE,
      hitY: target ? 0.35 : origin.y, hitZ: target ? target.z : origin.z + dz * MECH_FIRE_RANGE,
      targetId: target?.id || null, hit: Boolean(target) } });
    if (!target) return;
    target.hp = Math.max(0, target.hp - MECH_FIRE_DAMAGE);
    if (target.hp === 0) {
      player.rings += 5; this.gain(player, 'power', 1); this.dirty = true;
      this.notice(player.id, 'Robot defeated! +5 rings.', 'success');
    }
    if (this.mission.enemies.every(enemy => enemy.hp <= 0)) this.finishMission();
  }

  finishMission() {
    if (!this.mission || this.mission.completed) return;
    this.mission.completed = true;
    for (const id of this.mission.members) {
      const player = this.players.get(id);
      if (!player?.connections || player.zone !== 'mission' || this.mission.rewarded.has(id)) continue;
      this.mission.rewarded.add(id); player.rings += 50; player.missionsCompleted++;
      player.happiness = Math.min(100, player.happiness + 10);
      for (const stat of MAIN_STATS) this.gain(player, stat, 2);
      this.notice(id, 'Mission cleared! +50 rings and +2 to all five skill levels.', 'success');
    }
    this.dirty = true;
  }

  startRace(player) {
    if (player.race) return this.notice(player.id, 'Your race is already running. Follow the checkpoint order.');
    this.teleport(player, 'race', 0, 12);
    player.race = { startedAt: this.time, nextCheckpoint: 1 };
    this.notice(player.id, 'Race started! Follow the glowing checkpoints in order.', 'success');
  }

  tick(dt = 0.05) {
    if (!Number.isFinite(dt) || dt <= 0) return;
    dt = Math.min(dt, 0.1); this.time += dt * 1000;
    for (const player of this.players.values()) {
      if (!player.connections && !player.watch) continue;
      if (this.time - player.lastInputAt > 350) player.input = { dx: 0, dz: 0, jump: false, boost: false };
      tickWatch(this,player);
      const input = player.input;
      const water = player.zone === 'garden' && Math.hypot(player.x + 8, player.z) < 5;
      const speed = (1.4 + player.stats.run.level * 0.006) * (water ? 0.65 + player.stats.swim.level * 0.003 : 1);
      let ground = this.playerGround(player, player.x, player.z);
      if (ground === null) {
        const course=nativeCourse(this,player);
        const recovery = course ? { ...course.spawn, y:this.playerGround(player,course.spawn.x,course.spawn.z) } : this.safePosition(player.zone, player.x, player.z);
        player.x = recovery.x; player.z = recovery.z; ground = recovery.y;
        player.jumpOffset = 0; player.vy = 0;
      }
      const mounted = player.admin && player.vehicle === 'dark-chao-walker';
      if (input.jump && player.jumpOffset <= 0 && player.vy <= 0) player.vy = mounted ? 3.2 : JUMP_SPEED;
      input.jump = false;
      player.boosting = mounted && input.boost === true && player.boostEnergy > 0;
      let predictedY;
      if (player.boosting) {
        player.boostEnergy = Math.max(0, player.boostEnergy - MECH_BOOST_DRAIN * dt);
        player.vy = MECH_BOOST_SPEED;
        predictedY = Math.min(ground + MECH_BOOST_HEIGHT, ground + player.jumpOffset + player.vy * dt);
        if (predictedY >= ground + MECH_BOOST_HEIGHT) player.vy = 0;
      } else {
        predictedY = ground + player.jumpOffset + player.vy * dt - 0.5 * GRAVITY * dt * dt;
        player.vy -= GRAVITY * dt;
      }
      const course=nativeCourse(this,player);const bounds=course?.bounds||(player.zone==='karate'?{minX:-1.35,maxX:1.35,minZ:.4,maxZ:3.1}:{minX:-WORLD_BOUND,maxX:WORLD_BOUND,minZ:-WORLD_BOUND,maxZ:WORLD_BOUND});
      const movementSpeed=player.race?.courseId&&this.time<player.race.startedAt?0:speed;
      const nextX = clamp(player.x + input.dx * movementSpeed * dt, bounds.minX, bounds.maxX);
      const nextZ = clamp(player.z + input.dz * movementSpeed * dt, bounds.minZ, bounds.maxZ);
      const startX = player.x; const startZ = player.z; const startGround = ground;
      const terrain = this.playerTerrain(player);
      const subdivisions = terrain ? Math.max(1, Math.ceil(Math.hypot(nextX - startX, nextZ - startZ) / Math.min(0.2, terrain.step * 0.4))) : 1;
      for (let part = 1; part <= subdivisions; part++) {
        const candidateX = startX + (nextX - startX) * part / subdivisions;
        const candidateZ = startZ + (nextZ - startZ) * part / subdivisions;
        const candidateGround = this.playerGround(player, candidateX, candidateZ);
        if (candidateGround === null || (candidateGround - startGround > WALK_STEP_HEIGHT && predictedY < candidateGround)) break;
        player.x = candidateX; player.z = candidateZ; ground = candidateGround;
      }
      if (input.dx || input.dz) player.facing = Math.atan2(input.dx, input.dz);
      player.groundY = ground;
      player.y = Math.max(ground, player.boosting ? Math.min(predictedY, ground + MECH_BOOST_HEIGHT) : predictedY);
      player.jumpOffset = player.y - ground;
      if (player.jumpOffset === 0) player.vy = 0;
      if (player.jumpOffset === 0 && input.boost !== true) player.boostEnergy = Math.min(100, player.boostEnergy + MECH_BOOST_RECOVERY * dt);
      if (!player.watch) player.energy = Math.min(100, player.energy + dt * 1.1);
      if (!player.watch) {
        this.pickupRings(player);
        tickKarate(this,player,dt);
        this.tickRace(player);
      }
    }
    this.tickEnemies(dt);
  }

  pickupRings(player) {
    const nodes = player.zone === 'mission' ? this.mission?.rings || [] : this.rings;
    for (const ring of nodes) {
      if (ring.zone !== player.zone || ring.availableAt > this.time || distance(player, ring) > RING_PICKUP_RADIUS) continue;
      const ringGround = this.sampleGround(ring.zone, ring.x, ring.z);
      if (ringGround === null || Math.abs(player.y - ringGround) > 2.5) continue;
      ring.availableAt = player.zone === 'mission' ? Infinity : this.time + 20000;
      player.rings++; this.dirty = true;
      this.events.push({ target: player.id, message: { type: 'event', kind: 'ring', ringId: ring.id, x: ring.x, z: ring.z } });
    }
  }

  tickEnemies(dt) {
    if (!this.mission || this.mission.completed) return;
    const targets = [...this.players.values()].filter((player) => player.connections && player.zone === 'mission');
    if (!targets.length) return;
    for (const enemy of this.mission.enemies) {
      if (enemy.hp <= 0) continue;
      const target = targets.reduce((best, player) => distance(enemy, player) < distance(enemy, best) ? player : best);
      const length = distance(enemy, target);
      if (length > 0.55) {
        enemy.x = clamp(enemy.x + (target.x - enemy.x) / length * dt * 0.8, -20, 20);
        enemy.z = clamp(enemy.z + (target.z - enemy.z) / length * dt * 0.8, -20, 20);
      } else if (this.time - enemy.lastAttackAt > 1300 && target.y < 0.4) {
        enemy.lastAttackAt = this.time; target.hp = Math.max(0, target.hp - 8);
        if (target.hp <= 0) {
          this.teleport(target, 'hub'); target.hp = target.maxHp;
          this.notice(target.id, 'The robots overwhelmed you. You recovered safely in the main hub.', 'error');
        }
      }
    }
  }

  tickRace(player) {
    if (player.race?.courseId) return tickNativeRace(this,player);
    if (player.zone !== 'race' || !player.race) return;
    const race = player.race;
    if (distance(player, RACE_CHECKPOINTS[race.nextCheckpoint]) > RACE_CHECKPOINT_RADIUS) return;
    race.nextCheckpoint++;
    if (race.nextCheckpoint < RACE_CHECKPOINTS.length) {
      return this.notice(player.id, `Checkpoint ${race.nextCheckpoint - 1}/4!`);
    }
    const elapsed = Math.round(this.time - race.startedAt);
    // Crossing real checkpoints through bounded simulation determines completion.
    player.racesCompleted++; player.rings += 25;
    player.bestRaceMs = player.bestRaceMs ? Math.min(player.bestRaceMs, elapsed) : elapsed;
    this.gain(player, 'run', 3); player.happiness = Math.min(100, player.happiness + 10);
    this.teleport(player, 'hub'); this.dirty = true;
    this.notice(player.id, `Race finished in ${(elapsed / 1000).toFixed(1)}s! +25 rings.`, 'success');
  }

  evolutionRequirements(player) {
    const requirements = {
      missions: { current: player.missionsCompleted, required: 3 },
      races: { current: player.racesCompleted, required: 1 },
      run: { current: player.stats.run.level, required: 10 },
      swim: { current: player.stats.swim.level, required: 10 },
      power: { current: player.stats.power.level, required: 10 },
      happiness: { current: player.happiness, required: 80 },
    };
    return { ready: Object.values(requirements).every(({ current, required }) => current >= required), requirements };
  }

  publicPlayer(player) {
    return {
      id: player.id, name: player.name, x: player.x, y: player.y, z: player.z,
      groundY: player.groundY, jumpOffset: player.jumpOffset,
      facing: player.facing, zone: player.zone, alignment: player.alignment,
      form: player.form, admin: player.admin, stats: structuredClone(player.stats),
      ownedCosmetics: [...player.ownedCosmetics], equipped: player.equipped, vehicle: player.vehicle, gear: { ...player.gear },
      raceRecords: structuredClone(player.raceRecords), karateRecords: structuredClone(player.karateRecords),
      tinyRecords: structuredClone(player.tinyRecords),
      watch: publicWatch(player.watch,player.connections>0), connected:player.connections>0,
      mech: player.vehicle === 'dark-chao-walker' ? { boosting: player.boosting, boostEnergy: Math.round(player.boostEnergy), grounded: player.jumpOffset <= 0 && player.vy <= 0 } : null,
      rings: player.rings, hp: player.hp, maxHp: player.maxHp,
      happiness: player.happiness, energy: Math.round(player.energy), hunger: Math.round(player.hunger),
      missionsCompleted: player.missionsCompleted, racesCompleted: player.racesCompleted,
      bestRaceMs: player.bestRaceMs, evolution: this.evolutionRequirements(player),
    };
  }

  snapshot(selfId) {
    const self = this.players.get(selfId);
    const rings = [...this.rings, ...(this.mission?.rings || [])]
      .filter((ring) => ring.availableAt <= this.time).map(({ id, x, z, zone }) => ({ id, x, z, zone }));
    return {
      type: 'snapshot', selfId, serverTime: this.time,
      players: [...this.players.values()].filter((player) => player.connections || player.watch).map((player) => this.publicPlayer(player)),
      rings,
      enemies: this.mission?.enemies.filter((enemy) => enemy.hp > 0).map(({ id, x, z, hp, maxHp, zone }) => ({ id, x, z, hp, maxHp, zone })) || [],
      mission: this.mission ? { id: this.mission.id, completed: this.mission.completed, remaining: this.mission.enemies.filter((enemy) => enemy.hp > 0).length } : null,
      race: self?.race ? { ...self.race, active: true,
        native:Boolean(self.race.courseId),courseId:self.race.courseId||'prototype',name:nativeCourse(this,self)?.name||'Garden Circuit (prototype)',
        phase:self.race.courseId&&this.time<self.race.startedAt?'countdown':'racing',model:nativeCourse(this,self)?.model||null,worldId:nativeCourse(this,self)?.worldId||null,
        checkpoints: nativeCourse(this,self)?.checkpoints||RACE_CHECKPOINTS,bestMs:self.race.courseId?self.raceRecords[self.race.courseId]?.bestMs||null:self.bestRaceMs } : null,
      shop: { catalog: SHOP_CATALOG },
      activities: publicActivities(this), karate: publicKarate(self?.karate), activityResult: self?.activityResult || null,
    };
  }

  drainEvents() { return this.events.splice(0); }

  exportProfiles() {
    return [...this.players.values()].map((player) => ({
      id: player.id, tokenHash: player.tokenHash, name: player.name, admin: player.admin,
      stats: structuredClone(player.stats), rings: player.rings, alignment: player.alignment,
      form: player.form, happiness: player.happiness,
      ownedCosmetics: [...player.ownedCosmetics], equipped: player.equipped, vehicle: player.vehicle, gear: { ...player.gear },
      raceRecords: structuredClone(player.raceRecords), karateRecords: structuredClone(player.karateRecords),
      tinyRecords: structuredClone(player.tinyRecords),
      resident: player.watch ? {enabled:true,zone:player.zone,x:player.x,z:player.z} : null,
      missionsCompleted: player.missionsCompleted, racesCompleted: player.racesCompleted,
      bestRaceMs: player.bestRaceMs,
    }));
  }
}

export const createGame = (options) => new Game(options);
