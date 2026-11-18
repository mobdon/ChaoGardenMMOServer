import test from 'node:test';
import assert from 'node:assert/strict';
import { Game } from '../../game.js';
import { normalizeCatalog } from '../../activities.js';

const race = { id:'synthetic-course', name:'Synthetic validated course', group:'test', model:'race-normal', worldId:'test-race',
  available:true, geometryValidated:true, bounds:{minX:30,maxX:35,minZ:0,maxZ:1}, spawn:{x:32,z:.5},
  checkpoints:[{x:30.5,z:.5,radius:.2},{x:33.5,z:.5,radius:.2},{x:31.5,z:.5,radius:.2}], rewardRings:25 };
const field = { minX:30,minZ:0,step:.25,width:21,depth:5,heights:Array(105).fill(2) };
const tier = {id:'test-tier',name:'Synthetic two-opponent tier',opponents:[0,1].map(i=>({id:`cpu-${i}`,name:`Opponent ${i+1}`,nativeIndex:i,skills:Array(8).fill(1),appearance:{type:2}})),rewardRings:50};
function fixture({terrain=true,owner=false,...extra}={}) {
  const game=new Game({ now:()=>1000, allowOwnerBootstrap:owner, raceCatalog:{courses:[race]}, karateCatalog:{tiers:[tier]}, activityTerrains:terrain?{'test-race':field}:{},...extra });
  const session=game.createSession({name:'Activity test'});game.connect(session.player.id);
  return {game,player:game.players.get(session.player.id),session};
}
function advance(game,ms,input) { for(let i=0;i<Math.ceil(ms/50);i++){if(input)game.setInput(input.id,input);game.tick(.05);} }
function act(game,player,action,fields={}) { advance(game,100);game.action(player.id,{action,...fields}); }
function moveUntil(game,player,dx,predicate) { for(let i=0;i<200&&!predicate();i++)advance(game,50,{id:player.id,dx,dz:0});assert.ok(predicate(),'physical movement reached expected gate'); }

test('native catalog rejects invalid enabled routes; missing collision never becomes available',()=>{
  assert.throws(()=>normalizeCatalog({courses:[{...race,geometryValidated:false}]}),/validated/);
  assert.throws(()=>normalizeCatalog({courses:[{...race,checkpoints:[]}]}),/validated/);
  const {game,player}=fixture({terrain:false});assert.equal(game.snapshot(player.id).activities.raceCourses[0].available,false);
  act(game,player,'race-enter',{courseId:race.id,available:true,geometryValidated:true});assert.equal(player.zone,'hub');assert.equal(player.race,null);
  const disabled=fixture({raceCatalog:{courses:[{...race,available:false,geometryValidated:false,spawn:null,checkpoints:[]}]}});
  act(disabled.game,disabled.player,'race-enter',{courseId:race.id});assert.equal(disabled.player.zone,'hub');
});

test('native races retain independent bounds, count down, require physical ordered gates and pay/persist once',()=>{
  const {game,player,session}=fixture();const rings=player.rings;
  act(game,player,'race-enter',{courseId:race.id});assert.equal(player.x,32);assert.equal(player.y,2);
  assert.equal(game.snapshot(player.id).race.phase,'countdown');
  advance(game,1000,{id:player.id,dx:1,dz:0});assert.equal(player.x,32);assert.equal(player.race.nextCheckpoint,0);
  advance(game,2000);advance(game,1000,{id:player.id,dx:1,dz:0});assert.ok(player.x>33);assert.equal(player.race.nextCheckpoint,0);
  act(game,player,'race-result',{courseId:race.id,elapsedMs:1,rings:9999});assert.equal(player.rings,rings);
  moveUntil(game,player,-1,()=>player.race.nextCheckpoint===1);
  moveUntil(game,player,1,()=>player.race.nextCheckpoint===2);
  moveUntil(game,player,-1,()=>player.race===null);
  assert.equal(player.zone,'hub');assert.equal(player.rings,rings+25);assert.equal(player.raceRecords[race.id].completions,1);
  assert.ok(player.raceRecords[race.id].bestMs>5000);assert.equal(player.activityResult.kind,'race');
  advance(game,2000);assert.equal(player.rings,rings+25);
  const restored=new Game({profiles:game.exportProfiles()});assert.deepEqual(restored.playerForToken(session.token).raceRecords,player.raceRecords);assert.equal(restored.playerForToken(session.token).race,null);
});

test('native races and karate cancel only after last disconnect, and vehicles cannot enter',()=>{
  const {game,player}=fixture({owner:true});act(game,player,'mount-dark-walker');
  act(game,player,'race-enter',{courseId:race.id});assert.equal(player.race,null);
  act(game,player,'karate-enter',{tier:tier.id});assert.equal(player.karate,null);
  act(game,player,'dismount-vehicle');act(game,player,'race-enter',{courseId:race.id});
  act(game,player,'mount-dark-walker');assert.equal(player.vehicle,null);
  game.connect(player.id);game.disconnect(player.id);assert.ok(player.race);game.disconnect(player.id);assert.equal(player.race,null);assert.equal(player.zone,'hub');
  game.connect(player.id);act(game,player,'karate-enter',{tier:tier.id});game.disconnect(player.id);assert.equal(player.karate,null);
});

test('karate enforces actual ring floor, windup, range, recovery and server damage',()=>{
  const {game,player}=fixture();act(game,player,'karate-enter',{tier:tier.id});
  assert.equal(player.y,.75);assert.equal(player.z,.8);assert.equal(game.snapshot(player.id).karate.opponent.y,.75);
  const b=player.karate;const initial=b.opponentHp;b.opponentAction='recover';b.opponentUntil=game.time+10000;
  act(game,player,'karate-attack',{move:'punch',damage:9999,winner:player.id});advance(game,150);assert.equal(b.opponentHp,initial);
  advance(game,100);assert.equal(b.opponentHp,initial,'out-of-range punch misses');
  advance(game,400);player.z=1.5;
  act(game,player,'karate-attack',{move:'punch'});const stamina=b.playerStamina;
  advance(game,150);assert.equal(b.opponentHp,initial);advance(game,50);assert.ok(b.opponentHp<initial&&b.opponentHp>initial-11);
  act(game,player,'karate-attack',{move:'kick'});assert.equal(b.playerAction,'recover');assert.ok(b.playerStamina>=stamina);
  act(game,player,'karate-next');assert.equal(b.opponentIndex,0);
  act(game,player,'mount-dark-walker');assert.equal(player.vehicle,null);
  advance(game,2000,{id:player.id,dx:1,dz:-1});assert.ok(player.x<=1.35&&player.z>=.4);
});

test('karate guard reduces damage, dodge avoids a telegraphed hit and focus restores stamina',()=>{
  for(const defense of ['karate-block','karate-dodge']) {
    const {game,player}=fixture();act(game,player,'karate-enter',{tier:tier.id});player.z=1.5;
    const b=player.karate;const hp=b.playerHp;b.opponentAction='punch';b.opponentHitAt=game.time+300;b.opponentUntil=game.time+1000;
    act(game,player,defense,{direction:'right'});advance(game,300);
    const damage=hp-b.playerHp;if(defense==='karate-block')assert.ok(damage>2&&damage<3);else assert.equal(damage,0);
    assert.ok(b.playerStamina<100);
  }
  const {game,player}=fixture();act(game,player,'karate-enter',{tier:tier.id});player.karate.playerStamina=25;
  act(game,player,'karate-focus');const before=player.karate.playerStamina;advance(game,500);assert.ok(player.karate.playerStamina>=before+15);
});

test('karate requires real round wins and awards final tournament once with persisted records',()=>{
  const {game,player,session}=fixture();const rings=player.rings;
  act(game,player,'karate-enter',{tier:tier.id});act(game,player,'karate-result',{won:true,damage:9999,rings:9999});assert.equal(player.rings,rings);
  for(let round=0;round<2;round++) {
    advance(game,1500);
    for(let attempt=0;attempt<30&&player.karate.phase==='fighting';attempt++){act(game,player,'karate-attack',{move:'punch'});advance(game,550);}
    assert.equal(player.karate.phase,round===0?'round-won':'won');
    if(round===0){assert.equal(player.rings,rings);act(game,player,'karate-next');assert.equal(player.karate.opponentIndex,1);assert.equal(player.y,.75);}
  }
  assert.equal(player.rings,rings+50);assert.equal(player.karateRecords[tier.id].completions,1);assert.equal(player.activityResult.won,true);
  for(let i=0;i<10;i++)act(game,player,'karate-next');assert.equal(player.rings,rings+50);
  const restored=new Game({profiles:game.exportProfiles()});assert.deepEqual(restored.playerForToken(session.token).karateRecords,player.karateRecords);assert.equal(restored.playerForToken(session.token).karate,null);
  act(game,player,'karate-leave');assert.equal(player.zone,'hub');assert.equal(player.karate,null);
});

test('karate timeout loss and depleted stamina cannot be forged into a win',()=>{
  const {game,player}=fixture();act(game,player,'karate-enter',{tier:tier.id});const b=player.karate;
  b.playerStamina=0;act(game,player,'karate-attack',{move:'kick',stamina:100});assert.equal(b.playerAction,'idle');
  b.playerHp=1;b.endsAt=game.time+50;advance(game,50);assert.equal(b.phase,'lost');assert.equal(player.rings,20);assert.deepEqual(Object.keys(player.karateRecords),[]);
});
