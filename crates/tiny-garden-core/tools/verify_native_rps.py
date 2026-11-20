#!/usr/bin/env python3
"""Opt-in ASOE Janken clock/target-wave parity against the owner's native Thumb.

Requires local unicorn==2.1.4 and rustc. No game bytes are included/downloaded.
Original clock, target route/removal, score and RNG instructions execute;
only the out-of-scope display and sound callbacks are skipped/observed.
Creates a NEW ignored .local proof directory and contacts no game service.
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
from verify_native_shop import NativeOracle, STOP

LENGTH = 138752
SHA1 = "e2a1beec8519d36eae6478cbf3732942ac692023"
SHA256 = "989acfa4a2522cb78d4dc55cb75232528ebc8b3f5852afdb28410d276339b9d0"
SEED = 0x03000840
TARGETS = 0x03000E78
FIELDS = 127
INPUT_LEN = 138
OUTPUT_LEN = 140
ENTRIES = [0x0200A77C,0x0200A798,0x0200A7C0,0x0200A800,0x0200A8C8,0x0200A8EC,0x02008128]


def initial(generator):
    row = bytearray(generator.randbytes(FIELDS))
    row[0:2] = bytes([30,0])
    for index in range(10):
        start = 2 + 12 * index
        row[start] = 4
        struct.pack_into("<hhh",row,start+4,-32*index,10,8)
    row[122:127] = bytes([0,2,10,10,0])
    return row


def cases(random_count):
    generator = random.Random(0xA5_0E_A8_EC)
    for seconds in range(256):
        yield bytes(initial(generator)),0,0,seconds,1
    for seconds,frames,delta in itertools.product([0,88,89,98,99,127,128,255],[0,1,59,127,128,255],[-255,-10,0,1,10,100,0x7FFFFFFF,-0x80000000]):
        row=initial(generator);row[:2]=bytes([seconds,frames])
        yield bytes(row),0,1,delta,1
    for seconds,frames,ticks in itertools.product([0,1,30,127,128,255],[0,1,59,127,128,255],[1,2,60,1800,1801]):
        row=initial(generator);row[:2]=bytes([seconds,frames])
        yield bytes(row),0,2,0,ticks
    for wave,speed,seconds,seed in itertools.product([255,0,8,127,128],[2,4,255,128,127],[30,88,89,99],[0,0xFFFFFFFF]):
        row=initial(generator);row[:2]=bytes([seconds,59]);row[122:124]=bytes([wave,speed])
        yield bytes(row),seed,3,0,1
    for seed in [0,1,0x80000000,0xFFFFFFFF]:
        yield bytes(initial(generator)),seed,4,0,1
    for route,speed in itertools.product([-32768,-129,-5,-4,-3,-2,-1,0,61,62,63,64,193,194,195,196,257,258,259,260,390,391,392,393,999,32767],[2,4,127,128,255]):
        row=initial(generator);row[2]=0;row[123]=speed;struct.pack_into("<h",row,6,route)
        yield bytes(row),0,5,0,2
    for state,counter,x,y in itertools.product([2,3,4,255],[0,3,29,30,31,127,128,255],[-33,-32,0,239,240,32767],[-33,-32,0,159,160]):
        row=initial(generator);row[2:4]=bytes([state,counter]);struct.pack_into("<hh",row,8,x,y);row[12:14]=bytes([254,3])
        yield bytes(row),1,5,0,2
    for rings,delta in itertools.product(range(256),[-1,1,2,64]):
        row=initial(generator);row[126]=rings
        yield bytes(row),0,6,delta,1
    for _ in range(random_count):
        row=bytearray(generator.randbytes(FIELDS))
        op=generator.choice([1,2,3,4,5,5,5,5,6])
        for index in range(10):
            start=2+12*index
            row[start]=generator.choice([0,1,2,3,4,255])
            if generator.randrange(2):
                struct.pack_into("<hhh",row,start+4,generator.randrange(-400,800),generator.randrange(-50,270),generator.randrange(-50,180))
        row[124]=generator.choice([0,1,10,128,255])
        yield bytes(row),generator.getrandbits(32),op,generator.randrange(-512,513),generator.randrange(1,65)


def compile_harness(core,output,rustc):
    text="#![allow(dead_code)]\n"
    for name in ["native_growth","native_rps"]:
        path=str((core/"src"/f"{name}.rs").resolve()).replace("\\","/")
        if '"#' in path:raise ValueError("unsupported source path delimiter")
        text+=f'#[path=r#"{path}"#]mod {name};\n'
    text+='''use native_growth::NativeRng;
use native_rps::*;
use std::io::{self,Read,Write};
fn main(){
 let mut input=Vec::new();io::stdin().read_to_end(&mut input).unwrap();assert_eq!(input.len()%138,0);let mut output=Vec::new();
 for row in input.as_chunks::<138>().0{
  let mut state=NativeRpsWaves{clock:NativeRpsClock{seconds:row[0],subframes:row[1]},
   targets:std::array::from_fn(|i|NativeRpsTarget{raw:row[2+i*12..14+i*12].try_into().unwrap()}),
   wave:row[122],route_speed:row[123],active_targets:row[124],unhit_targets:row[125],round_rings:row[126]};
  let mut rng=NativeRng::new(u32::from_le_bytes(row[127..131].try_into().unwrap()));
  let op=row[131];let arg=i32::from_le_bytes(row[132..136].try_into().unwrap());let ticks=u16::from_le_bytes(row[136..138].try_into().unwrap());
  let mut random_calls=0_u32;let mut expired=false;let mut appear=0_u16;let mut award=0_u16;
  for _ in 0..ticks{match op{
   0=>state.clock.set(arg as u8),1=>state.clock.add_seconds(arg),2=>expired=state.clock.tick(),
   3=>{state.next_wave(&mut rng);random_calls+=10;},4=>{state.initialize(&mut rng);random_calls+=10;},
   5=>{let update=state.tick_targets(&mut rng);random_calls+=u32::from(update.random_calls);appear+=update.appeared.count_ones()as u16;award+=update.defeated_removed.count_ones()as u16;},
   6=>state.add_round_rings(arg),_=>panic!("unknown operation")}}
  output.extend([state.clock.seconds,state.clock.subframes]);for target in state.targets{output.extend(target.raw);}
  output.extend([state.wave,state.route_speed,state.active_targets,state.unhit_targets,state.round_rings]);
  output.extend(rng.state().to_le_bytes());output.extend(random_calls.to_le_bytes());output.push(expired as u8);output.extend(appear.to_le_bytes());output.extend(award.to_le_bytes());
 }
 io::stdout().write_all(&output).unwrap();
}
'''
    harness=output/"native_rps_harness.rs";harness.write_text(text,encoding="utf-8")
    executable=output/("native_rps_harness.exe"if sys.platform=="win32"else"native_rps_harness")
    subprocess.run([rustc,"--edition=2024",str(harness),"-o",str(executable)],check=True,timeout=60)
    return executable


class RpsOracle(NativeOracle):
    def __init__(self,payload):
        super().__init__(payload)
        self.uc.mem_map(0x07000000,0x1000)

    def code(self,uc,address,size,data):
        arm=self.arm
        if address==STOP:self.stopped=True;uc.emu_stop()
        elif address==0x02001114:self.rng_calls+=1
        elif address==0x0200A728:
            # Display callback writes only out-of-scope glyph/OAM state.
            uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))
        elif address==0x0200C690:
            sound=uc.reg_read(arm.UC_ARM_REG_R0)
            if sound not in [0x1D,0x18]:raise RuntimeError(f"unexpected sound{sound:#x}")
            self.sounds[sound]+=1
            uc.reg_write(arm.UC_ARM_REG_PC,uc.reg_read(arm.UC_ARM_REG_LR))

    def bios_div(self,uc,interrupt,data):
        raise RuntimeError("unexpected native interrupt in bounded RPS callbacks")

    def call(self,address,arg):
        uc,arm=self.uc,self.arm
        for index in range(13):uc.reg_write(getattr(arm,f"UC_ARM_REG_R{index}"),0)
        uc.reg_write(arm.UC_ARM_REG_SP,0x0300F000);uc.reg_write(arm.UC_ARM_REG_LR,STOP|1);uc.reg_write(arm.UC_ARM_REG_R0,arg&0xFFFFFFFF)
        self.stopped=False;uc.emu_start(address|1,STOP+2,count=200_000)
        if not self.stopped:raise RuntimeError("native callback exceeded instruction bound")
        return uc.reg_read(arm.UC_ARM_REG_R0)

    def run_rps(self,row,seed,op,arg,ticks):
        uc=self.uc;uc.mem_write(0x03000000,bytes(0x10000));uc.mem_write(0x07000000,bytes(0x1000))
        uc.mem_write(0x03000E75,row[:2]);uc.mem_write(TARGETS,row[2:122]);uc.mem_write(0x03000EF0,row[122:126]);uc.mem_write(0x03000DE0,row[126:127]);uc.mem_write(SEED,struct.pack("<I",seed))
        self.rng_calls=0;self.sounds={0x1D:0,0x18:0};expired=False
        for _ in range(ticks):
            result=self.call(ENTRIES[op],arg)
            if op==2:expired=bool(result)
        result=bytes(uc.mem_read(0x03000E75,2))+bytes(uc.mem_read(TARGETS,120))+bytes(uc.mem_read(0x03000EF0,4))+bytes(uc.mem_read(0x03000DE0,1))+bytes(uc.mem_read(SEED,4))
        return result+struct.pack("<IBHH",self.rng_calls,expired,self.sounds[0x1D],self.sounds[0x18])


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload",required=True,type=Path);parser.add_argument("--output-dir",required=True,type=Path);parser.add_argument("--unicorn-dir",type=Path);parser.add_argument("--rustc",default="rustc");parser.add_argument("--random-cases",type=int,default=4096)
    args=parser.parse_args();payload=args.payload.read_bytes()
    if len(payload)!=LENGTH or hashlib.sha1(payload).hexdigest()!=SHA1 or hashlib.sha256(payload).hexdigest()!=SHA256:raise ValueError("refusing unverified ASOE source payload")
    if not 1<=args.random_cases<=100_000:parser.error("--random-cases must be1..100000")
    output=args.output_dir.resolve()
    if ".local"not in[part.casefold()for part in output.parts]or output.exists():raise ValueError("output must be NEW under .local")
    if args.unicorn_dir:sys.path.insert(0,str(args.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__!="2.1.4":raise ValueError("requires unicorn==2.1.4")
    rustc=shutil.which(args.rustc)
    if not rustc:raise ValueError("activate Rust or provide --rustc")
    output.mkdir(parents=True,exist_ok=False);core=Path(__file__).resolve().parents[1];executable=compile_harness(core,output,rustc)
    corpus=list(cases(args.random_cases));raw=b"".join(row+struct.pack("<IBiH",seed,op,arg,ticks)for row,seed,op,arg,ticks in corpus)
    started=time.monotonic();rust=subprocess.run([str(executable)],input=raw,capture_output=True,check=True,timeout=60).stdout
    if len(rust)!=len(corpus)*OUTPUT_LEN:raise RuntimeError("wrong Rust record count")
    oracle=RpsOracle(payload)
    for index,case in enumerate(corpus):
        native=oracle.run_rps(*case);actual=rust[index*OUTPUT_LEN:(index+1)*OUTPUT_LEN]
        if native!=actual:
            offsets=[i for i,(a,b)in enumerate(zip(native,actual))if a!=b]
            raise AssertionError(f"case{index} op{case[2]} arg{case[3]} ticks{case[4]} offsets{offsets} native{native.hex()} rust{actual.hex()}")
    result={"result":"pass","cases":len(corpus),"random_cases":args.random_cases,"payload_sha1":SHA1,"payload_sha256":SHA256,"compared_bytes_per_case":OUTPUT_LEN,
        "native_entries":[f"{address:08X}"for address in ENTRIES],"rust_source_sha256":hashlib.sha256((core/"src/native_rps.rs").read_bytes()).hexdigest(),
        "verifier_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),"base_verifier_sha256":hashlib.sha256((core/"tools/verify_native_shop.py").read_bytes()).hexdigest(),"corpus_sha256":hashlib.sha256(raw).hexdigest(),"unicorn_version":unicorn.__version__,"elapsed_seconds":round(time.monotonic()-started,3),
        "scope":"Original ASOE clock, ten-record wave initialization, target route/removal, per-removal round score, RNG state/count and appearance/award sound decisions.",
        "modeled_primitives":["display callback skipped","sound callback observed/skipped"],
        "limits":["Not full Janken: cannon input, projectiles, collisions and native ArcTan2 reflection remain absent","Does not prove renderer, OAM decisions or whole-game task scheduling","Round score does not grant durable MMO rings; no service integration","No cartridge reader added; uses exact supplied decoded payload identity"]}
    (output/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8");print(json.dumps(result,indent=2))


if __name__=="__main__":main()
