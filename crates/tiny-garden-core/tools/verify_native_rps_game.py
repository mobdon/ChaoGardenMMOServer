#!/usr/bin/env python3
"""Original ASOE active Janken callback parity with explicit reference BIOS.

Unicorn executes the owner's original Thumb; SWI0A uses independently compiled
pinned mGBA numerical C. This is not authentic BIOS machine-code parity.
Local payload/reference executable stay private; output must be new in.local.
"""
from __future__ import annotations
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import time
sys.dont_write_bytecode=True
from verify_native_rps import RpsOracle,LENGTH,SHA1,SHA256,SEED,TARGETS

FIELDS=175
INPUT_LEN=184
OUTPUT_LEN=194
BIOS_EXE_SHA256="5482575dfb1849c2672144e7c5b13737cb1f920608dc76d03f509f9d5d48fb97"
BIOS_SOURCE_SHA256="8885962e1ca2d8c79a7f13a5d1e074bb2b39704a64aa164f76e1506868c8759a"
CARD=127
ACTOR=169


def initial(generator):
    row=bytearray(generator.randbytes(FIELDS));row[:2]=bytes([30,0])
    for index in range(10):
        offset=2+index*12;row[offset]=4
        struct.pack_into("<hhh",row,offset+4,-32*index,10,8)
    row[122:127]=bytes([0,2,10,10,0])
    for index in range(3):
        offset=CARD+index*12;row[offset]=0;row[offset+2]=index
        struct.pack_into("<hh",row,offset+4,index*64+12,116)
    row[163:169]=bytes([5,5,0,0,1,76])
    struct.pack_into("<hHB",row,ACTOR,80,60,2);row[174]=0
    return row


def cases(random_count):
    generator=random.Random(0xA5_0E_AC_CC)
    for seed in [0,1,0x7FFFFFFF,0x80000000,0xFFFFFFFF]:
        yield bytes(initial(generator)),seed,0,0,1
    row=initial(generator);row[2]=0;row[CARD:CARD+2]=bytes([8,10])
    yield bytes(row),858717501,3,0,1 # Native one-past initialized scratch read.
    for lane,marker,keys,state in itertools.product(range(3),[0,10,12,14,28,44,75,76,77,108,140,156,255],[0,1,16,17,32,33,48,49],[0,1,4,9]):
        row=initial(generator);row[167:169]=bytes([lane,marker]);row[CARD+12*lane]=state
        yield bytes(row),1,1,keys,3
    for shots,display,fraction,counter in itertools.product([0,1,2,5,255],[0,1,5,127,128,255],[0,1,11,12,128,255],[0,1,2,255]):
        row=initial(generator);row[163:167]=bytes([shots,display,fraction,counter])
        yield bytes(row),1,2,0,25
    for state,counter in itertools.product(range(10),[0,9,10,19,20,29,30,31,127,128,255]):
        row=initial(generator);row[CARD:CARD+2]=bytes([state,counter]);struct.pack_into("<h",row,CARD+6,-32)
        yield bytes(row),123,3,0,65
    for card_gesture,target_gesture,dx,dy in itertools.product(range(3),range(3),[-25,-24,-23,-1,0,1,23,24,25],[-26,-25,-24,-1,0,1,20,21,22]):
        row=initial(generator);row[CARD]=1;row[CARD+2]=card_gesture;struct.pack_into("<h",row,CARD+6,80)
        row[2]=0;row[4]=target_gesture;struct.pack_into("<hhh",row,6,0,12+dx,76+dy)
        yield bytes(row),123,3,0,1
    for mode,timer,x in itertools.product([0,1,2,3,4,5,255],[0,1,2,60,32768,65535],[-32768,44,80,115,32767]):
        row=initial(generator);struct.pack_into("<hHB",row,ACTOR,x,timer,mode)
        yield bytes(row),123,4,0,3
    for seed,keys in itertools.product(range(16),[0,1,16,17,32,33,48,49]):
        yield bytes(initial(generator)),seed,7,keys,2400
    for seed,keys in itertools.product(range(8),[0,1,17,33]):
        yield bytes(initial(generator)),seed,8,keys,3000
    for _ in range(random_count):
        row=initial(generator)
        for index in range(10):
            start=2+index*12;row[start]=generator.choice([0,0,2,3,4]);row[start+1]=generator.randrange(256);row[start+2]=generator.randrange(3)
            struct.pack_into("<hhh",row,start+4,generator.randrange(-320,500),generator.randrange(-35,250),generator.randrange(-35,170))
            row[start+10:start+12]=generator.randbytes(2)
        row[122:127]=bytes([generator.randrange(256),generator.choice([2,4]),generator.randrange(1,11),generator.randrange(11),generator.randrange(100)])
        row[:2]=bytes([generator.randrange(100),generator.randrange(60)])
        for index in range(3):
            start=CARD+index*12;row[start]=generator.randrange(10);row[start+1]=generator.randrange(256);row[start+2]=generator.randrange(3)
            struct.pack_into("<hh",row,start+4,generator.randrange(-35,250),generator.randrange(-35,170));row[start+8:start+10]=generator.randbytes(2)
        row[163:169]=bytes([generator.randrange(1,6),generator.randrange(6),generator.randrange(12),generator.randrange(2),generator.randrange(3),generator.randrange(256)])
        struct.pack_into("<hHB",row,ACTOR,generator.randrange(44,116),generator.randrange(1,181),generator.randrange(6))
        yield bytes(row),generator.getrandbits(32),generator.choice([1,2,3,3,3,4,5]),generator.choice([0,1,16,17,32,33,48,49]),generator.randrange(1,65)


def compile_harness(core,output,rustc):
    text='#![allow(dead_code)]\nmod gba{pub fn sha256(_: &[u8])->String{unreachable!("source gate tested separately")}}\n'
    for name in ["native_growth","native_rps","native_bios_math","native_rps_game","native_rps_finish"]:
        path=str((core/"src"/f"{name}.rs").resolve()).replace("\\","/")
        if '"#'in path:raise ValueError("unsupported path delimiter")
        text+=f'#[path=r#"{path}"#]mod {name};\n'
    text+='''use native_growth::NativeRng;
use native_rps::*;use native_rps_game::*;use native_rps_finish::*;
use std::io::{self,Read,Write};
fn main(){let mut data=Vec::new();io::stdin().read_to_end(&mut data).unwrap();
 if data.is_empty(){let mut out=Vec::new();for x in -32_i16..=32{for y in -32_i16..=32{out.extend(ReferenceBiosAngle.atan2(x,y).unwrap().to_le_bytes());}}for value in 0_u32..256{out.extend(ReferenceBiosAngle.sqrt(value).unwrap().to_le_bytes());}for value in 0_u32..256{out.extend(ReferenceBiosAngle.sqrt(value.wrapping_mul(0x01010101)).unwrap().to_le_bytes());}io::stdout().write_all(&out).unwrap();return;}
 assert_eq!((data.len()-175)%184,0);
 let assets=NativeRpsAssets{trig:std::array::from_fn(|i|i16::from_le_bytes(data[2*i..2*i+2].try_into().unwrap())),hitbox:std::array::from_fn(|i|data[128+i]as i8)};
 let mut output=Vec::new();for row in data[175..].as_chunks::<184>().0{
  let waves=NativeRpsWaves{clock:NativeRpsClock{seconds:row[0],subframes:row[1]},targets:std::array::from_fn(|i|NativeRpsTarget{raw:row[2+i*12..14+i*12].try_into().unwrap()}),wave:row[122],route_speed:row[123],active_targets:row[124],unhit_targets:row[125],round_rings:row[126]};
  let launcher=NativeRpsLauncher{cards:std::array::from_fn(|i|NativeRpsCard{raw:row[127+i*12..139+i*12].try_into().unwrap()}),shots:row[163],displayed_shots:row[164],display_fraction:row[165],display_counter:row[166],lane:row[167],marker_x:row[168]};
  let actor=NativeRpsActor{x:i16::from_le_bytes(row[169..171].try_into().unwrap()),countdown:u16::from_le_bytes(row[171..173].try_into().unwrap()),mode:row[173]};
  let mut state=NativeRpsRound{waves,launcher,actor,finishing:row[174]!=0};let mut rng=NativeRng::new(u32::from_le_bytes(row[175..179].try_into().unwrap()));let op=row[179];let keys=u16::from_le_bytes(row[180..182].try_into().unwrap());let ticks=u16::from_le_bytes(row[182..184].try_into().unwrap());let mut calls=0_u32;let mut sounds=[0_u16;5];let mut error=0_u8;
  let mut session=NativeRpsSession{round:state.clone(),..NativeRpsSession::default()};
  if op==7{state.initialize(&mut rng);calls+=13;}
  if op==8{session.initialize(&mut rng);calls+=13;state=session.round.clone();}
  for _ in 0..ticks{match op{
   0=>{state.initialize(&mut rng);calls+=13;},
   1=>sounds[0]+=state.launcher.input_tick(keys).unwrap()as u16,
   2=>state.launcher.tick_shot_display(),
   3=>match state.launcher.tick_projectiles(&mut state.waves,&assets,&ReferenceBiosAngle,&mut rng){Ok(e)=>{calls+=u32::from(e.random_calls);sounds[2]+=e.won.count_ones()as u16;sounds[3]+=e.reflected.count_ones()as u16;sounds[4]+=e.tied.count_ones()as u16;},Err(Error::NativeScratchRead{..})=>{error=1;break;},Err(e)=>panic!("{e:?}")},
   4=>calls+=u32::from(state.actor.tick(&mut rng).unwrap().random_calls),
   5|7=>match state.tick_active(keys,&assets,&ReferenceBiosAngle,&mut rng){Ok(e)=>{calls+=u32::from(e.actor.random_calls)+u32::from(e.targets.random_calls)+u32::from(e.projectiles.random_calls);sounds[0]+=e.targets.appeared.count_ones()as u16+e.fired as u16;sounds[1]+=e.targets.defeated_removed.count_ones()as u16;sounds[2]+=e.projectiles.won.count_ones()as u16;sounds[3]+=e.projectiles.reflected.count_ones()as u16;sounds[4]+=e.projectiles.tied.count_ones()as u16;},Err(Error::NativeScratchRead{..})=>{error=1;break;},Err(e)=>panic!("{e:?}")},
   6=>{let e=state.waves.tick_targets(&mut rng);calls+=u32::from(e.random_calls);sounds[0]+=e.appeared.count_ones()as u16;sounds[1]+=e.defeated_removed.count_ones()as u16;},
   8=>match session.tick(keys,&assets,&ReferenceBiosAngle,&mut rng){Ok(e)=>{let a=e.active;calls+=u32::from(a.actor.random_calls)+u32::from(a.targets.random_calls)+u32::from(a.projectiles.random_calls)+u32::from(e.finish_targets.random_calls);sounds[0]+=a.targets.appeared.count_ones()as u16+a.fired as u16+e.finish_targets.appeared.count_ones()as u16;sounds[1]+=a.targets.defeated_removed.count_ones()as u16+e.finish_targets.defeated_removed.count_ones()as u16;sounds[2]+=a.projectiles.won.count_ones()as u16;sounds[3]+=a.projectiles.reflected.count_ones()as u16;sounds[4]+=a.projectiles.tied.count_ones()as u16;state=session.round.clone();if e.completed{error=2;break;}},Err(FinishError::Game(Error::NativeScratchRead{..}))=>{error=1;break;},Err(e)=>panic!("{e:?}")},_=>panic!("unknown op")}}
  output.extend([state.waves.clock.seconds,state.waves.clock.subframes]);for t in state.waves.targets{output.extend(t.raw);}output.extend([state.waves.wave,state.waves.route_speed,state.waves.active_targets,state.waves.unhit_targets,state.waves.round_rings]);
  for c in state.launcher.cards{output.extend(c.raw);}output.extend([state.launcher.shots,state.launcher.displayed_shots,state.launcher.display_fraction,state.launcher.display_counter,state.launcher.lane,state.launcher.marker_x]);output.extend(state.actor.x.to_le_bytes());output.extend(state.actor.countdown.to_le_bytes());output.extend([state.actor.mode,state.finishing as u8]);output.extend(rng.state().to_le_bytes());output.extend(calls.to_le_bytes());for s in sounds{output.extend(s.to_le_bytes());}output.push(error);
 }io::stdout().write_all(&output).unwrap();}
'''
    harness=output/"native_rps_game_harness.rs";harness.write_text(text,encoding="utf-8");executable=output/("native_rps_game_harness.exe"if sys.platform=="win32"else"native_rps_game_harness")
    subprocess.run([rustc,"--edition=2024",str(harness),"-o",str(executable)],check=True,timeout=60);return executable


class ScratchRead(Exception):pass


class GameOracle(RpsOracle):
    def __init__(self,payload,bios):
        super().__init__(payload);self.bios=bios;self.bios_calls=0
        self.uc.mem_map(0x04000000,0x1000);self.uc.mem_map(0x06000000,0x20000)
    def code(self,uc,address,size,data):
        arm=self.arm
        if address in [0x0200AB54,0x02008190,0x020082FC,0x02008284,0x02008204,0x02008324,0x0200DB08,0x020080C0,0x0200A6E0,0x0200C828]:
            uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        elif address==0x0200B2F8:
            if uc.reg_read(arm.UC_ARM_REG_R0)>=uc.reg_read(arm.UC_ARM_REG_R5):raise ScratchRead()
        elif address==0x0200C690:
            sound=uc.reg_read(arm.UC_ARM_REG_R0)
            if sound in self.sounds:self.sounds[sound]+=1
            elif sound not in[0x17,0x1F]:raise RuntimeError(f"unexpected sound{sound:#x}")
            uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        else:super().code(uc,address,size,data)
    def bios_div(self,uc,interrupt,data):
        arm=self.arm;pc=uc.reg_read(arm.UC_ARM_REG_PC)
        if interrupt!=2 or bytes(uc.mem_read(pc-2,2))!=b"\x0a\xdf":raise RuntimeError("unexpected native BIOS call")
        x=uc.reg_read(arm.UC_ARM_REG_R0);y=uc.reg_read(arm.UC_ARM_REG_R1)
        if x&0x80000000:x-=1<<32
        if y&0x80000000:y-=1<<32
        if (x,y)not in self.bios:raise RuntimeError(f"unmapped BIOS reference input{x},{y}")
        uc.reg_write(arm.UC_ARM_REG_R0,self.bios[x,y]);self.bios_calls+=1
    def state_read(self):
        u=self.uc
        return bytes(u.mem_read(0x03000E75,2))+bytes(u.mem_read(TARGETS,120))+bytes(u.mem_read(0x03000EF0,4))+bytes(u.mem_read(0x03000DE0,1))+bytes(u.mem_read(0x03000EF8,42))+bytes(u.mem_read(0x03000E70,5))+bytes([self.finishing])
    def state_write(self,row):
        u=self.uc;u.mem_write(0x03000E75,row[:2]);u.mem_write(TARGETS,row[2:122]);u.mem_write(0x03000EF0,row[122:126]);u.mem_write(0x03000DE0,row[126:127]);u.mem_write(0x03000EF8,row[127:169]);u.mem_write(0x03000E70,row[169:174]);self.finishing=bool(row[174])
    def init(self):
        self.uc.mem_write(0x03000DE0,b"\0");self.call(0x0200A4E4,0);self.call(0x0200A8C8,0);self.call(0x0200ABE8,0);self.call(0x0200B3B4,0);self.finishing=False
    def run_game(self,row,seed,op,keys,ticks):
        u=self.uc;u.mem_write(0x03000000,bytes(0x10000));u.mem_write(0x07000000,bytes(0x1000));self.state_write(row);u.mem_write(SEED,struct.pack("<I",seed));u.mem_write(0x03003834,struct.pack("<H",keys));self.rng_calls=0;self.sounds={s:0 for s in[0x1D,0x18,0xC,0xD,0x14]};error=0
        if op==7:self.init()
        if op==8:
            self.init();u.mem_write(0x03000F24,struct.pack("<I",2));u.mem_write(0x03000F23,b"\x01")
        for _ in range(ticks):
            before=self.state_read();before_seed=bytes(u.mem_read(SEED,4));before_calls=self.rng_calls;before_sounds=self.sounds.copy()
            try:
                if op==0:self.init()
                elif op==1:self.call(0x0200B3EC,0)
                elif op==2:self.call(0x0200AB84,0)
                elif op==3:self.call(0x0200ACCC,0)
                elif op==4:self.call(0x0200A528,0)
                elif op in[5,7]:
                    if not self.finishing:
                        self.call(0x0200A528,0);self.call(0x0200A8EC,0);self.call(0x0200ACCC,0);self.call(0x0200B3EC,0)
                        if u.mem_read(0x03000F1C,1)[0]==0:self.finishing=True
                        elif self.call(0x0200A7C0,0)and 0<u.mem_read(0x03000EF3,1)[0]<128:self.finishing=True
                        self.call(0x0200AB84,0)
                elif op==6:self.call(0x0200A8EC,0)
                elif op==8:
                    self.call(0x0200B4E0,0)
                    self.finishing=struct.unpack("<I",bytes(u.mem_read(0x03000F24,4)))[0]>=3
                    if struct.unpack("<I",bytes(u.mem_read(0x03002760,4)))[0]==0x020017FD:
                        self.finishing=True;error=2;break
                else:raise RuntimeError("unknown op")
            except ScratchRead:
                self.state_write(before);u.mem_write(SEED,before_seed);self.rng_calls=before_calls;self.sounds=before_sounds;error=1;break
        result=self.state_read()+bytes(u.mem_read(SEED,4))+struct.pack("<I",self.rng_calls)
        return result+b"".join(struct.pack("<H",self.sounds[s])for s in[0x1D,0x18,0xC,0xD,0x14])+bytes([error])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in["payload","output-dir","bios-reference"]:parser.add_argument("--"+name,required=True,type=Path)
    parser.add_argument("--unicorn-dir",type=Path);parser.add_argument("--rustc",default="rustc");parser.add_argument("--random-cases",type=int,default=2048);args=parser.parse_args()
    payload=args.payload.read_bytes()
    if len(payload)!=LENGTH or hashlib.sha1(payload).hexdigest()!=SHA1 or hashlib.sha256(payload).hexdigest()!=SHA256:raise ValueError("wrong ASOE source identity")
    if hashlib.sha256(args.bios_reference.read_bytes()).hexdigest()!=BIOS_EXE_SHA256:raise ValueError("wrong independent numerical BIOS reference executable")
    if not 1<=args.random_cases<=100_000:parser.error("random-cases must be1..100000")
    output=args.output_dir.resolve()
    if ".local"not in[p.casefold()for p in output.parts]or output.exists():raise ValueError("output must be NEW under.local")
    if args.unicorn_dir:sys.path.insert(0,str(args.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__!="2.1.4":raise ValueError("requires unicorn2.1.4")
    rustc=shutil.which(args.rustc)
    if not rustc:raise ValueError("activate Rust")
    output.mkdir(parents=True,exist_ok=False);core=Path(__file__).resolve().parents[1];exe=compile_harness(core,output,rustc)
    vectors=list(itertools.product(range(-32,33),repeat=2));sqrt_vectors=list(range(256))+[value*0x01010101 for value in range(256)];reference_input="".join(f"A {x} {y}\n"for x,y in vectors)+"".join(f"S {value} 0\n"for value in sqrt_vectors)
    angles=subprocess.run([str(args.bios_reference.resolve())],input=reference_input,text=True,capture_output=True,check=True,timeout=60).stdout.splitlines()
    if len(angles)!=len(vectors)+len(sqrt_vectors):raise RuntimeError("wrong independent BIOS vector count")
    math_expected=b"".join(struct.pack("<H",int(value))for value in angles)
    math_rust=subprocess.run([str(exe)],input=b"",capture_output=True,check=True,timeout=60).stdout
    if math_rust!=math_expected:raise AssertionError("independent BIOS numerical reference mismatch")
    bios=dict(zip(vectors,map(int,angles[:len(vectors)])));corpus=list(cases(args.random_cases));raw=payload[0x11C5E:0x11CDE]+payload[0x11F84:0x11FB3]+b"".join(row+struct.pack("<IBHH",seed,op,keys,ticks)for row,seed,op,keys,ticks in corpus)
    started=time.monotonic();rust=subprocess.run([str(exe)],input=raw,capture_output=True,check=True,timeout=60).stdout
    if len(rust)!=len(corpus)*OUTPUT_LEN:raise RuntimeError(f"wrong Rust output length{len(rust)}")
    oracle=GameOracle(payload,bios);scratch=0;completed=0
    for index,case in enumerate(corpus):
        native=oracle.run_game(*case);actual=rust[index*OUTPUT_LEN:(index+1)*OUTPUT_LEN]
        if native!=actual:
            offsets=[i for i,(a,b)in enumerate(zip(native,actual))if a!=b]
            raise AssertionError(f"case{index} op{case[2]} keys{case[3]} ticks{case[4]} offsets{offsets} native{native.hex()} rust{actual.hex()}")
        scratch+=native[-1]==1;completed+=native[-1]==2
        if index%500==0:print(f"checked{index}/{len(corpus)}",flush=True)
    result={"result":"pass","cases":len(corpus),"random_cases":args.random_cases,"compared_bytes_per_case":OUTPUT_LEN,"payload_sha1":SHA1,"payload_sha256":SHA256,"bios_reference_executable_sha256":BIOS_EXE_SHA256,"bios_reference_primary_source_sha256":BIOS_SOURCE_SHA256,"independently_compared_bios_atan_vectors":len(vectors),"independently_compared_bios_sqrt_vectors":len(sqrt_vectors),"native_reference_bios_calls":oracle.bios_calls,"bounded_uninitialized_scratch_rejections":scratch,"whole_native_controller_completed_rounds":completed,"unicorn_version":unicorn.__version__,"elapsed_seconds":round(time.monotonic()-started,3),
        "rust_source_sha256":hashlib.sha256((core/"src/native_rps_game.rs").read_bytes()).hexdigest(),"finish_source_sha256":hashlib.sha256((core/"src/native_rps_finish.rs").read_bytes()).hexdigest(),"math_source_sha256":hashlib.sha256((core/"src/native_bios_math.rs").read_bytes()).hexdigest(),"wave_source_sha256":hashlib.sha256((core/"src/native_rps.rs").read_bytes()).hexdigest(),"verifier_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"corpus_sha256":hashlib.sha256(raw).hexdigest(),
        "native_entries":["0200A4E4","0200A528","0200ABE8","0200ACCC","0200B3B4","0200B3EC","0200AB84","0200B4E0 complete controller from phase2","02008744","020087CC","0200816C native cashout","wave/clock callbacks from earlier proof"],"modeled_primitives":["display/actor rendering and common blink callbacks skipped","sound callbacks observed/skipped","graphics CpuSet and final music fade skipped","SWI0A scalar output from independent compiled mGBA C reference","uninitialized native scratch-list read rejected/rolled back"],
        "limits":["BIOS is numerical reference only: authentic BIOS instructions/timing/clobbers unavailable","Does not prove OAM/art, entry scene loading, active pause/menu or whole-system task scheduling","Native cashout runs only in private oracle memory; no durable MMO wallet mutation","No durable MMO authority/service integration yet"]}
    (output/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8");print(json.dumps(result,indent=2))


if __name__=="__main__":main()
