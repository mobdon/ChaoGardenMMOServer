// Custom resident routine, separate from the retail Chao AI engine.
import { randomUUID } from 'node:crypto';
const clamp=(value,min,max)=>Math.max(min,Math.min(max,value));
const emptyInput=()=>({dx:0,dz:0,jump:false,boost:false});
export function publicWatch(watch,ownerOnline){return watch?{id:watch.id,mode:'watch',activity:watch.activity,startedAt:watch.startedAt,activityStartedAt:watch.activityStartedAt,activityUntil:watch.activityUntil,target:watch.target,lastCare:watch.lastCare,resident:true,ownerOnline,rules:'adapted-resident'}:null;}
function createWatch(player,now){return {id:randomUUID(),activity:'idle',startedAt:now,activityStartedAt:now,activityUntil:now+1800,target:null,anchor:{x:player.x,z:player.z},decision:0,salt:Number.parseInt(randomUUID().replaceAll('-','').slice(0,8),16),lastCare:null,nextCheckpointAt:now+15000};}
export function restoreWatch(game,player,value){
  if(value?.enabled!==true)return;
  const zone=value.zone==='hero'&&player.alignment>=60?'hero':value.zone==='dark'&&player.alignment<=-60?'dark':'garden';
  const coordinate=(v,fallback)=>Number.isFinite(v)&&Math.abs(v)<=22?v:fallback;
  game.teleport(player,zone,coordinate(value.x,0),coordinate(value.z,6));player.vehicle=null;player.watch=createWatch(player,game.time);
}
function state(watch,activity,now,duration){Object.assign(watch,{activity,activityStartedAt:now,activityUntil:now+duration});if(activity!=='wander')watch.target=null;}
export function watchAction(game,player,message){
  const action=message.action;
  const allowed=action==='watch-start'?['type','action']:action==='watch-stop'?['type','action','sessionId']:['type','action','sessionId','kind'];
  if(Object.keys(message).some(key=>!allowed.includes(key)))return game.notice(player.id,'Invalid watch intent.','error');
  if(action==='watch-start'){
    if(player.watch)return;
    if(player.race||player.karate||!['hub','garden','hero','dark'].includes(player.zone))return game.notice(player.id,'Finish your current activity before watching your Chao.','error');
    if(player.vehicle){player.vehicle=null;game.dirty=true;}
    if(player.zone==='hub')game.teleport(player,'garden');
    player.input=emptyInput();player.vy=0;player.jumpOffset=0;player.y=player.groundY;player.boosting=false;
    player.watch=createWatch(player,game.time);game.dirty=true;return;
  }
  const watch=player.watch;if(!watch||message.sessionId!==watch.id)return;
  if(action==='watch-stop'){player.watch=null;player.input=emptyInput();game.dirty=true;return;}
  if(action!=='watch-care')return;
  if(!['pet','rest','wake'].includes(message.kind))return game.notice(player.id,'Unknown care interaction.','error');
  if(!game.cooldown(player,'watch-care',1500))return;
  watch.lastCare={kind:message.kind,at:game.time};
  if(message.kind==='pet'){player.happiness=Math.min(100,player.happiness+2);game.dirty=true;state(watch,'idle',game.time,1800);}
  else state(watch,message.kind==='rest'?'rest':'idle',game.time,message.kind==='rest'?5000:1800);
  player.input=emptyInput();
}
function target(game,player,watch){
  for(let attempt=0;attempt<8;attempt++){
    const step=watch.decision+attempt;const angle=(watch.salt%4096)/4096*Math.PI*2+step*2.3999632297;
    const radius=1+((watch.salt+step)%5)*.36;
    const x=clamp(watch.anchor.x+Math.sin(angle)*radius,-21,21);const z=clamp(watch.anchor.z+Math.cos(angle)*radius,-21,21);
    const count=Math.max(1,Math.ceil(Math.hypot(player.x-x,player.z-z)/.2));let previous=game.playerGround(player,player.x,player.z);if(previous===null)return null;
    let safe=true;
    for(let i=1;i<=count;i++){const t=i/count;const y=game.playerGround(player,player.x+(x-player.x)*t,player.z+(z-player.z)*t);if(y===null||Math.abs(y-previous)>.15){safe=false;break;}previous=y;}
    if(safe)return {x,z};
  }
  return null;
}
export function tickWatch(game,player){
  const watch=player.watch;if(!watch)return;
  if(game.time>=watch.nextCheckpointAt){game.dirty=true;watch.nextCheckpointAt=game.time+15000;}
  if(game.time>=watch.activityUntil){
    if(watch.activity==='idle'){
      watch.decision=(watch.decision+1)>>>0;
      if(watch.decision%3===0)state(watch,'rest',game.time,5000);
      else{const destination=target(game,player,watch);if(destination){watch.target=destination;state(watch,'wander',game.time,5500);}else state(watch,'idle',game.time,1800);}
    }else state(watch,'idle',game.time,1800);
  }
  player.input=emptyInput();
  if(watch.target){const distance=Math.hypot(watch.target.x-player.x,watch.target.z-player.z);if(distance<=.15)state(watch,'idle',game.time,1800);else{player.input.dx=(watch.target.x-player.x)/distance*.28;player.input.dz=(watch.target.z-player.z)/distance*.28;}}
}
