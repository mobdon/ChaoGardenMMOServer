// Original activity catalog/opponent data; adapted browser controls and combat.
import { randomUUID } from 'node:crypto';

const clamp = (value, min, max) => Math.max(min, Math.min(max, value));
const validId = id => typeof id === 'string' && /^[a-z0-9-]{1,64}$/.test(id);
export function restoreRecords(value) {
  const records = Object.create(null);
  for (const [id, row] of Object.entries(value && typeof value === 'object' ? value : {}).slice(0, 256)) {
    if (!validId(id) || !row || typeof row !== 'object') continue;
    records[id] = { completions: clamp(Number.isFinite(row.completions) ? Math.floor(row.completions) : 0, 0, 99999),
      bestMs: Number.isFinite(row.bestMs) && row.bestMs > 0 && row.bestMs <= 86400000 ? row.bestMs : null };
  }
  return records;
}
export function normalizeCatalog(races = {}, karate = {}) {
  const courses = structuredClone(races.courses || []); const tiers = structuredClone(karate.tiers || []);
  if (!Array.isArray(courses) || courses.length > 256 || !Array.isArray(tiers) || tiers.length > 32) throw new Error('Activity catalog is too large.');
  const ids = new Set();
  for (const course of courses) {
    course.rewardRings ??= 25; course.bounds ??= { minX: -22, maxX: 22, minZ: -22, maxZ: 22 }; course.checkpoints ??= [];
    if (!validId(course.id) || ids.has(course.id) || !course.name || course.rewardRings > 1000 || course.rewardRings < 0) throw new Error('Invalid race metadata.');
    ids.add(course.id); const b = course.bounds;
    if (![b.minX,b.maxX,b.minZ,b.maxZ].every(v => Number.isFinite(v) && Math.abs(v) <= 10000) || b.minX >= b.maxX || b.minZ >= b.maxZ) throw new Error('Invalid race bounds.');
    const inside = (x,z) => Number.isFinite(x) && Number.isFinite(z) && x >= b.minX && x <= b.maxX && z >= b.minZ && z <= b.maxZ;
    if (!Array.isArray(course.checkpoints) || course.checkpoints.length > 2048 || course.checkpoints.some(g => !inside(g.x,g.z) || !Number.isFinite(g.radius ?? 0.8) || (g.radius ?? 0.8) < 0.1 || (g.radius ?? 0.8) > 5)) throw new Error('Invalid race gates.');
    course.checkpoints.forEach(g => { g.radius ??= 0.8; });
    if (course.available && (!course.geometryValidated || course.checkpoints.length < 2 || !course.spawn || !inside(course.spawn.x,course.spawn.z))) throw new Error('Available native races require validated gates/spawn.');
  }
  ids.clear();
  for (const tier of tiers) {
    tier.rewardRings ??= 50;
    if (!validId(tier.id) || ids.has(tier.id) || !tier.name || !Array.isArray(tier.opponents) || !tier.opponents.length || tier.opponents.length > 32 || tier.rewardRings < 0 || tier.rewardRings > 1000) throw new Error('Invalid karate tier.');
    ids.add(tier.id);
    for (const opponent of tier.opponents) {
      if (!validId(opponent.id) || !opponent.name || !Array.isArray(opponent.skills) || opponent.skills.length < 7 || opponent.skills.length > 8 || !opponent.skills.every(v => Number.isFinite(v) && v >= 0 && v <= 9999)) throw new Error('Invalid native karate opponent.');
    }
  }
  return { raceCourses: courses, karateTiers: tiers, rules: 'Adapted browser controls and combat; original catalog/opponent data. Native engine routines are not ported.' };
}
export function nativeCourse(game,player){return player.race?.courseId?game.activities.raceCourses.find(c=>c.id===player.race.courseId):null;}
export function publicActivities(game, detailed = false) {
  const available = c => Boolean(c.available && c.geometryValidated && game.activityTerrains.has(c.worldId || c.id));
  return {
    raceCourses: game.activities.raceCourses.map(c => detailed ? { ...c, available: available(c) }
      : { id:c.id, name:c.name, group:c.group, level:c.level ?? null, model:c.model ?? null, worldId:c.worldId ?? null, available:available(c), gaps:c.gaps || [] }),
    karateTiers: detailed ? game.activities.karateTiers : game.activities.karateTiers.map(t => ({ id:t.id, name:t.name, rounds:t.opponents.length,
      opponents:t.opponents.map(o => ({id:o.id, name:o.name, nativeIndex:o.nativeIndex || 0})) })),
    rules: game.activities.rules,
  };
}
export function enterRace(game,player,message){
  if(player.race)return game.notice(player.id,'Finish or leave your current race first.','error');
  if(player.vehicle)return game.notice(player.id,'Step out of your vehicle before entering a native Chao race.','error');
  const course=game.activities.raceCourses.find(c=>c.id===message.courseId);
  if(!course)return game.notice(player.id,'Unknown native race course.','error');
  const field=game.activityTerrains.get(course.worldId||course.id);const y=course.spawn&&field?game.sampleField(field,course.spawn.x,course.spawn.z):null;
  if(!course.available||!course.geometryValidated||y===null)return game.notice(player.id,'This original course is catalogued; its route and collision still need validation.','error');
  game.teleport(player,'race',0,12);Object.assign(player,{x:course.spawn.x,z:course.spawn.z,y,groundY:y,activityResult:null});
  player.race={startedAt:game.time+3000,nextCheckpoint:0,courseId:course.id};
}
export function tickNativeRace(game,player){
  const course=nativeCourse(game,player);if(!course||game.time<player.race.startedAt)return;
  const race=player.race;const gate=course.checkpoints[race.nextCheckpoint];if(!gate)return;
  const y=game.playerGround(player,gate.x,gate.z);if(Math.hypot(player.x-gate.x,player.z-gate.z)>gate.radius||y===null||Math.abs(player.y-y)>1)return;
  race.nextCheckpoint++;if(race.nextCheckpoint<course.checkpoints.length)return;
  const elapsed=Math.round(game.time-race.startedAt);const record=player.raceRecords[course.id]??={completions:0,bestMs:null};record.completions=Math.min(99999,record.completions+1);record.bestMs=record.bestMs===null?elapsed:Math.min(record.bestMs,elapsed);
  player.racesCompleted++;player.rings+=course.rewardRings;game.gain(player,'run',1);player.happiness=Math.min(100,player.happiness+8);
  player.activityResult={kind:'race',courseId:course.id,name:course.name,native:true,elapsedMs:elapsed,rings:course.rewardRings,rules:'adapted'};
  game.teleport(player,'hub');game.dirty=true;
}
const skill = (opponent,index) => clamp((opponent.skills[index] || 0) / 2500, 0, 1);
const opponentOf = bout => bout.tier.opponents[bout.opponentIndex];
function resetRound(bout,player,now) {
  Object.assign(bout, { phase:'fighting', roundStartedAt:now, endsAt:now+60000,
    playerMaxHp:100+player.stats.stamina.level/99*60, playerStamina:100,
    opponentMaxHp:85+skill(opponentOf(bout),4)*60, opponentStamina:100,
    opponentX:0,opponentZ:2.7,playerAction:'idle',opponentAction:'idle',playerHitAt:null,opponentHitAt:null,
    playerUntil:0,opponentUntil:0,nextAiAt:now+800 });
  bout.playerHp=bout.playerMaxHp; bout.opponentHp=bout.opponentMaxHp;
}
export function publicKarate(bout) {
  if (!bout) return null;
  const opponent=opponentOf(bout);
  return { id:bout.id,tierId:bout.tier.id,tierName:bout.tier.name,round:bout.opponentIndex+1,rounds:bout.tier.opponents.length,phase:bout.phase,
    startedAt:bout.startedAt,roundEndsAt:bout.endsAt,playerHp:bout.playerHp,playerMaxHp:bout.playerMaxHp,playerStamina:bout.playerStamina,playerAction:bout.playerAction,playerActionUntil:bout.playerUntil,
    opponent:{ id:opponent.id,name:opponent.name,nativeIndex:opponent.nativeIndex || 0,appearance:opponent.appearance || null,x:bout.opponentX,y:.75,z:bout.opponentZ,
      hp:bout.opponentHp,maxHp:bout.opponentMaxHp,stamina:bout.opponentStamina,action:bout.opponentAction,actionUntil:bout.opponentUntil,hitAt:bout.opponentHitAt } };
}
export function karateAction(game,player,message) {
  const action=message.action;
  if (action==='karate-leave') return game.teleport(player,'hub');
  if (action==='karate-enter') {
    if (player.vehicle) return game.notice(player.id,'Step out of your vehicle before entering Chao Karate.','error');
    if (player.karate) return game.notice(player.id,'Finish or leave your current tournament first.','error');
    const tier=game.activities.karateTiers.find(tier=>tier.id===message.tier);
    if (!tier) return game.notice(player.id,'That native karate tier is not available.','error');
    game.teleport(player,'karate',0,.8);player.facing=0;player.activityResult=null;
    const bout={id:randomUUID(),tier:structuredClone(tier),opponentIndex:0,startedAt:game.time,decision:0};resetRound(bout,player,game.time);player.karate=bout;return;
  }
  const bout=player.karate;
  if (!bout) return game.notice(player.id,'Enter a Chao Karate tournament first.','error');
  if (action==='karate-next'&&bout.phase==='round-won') {
    bout.opponentIndex++;game.teleport(player,'karate',0,.8);player.facing=0;resetRound(bout,player,game.time);return;
  }
  if (bout.phase!=='fighting'||game.time<bout.playerUntil) return;
  if (action==='karate-attack') {
    const kick=message.move==='kick';const cost=kick?20:10;if(bout.playerStamina<cost)return;
    bout.playerStamina-=cost;bout.playerAction=kick?'kick':'punch';const windup=kick?320:180;
    bout.playerHitAt=game.time+windup;bout.playerUntil=game.time+windup+(kick?650:400);player.facing=Math.atan2(bout.opponentX-player.x,bout.opponentZ-player.z);
  } else if(action==='karate-block'&&bout.playerStamina>=6){bout.playerStamina-=6;bout.playerAction='block';bout.playerUntil=game.time+850;}
  else if(action==='karate-dodge'&&bout.playerStamina>=18&&game.cooldown(player,'karate-dodge',1100)){bout.playerStamina-=18;bout.playerAction='dodge';bout.playerUntil=game.time+450;player.x=clamp(player.x+(message.direction==='right'?0.65:-0.65),-1.35,1.35);}
  else if(action==='karate-focus'){bout.playerAction='focus';bout.playerUntil=game.time+900;}
}
function emitHit(game,player,bout,target,damage){game.events.push({target:player.id,message:{type:'event',kind:'karate-hit',boutId:bout.id,target,damage}});}
export function tickKarate(game,player,dt) {
  const bout=player.karate;if(!bout||player.zone!=='karate'||bout.phase!=='fighting')return;
  if(game.time>=bout.playerUntil)bout.playerAction='idle';if(game.time>=bout.opponentUntil)bout.opponentAction='idle';
  const regen=bout.playerAction==='focus'?32:['idle','recover'].includes(bout.playerAction)?9:0;
  bout.playerStamina=Math.min(100,bout.playerStamina+regen*dt);bout.opponentStamina=Math.min(100,bout.opponentStamina+dt*10);
  const opponent=opponentOf(bout);const separation=Math.hypot(player.x-bout.opponentX,player.z-bout.opponentZ);
  if(bout.opponentAction==='idle'&&separation>1.05){const speed=.75+skill(opponent,2)*.6;bout.opponentX+=(player.x-bout.opponentX)/separation*speed*dt;bout.opponentZ+=(player.z-bout.opponentZ)/separation*speed*dt;}
  if(bout.playerHitAt!==null&&game.time>=bout.playerHitAt){bout.playerHitAt=null;const kick=bout.playerAction==='kick';bout.playerAction='recover';
    if(separation<=1.5){const damage=(kick?16:10)+player.stats.power.level/99*(kick?15:12);bout.opponentHp=Math.max(0,bout.opponentHp-damage);emitHit(game,player,bout,'opponent',damage);}}
  if(bout.opponentHitAt!==null&&game.time>=bout.opponentHitAt){bout.opponentHitAt=null;const kick=bout.opponentAction==='kick';bout.opponentAction='recover';
    if(separation<=1.5&&bout.playerAction!=='dodge'){let damage=(kick?14:9)+skill(opponent,3)*(kick?14:12);if(bout.playerAction==='block'&&bout.playerStamina>=8){damage*=.25;bout.playerStamina-=8;}
      bout.playerHp=Math.max(0,bout.playerHp-damage);emitHit(game,player,bout,'player',damage);}}
  if(bout.opponentAction==='idle'&&game.time>=bout.nextAiAt&&separation<=1.5){bout.decision++;const kick=bout.decision%3===0;const cost=kick?20:10;
    if(bout.opponentStamina>=cost){bout.opponentStamina-=cost;bout.opponentAction=kick?'kick':'punch';const windup=650-skill(opponent,6)*250+(kick?150:0);bout.opponentHitAt=game.time+windup;bout.opponentUntil=game.time+windup+650;bout.nextAiAt=bout.opponentUntil+200;}}
  const timedOut=game.time>=bout.endsAt;
  if(bout.playerHp<=0||bout.opponentHp<=0||timedOut){
    const won=bout.opponentHp<=0||(timedOut&&bout.playerHp/bout.playerMaxHp>bout.opponentHp/bout.opponentMaxHp);
    if(won&&bout.opponentIndex+1<bout.tier.opponents.length){bout.phase='round-won';}
    else{bout.phase=won?'won':'lost';const elapsed=Math.round(game.time-bout.startedAt);
      if(won){const record=player.karateRecords[bout.tier.id]??={completions:0,bestMs:null};record.completions=Math.min(99999,record.completions+1);record.bestMs=record.bestMs===null?elapsed:Math.min(record.bestMs,elapsed);
        player.rings+=bout.tier.rewardRings;game.gain(player,'power',2);game.gain(player,'stamina',1);player.happiness=Math.min(100,player.happiness+8);game.dirty=true;}
      player.activityResult={kind:'karate',id:bout.id,tierId:bout.tier.id,won,elapsedMs:elapsed,rings:won?bout.tier.rewardRings:0,rules:'adapted'};
    }
  }
}
