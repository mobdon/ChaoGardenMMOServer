#!/usr/bin/env python3
"""Opt-in original A2NE Thumb parity for weed spawn, growth, pull and effects.

No native game bytes are bundled/downloaded. Requires local unicorn==2.1.4,
rustc and the neighboring verified-shop oracle's basic CPU/memory setup.
Creates a NEW .local proof directory; does not contact a game service.
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

sys.dont_write_bytecode = True
from verify_native_shop import NativeOracle, LENGTH, SHA1, SAVE, SEED, GLOBAL, STOP

FIELDS = 0x57
INPUT_LEN = 95
OUTPUT_LEN = 120
WEED = 0x03004568  # Native task slot10, 36-byte records.
EFFECT = WEED + 36
CURSOR = 0x03002100


def records(generator, count):
    data = bytearray(generator.randbytes(FIELDS))
    data[0x10:0x14] = struct.pack("<I",1)
    for index in range(16):
        data[0x37+2*index] = 8+index if index < count else 0
        data[0x38+2*index] = 80+index
    return data


def cases(random_count):
    generator = random.Random(0xA2_34_24)
    for count,timer,flag,seed in itertools.product([0,7,15,16],[0,1,2],[0,4,8,0x10,0x20,0x80],[0,0xFFFFFFFF]):
        data=records(generator,count);data[0x10:0x14]=struct.pack("<I",timer)
        yield bytes(data),seed,0,flag,1
    for op,last in [(1,70),(2,85),(3,35)]:
        for flag,frames in itertools.product([0,0x10,0x80],range(last+1)):
            data=records(generator,1)
            yield bytes(data),123,op,flag,frames
    for _ in range(random_count):
        data=records(generator,generator.randrange(16))
        yield bytes(data),generator.getrandbits(32),0,0,1


def compile_harness(core,output,rustc):
    paths={name:str((core/"src"/f"{name}.rs").resolve()).replace("\\","/") for name in ["native_growth","native_weeds"]}
    if any('"#' in path for path in paths.values()):raise ValueError("unsupported source path delimiter")
    text='''#![allow(dead_code)]
mod gba {pub fn sha256(_: &[u8])->String{unreachable!("reader tested separately")}}
mod shop {
 #[derive(Clone,Debug,PartialEq,Eq)]pub struct Error;
 pub struct TinyShopCatalog;
 impl TinyShopCatalog{pub fn from_verified_payload(_: &[u8])->Result<Self,Error>{unreachable!("reader tested separately")}}
}
'''
    for name,path in paths.items():text+=f'#[path=r#"{path}"#]mod {name};\n'
    text+='''use native_growth::NativeRng;
use native_weeds::*;
use std::{io::{self,Read,Write},num::NonZeroU16};
fn main(){
 let mut data=Vec::new();io::stdin().read_to_end(&mut data).unwrap();assert!(data.len()>=1396);assert_eq!((data.len()-1396)%95,0);
 let assets=NativeWeedAssets{map:std::array::from_fn(|i|u16::from_le_bytes(data[2*i..2*i+2].try_into().unwrap())),
  trig:std::array::from_fn(|i|i16::from_le_bytes(data[880+2*i..882+2*i].try_into().unwrap())),hitbox:std::array::from_fn(|i|data[1392+i]as i8)};
 let mut output=Vec::new();
 for row in data[1396..].as_chunks::<95>().0{
  let fields=NativeWeedFields::read_raw_fields(&row[..87]).unwrap();let mut state=NativeWeeds::restore(fields);
  let mut rng=NativeRng::new(u32::from_le_bytes(row[87..91].try_into().unwrap()));
  let op=row[91];let flags=row[92];let frames=u16::from_le_bytes(row[93..95].try_into().unwrap());
  let mut calls=0_u32;let mut target=None;let mut affine=None;let mut cursor=None;let mut effect=None;
  if op==0{
   let mut r=state.spawn_tick(flags,&assets,&mut rng,NonZeroU16::new(65535).unwrap()).unwrap();calls+=r.random_calls;
   while r.event==SpawnEvent::Pending{r=state.continue_spawn(&assets,&mut rng,NonZeroU16::new(65535).unwrap()).unwrap();calls+=r.random_calls;}
   if let SpawnEvent::Spawned{slot,..}=r.event{target=Some(slot as usize);}
  }else if op==1{
   let p=state.fields.positions[0];state.weeds[0]=NativeWeed{present:true,emerging:true,x:p.x,base_y:p.y,..NativeWeed::default()};target=Some(0);
   for _ in 0..frames{affine=state.update_weed(0,flags).unwrap().affine_parameter;}
  }else if op==2{
   let w=state.weeds[0];state.pull=Some(NativeWeedPull{slot:0,phase:0,counter:0,cursor_x:w.x as i16,cursor_y:w.y-16,cursor_layer:w.base_y+1,cursor_tile:0xb0});cursor=state.pull;target=Some(0);
   for _ in 0..frames{
    if state.pull.is_some(){cursor=Some(state.update_pull(flags,&assets).unwrap().cursor);}
    let update=state.update_weed(0,flags).unwrap();
    if update.effect==Some(EffectCommand::Create){effect=Some(NativeWeedEffect::default());}
    if update.effect==Some(EffectCommand::Release){effect.as_mut().unwrap().releasing=true;}
    if let Some(e)=effect.as_mut(){e.update(flags);}
   }
  }else{
   effect=Some(NativeWeedEffect{releasing:true,..NativeWeedEffect::default()});
   for _ in 0..frames{effect.as_mut().unwrap().update(flags);}
  }
  let mut record=row[..87].to_vec();state.fields.write_raw_fields(&mut record).unwrap();output.extend(record);output.extend(rng.state().to_le_bytes());output.extend(calls.to_le_bytes());
  let w=target.map(|i|state.weeds[i]).unwrap_or_default();output.extend([w.present as u8,w.emerging as u8,w.x]);output.extend(w.y.to_le_bytes());output.extend([w.base_y,w.growth_frame,w.interaction,w.removal_requested as u8]);output.extend(affine.unwrap_or(-1).to_le_bytes());
  output.push(state.pull.is_some()as u8);output.push(cursor.map(|p|p.phase).unwrap_or(0));output.push(cursor.map(|p|p.counter).unwrap_or(0));
  output.extend(cursor.map(|p|p.cursor_x).unwrap_or(0).to_le_bytes());output.extend(cursor.map(|p|p.cursor_y).unwrap_or(0).to_le_bytes());output.extend(cursor.map(|p|p.cursor_tile).unwrap_or(0).to_le_bytes());
  output.push(effect.is_some_and(|e|!e.disposed)as u8);output.extend(effect.map(|e|e.tile).unwrap_or(0).to_le_bytes());output.push(effect.map(|e|e.counter).unwrap_or(0));output.push(effect.is_some_and(|e|e.releasing)as u8);
 }
 io::stdout().write_all(&output).unwrap();
}
'''
    harness=output/"native_weeds_harness.rs";harness.write_text(text,encoding="utf-8")
    executable=output/("native_weeds_harness.exe"if sys.platform=="win32"else"native_weeds_harness")
    subprocess.run([rustc,"--edition=2024",str(harness),"-o",str(executable)],check=True,timeout=60)
    return executable


def bl_target(payload,address):
    a,b=struct.unpack_from("<HH",payload,address-0x02000000)
    if a&0xF800!=0xF000 or b&0xF800!=0xF800:raise ValueError("not a native Thumb BL")
    offset=((a&0x7FF)<<12)|((b&0x7FF)<<1)
    if offset&0x400000:offset-=0x800000
    return address+4+offset


class WeedOracle(NativeOracle):
    def __init__(self,payload):
        self.sound=bl_target(payload,0x02003CEE)
        super().__init__(payload)
    def code(self,uc,address,size,data):
        arm=self.arm
        if address==STOP:self.stopped=True;uc.emu_stop()
        elif address in [0x02001534,0x02001554]:self.rng_calls+=1
        elif address==0x02000D04:
            callback=uc.reg_read(arm.UC_ARM_REG_R0)&~1
            if callback==0x02003534:pointer=WEED;self.target=WEED
            elif callback==0x020036EC:pointer=EFFECT;self.effect=EFFECT
            else:raise RuntimeError(f"unexpected native child {callback:#x}")
            uc.mem_write(pointer,bytes(36));uc.mem_write(pointer,struct.pack("<I",callback|1))
            uc.mem_write(pointer+4,bytes([(pointer-0x03004400)//36]))
            uc.reg_write(arm.UC_ARM_REG_R0,pointer);uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        elif address==0x020012F8:
            uc.reg_write(arm.UC_ARM_REG_R0,0);uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        elif address==0x02000ED4:
            self.disposed.add(uc.reg_read(arm.UC_ARM_REG_R0));uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        elif address in [0x02001398,0x0200133C,self.sound]:
            uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
    def bios_div(self,uc,interrupt,data):
        arm=self.arm;pc=uc.reg_read(arm.UC_ARM_REG_PC);instruction=bytes(uc.mem_read(pc-2,2))
        if interrupt!=2:raise RuntimeError("unexpected native interrupt")
        if instruction==b"\x0f\xdf":
            pointer=uc.reg_read(arm.UC_ARM_REG_R0);self.affine=struct.unpack("<h",bytes(uc.mem_read(pointer,2)))[0];return
        if instruction!=b"\x06\xdf":raise RuntimeError(f"unexpected SVC {instruction.hex()}")
        a=uc.reg_read(arm.UC_ARM_REG_R0);b=uc.reg_read(arm.UC_ARM_REG_R1)
        if a&0x80000000:a-=1<<32
        if b&0x80000000:b-=1<<32
        if b==0:raise RuntimeError("native divide by zero")
        q=(abs(a)//abs(b))*(-1 if(a<0)!=(b<0)else 1);r=a-q*b
        for reg,value in [(arm.UC_ARM_REG_R0,q),(arm.UC_ARM_REG_R1,r),(arm.UC_ARM_REG_R3,abs(q))]:uc.reg_write(reg,value&0xFFFFFFFF)
        self.divisions+=1
    def call(self,address,pointer):
        uc,arm=self.uc,self.arm
        for i in range(13):uc.reg_write(getattr(arm,f"UC_ARM_REG_R{i}"),0)
        uc.reg_write(arm.UC_ARM_REG_SP,0x0300F000);uc.reg_write(arm.UC_ARM_REG_LR,STOP|1);uc.reg_write(arm.UC_ARM_REG_R0,pointer)
        self.stopped=False;uc.emu_start(address|1,STOP+2,count=200_000)
        if not self.stopped:raise RuntimeError("native callback exceeded bound")
    def task(self,pointer,callback,x,y,base,kind):
        u=self.uc;u.mem_write(pointer,bytes(36));u.mem_write(pointer,struct.pack("<I",callback|1));u.mem_write(pointer+4,bytes([(pointer-0x03004400)//36]))
        u.mem_write(pointer+10,struct.pack("<h",x));u.mem_write(pointer+14,struct.pack("<h",y));u.mem_write(pointer+0x1D,bytes([base]));u.mem_write(pointer+0x14,struct.pack("<H",0x109 if kind==0 else 0x18E))
    def run_weed(self,record,seed,op,flags,frames):
        u=self.uc;u.mem_write(0x03000000,bytes(0x10000));u.mem_write(SAVE,record);u.mem_write(SEED,struct.pack("<I",seed));u.mem_write(GLOBAL+15,bytes([flags]))
        count=sum(record[0x37+i*2]!=0 for i in range(16));u.mem_write(0x0300159B,bytes([count]))
        self.target=self.effect=None;self.disposed=set();self.rng_calls=0;self.affine=None;cursor=False
        if op==0:self.call(0x02003424,0x03002000)
        elif op==1:
            self.target=WEED;self.task(WEED,0x02003534,record[0x37],0,record[0x38],0)
            for _ in range(frames):self.affine=None;self.call(struct.unpack("<I",bytes(u.mem_read(WEED,4)))[0]&~1,WEED)
        elif op==2:
            self.target=WEED;self.task(WEED,0x02003614,record[0x37],record[0x38],record[0x38],0)
            u.mem_write(CURSOR,bytes(36));u.mem_write(CURSOR,struct.pack("<I",0x02003C25));u.mem_write(CURSOR+10,struct.pack("<h",record[0x37]));u.mem_write(CURSOR+14,struct.pack("<h",record[0x38]-16));u.mem_write(CURSOR+0x14,struct.pack("<H",0xB0));u.mem_write(CURSOR+0x1B,bytes([10]));cursor=True
            for _ in range(frames):
                if struct.unpack("<I",bytes(u.mem_read(CURSOR,4)))[0]&~1==0x02003C24:self.call(0x02003C24,CURSOR)
                if WEED not in self.disposed:self.call(0x02003614,WEED)
                if self.effect and EFFECT not in self.disposed:self.call(0x020036EC,EFFECT)
        else:
            self.effect=EFFECT;self.task(EFFECT,0x020036EC,0,0,0,1);u.mem_write(EFFECT+7,b"\x01")
            for _ in range(frames):
                if EFFECT not in self.disposed:self.call(0x020036EC,EFFECT)
        result=bytes(u.mem_read(SAVE,FIELDS))+bytes(u.mem_read(SEED,4))+struct.pack("<I",self.rng_calls)
        w=bytes(u.mem_read(self.target,36))if self.target else bytes(36)
        emerging=bool(self.target and(struct.unpack_from("<I",w)[0]&~1)==0x02003534)
        result+=struct.pack("<BBBhBBBBh",bool(self.target and self.target not in self.disposed),emerging,w[10],struct.unpack_from("<h",w,14)[0],w[0x1D],w[0x1F],w[0x19],bool(w[7]),self.affine if self.affine is not None else -1)
        c=bytes(u.mem_read(CURSOR,36))if cursor else bytes(36)
        active=bool(cursor and(struct.unpack_from("<I",c)[0]&~1)==0x02003C24)
        result+=struct.pack("<BBBhhH",active,c[0x19],c[0x1A],struct.unpack_from("<h",c,10)[0],struct.unpack_from("<h",c,14)[0],struct.unpack_from("<H",c,0x14)[0])
        e=bytes(u.mem_read(self.effect,36))if self.effect else bytes(36)
        result+=struct.pack("<BHBB",bool(self.effect and self.effect not in self.disposed),struct.unpack_from("<H",e,0x14)[0],e[0x19],bool(e[7]))
        return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload",required=True,type=Path);parser.add_argument("--output-dir",required=True,type=Path);parser.add_argument("--unicorn-dir",type=Path);parser.add_argument("--rustc",default="rustc");parser.add_argument("--random-cases",type=int,default=2048)
    args=parser.parse_args();payload=args.payload.read_bytes()
    if len(payload)!=LENGTH or hashlib.sha1(payload).hexdigest()!=SHA1:raise ValueError("refusing unverified source payload")
    if not 1<=args.random_cases<=100_000:parser.error("--random-cases must be 1..100000")
    output=args.output_dir.resolve()
    if ".local"not in[part.casefold()for part in output.parts]or output.exists():raise ValueError("output must be NEW under .local")
    if args.unicorn_dir:sys.path.insert(0,str(args.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__!="2.1.4":raise ValueError("requires unicorn==2.1.4")
    rustc=shutil.which(args.rustc)
    if not rustc:raise ValueError("activate Rust or provide --rustc")
    output.mkdir(parents=True,exist_ok=False);core=Path(__file__).resolve().parents[1];executable=compile_harness(core,output,rustc)
    corpus=list(cases(args.random_cases));raw=payload[0x21BC4:0x21BC4+880]+payload[0x1FB74:0x1FB74+512]+payload[0x20708:0x2070C]
    raw+=b"".join(record+struct.pack("<IBBH",seed,op,flags,frames)for record,seed,op,flags,frames in corpus)
    started=time.monotonic();rust=subprocess.run([str(executable)],input=raw,capture_output=True,check=True,timeout=60).stdout
    if len(rust)!=len(corpus)*OUTPUT_LEN:raise RuntimeError(f"wrong Rust record count {len(rust)}")
    oracle=WeedOracle(payload)
    for index,case in enumerate(corpus):
        native=oracle.run_weed(*case);actual=rust[index*OUTPUT_LEN:(index+1)*OUTPUT_LEN]
        if native!=actual:
            offsets=[i for i,(a,b)in enumerate(zip(native,actual))if a!=b]
            raise AssertionError(f"case{index} op{case[2]} flag{case[3]:#x} frames{case[4]} offsets{offsets}")
    result={"result":"pass","cases":len(corpus),"random_cases":args.random_cases,"payload_sha1":SHA1,"compared_bytes_per_case":OUTPUT_LEN,"native_bios_div_calls":oracle.divisions,
        "native_entries":["02003424","02003534","02003614","020036EC","02003C24"],"rust_source_sha256":hashlib.sha256((core/"src/native_weeds.rs").read_bytes()).hexdigest(),
        "verifier_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"base_verifier_sha256":hashlib.sha256((core/"tools/verify_native_shop.py").read_bytes()).hexdigest(),"corpus_sha256":hashlib.sha256(raw).hexdigest(),"unicorn_version":unicorn.__version__,"elapsed_seconds":round(time.monotonic()-started,3),
        "scope":"Original native spawn/RNG/rejection, emergence/affine input, weed/cursor pulling/removal and three-frame child effect callbacks with fixed explicit task ordering.",
        "modeled_primitives":["task allocation/disposal","affine slot allocation/release","sprite queues","sounds","BIOS signed Div","ObjAffineSet observes source input, skips OAM output"],
        "limits":["Does not prove original whole-game task scheduling, initial cursor pick sorting/hit test or renderer","Does not grant rings/stats or integrate durable MMO state","Reader gates separately tested with existing Rust reader"]}
    (output/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8");print(json.dumps(result,indent=2))


if __name__=="__main__":main()
