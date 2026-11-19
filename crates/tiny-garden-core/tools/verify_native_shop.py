#!/usr/bin/env python3
"""Compare Rust stock/timer operations with the owner's exact native A2NE Thumb.

Requires local unicorn==2.1.4 and rustc. No game data is embedded/downloaded.
Models only native task allocation, sprite queuing and BIOS signed Div. The
original native stock selection, RNG, cache, timer and save writes execute.
Creates a NEW ignored .local proof directory; never contacts a game service.
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

LENGTH = 191_340
SHA1 = "2c7da8e1c9af85986eecf4effae39197cdac9abf"
SAVE = 0x03003B80
SEED = 0x030013A8
GLOBAL = 0x03003330
INPUT = 0x030043F0
SHOP = 0x03002000
CURSOR = 0x03002100
STOP = 0x0203F000
FIELDS = 0xD0
INPUT_LEN = FIELDS + 11
OUTPUT_LEN = FIELDS + 9


def make_record(generator, chao=0, jewel=0, cycle=255, egg=255, toys=0, timer=0):
    data = bytearray(generator.randbytes(FIELDS))
    data[0x14:0x18] = struct.pack("<I", timer)
    data[0x1C] = cycle
    for toy in range(3):
        data[0x31 + toy * 2] = 8 + toy if toys & (1 << toy) else 0
    data[0x58] = chao
    data[0x59] = egg
    data[0x5A:0x62] = bytes(generator.choice([0, 1, 2, 3, 4, 5, 6, 255]) for _ in range(8))
    data[0x68] = jewel
    return bytes(data)


def cases(random_cases):
    generator = random.Random(0xA2_76_88)
    for chao, jewel, cycle, blocked, toys, timer, seed in itertools.product(
        [255, 0], [0, 1], [255, 0, 4], [0, 1], range(8), [0, 1, 216000], [0, 0xFFFFFFFF]
    ):
        data = make_record(generator, chao, jewel, cycle, 255, toys, timer)
        yield data, seed, 17, blocked, 0, 1
    for flag, timer in itertools.product([0x10, 0x80, 0x90], [0, 1, 2, 216000]):
        yield make_record(generator, timer=timer), 123, 19, 0, flag, 1
    for _ in range(random_cases):
        data = make_record(generator, generator.choice([255, 0, 1]), generator.randrange(2),
            generator.choice([255, 0, 1, 2, 3, 4]), generator.choice([255, 0, 5, 11]),
            generator.randrange(8), generator.randrange(216001))
        yield data, generator.getrandbits(32), generator.randrange(1_000_000), generator.randrange(2), 0, generator.randrange(2)


def compile_harness(core, output, rustc):
    paths = {name: str((core / "src" / f"{name}.rs").resolve()).replace("\\", "/")
        for name in ["native_growth", "native_shop_state"]}
    if any('"#' in path for path in paths.values()):
        raise ValueError("unsupported raw string path delimiter")
    # The reader itself is tested separately with its real existing crate.
    # These unreachable stubs allow rustc to compile only the unchanged pure
    # state module without introducing a second cartridge/table reader.
    text = '''#![allow(dead_code)]
mod gba { pub fn sha256(_: &[u8])->String { unreachable!("not a reader proof") } }
mod shop {
 #[derive(Clone,Debug,PartialEq,Eq)] pub struct Error;
 pub struct TinyFruit { pub price:u32 }
 pub struct TinyShopCatalog { pub fruits:[TinyFruit;7] }
 impl TinyShopCatalog { pub fn from_verified_payload(_: &[u8])->Result<Self,Error> { unreachable!("not a reader proof") } }
}
'''
    for name, path in paths.items():
        text += f'#[path = r#"{path}"#] mod {name};\n'
    text += '''use native_growth::NativeRng;
use native_shop_state::*;
use std::io::{self,Read,Write};
fn main() {
 let mut data=Vec::new();io::stdin().read_to_end(&mut data).unwrap();
 assert_eq!(data.len()%219,0);
 let mut output=Vec::new();
 for row in data.as_chunks::<219>().0 {
  let fields=NativeShopFields::read_raw_fields(&row[..208]).unwrap();
  let seed=u32::from_le_bytes(row[208..212].try_into().unwrap());
  let frame=u32::from_le_bytes(row[212..216].try_into().unwrap());
  let mut state=NativeShopState::restore(fields,0).unwrap();let mut rng=NativeRng::new(seed);let mut calls=0;
  for _ in 0..=row[218] {
   state.advance_shop_ticks(1,row[217]);
   if row[217]&0x90==0 { calls+=state.open_stock(frame,row[216],&mut rng).unwrap().random_calls; }
  }
  let mut result=row[..208].to_vec();state.fields.write_raw_fields(&mut result).unwrap();
  output.extend(result);output.extend(rng.state().to_le_bytes());output.push(state.cached_egg().unwrap_or(255));
  let stock=state.stock();output.push(stock.and_then(|s|s.egg).unwrap_or(255));output.push(stock.and_then(|s|s.toy).unwrap_or(255));
  output.push(calls);output.push(stock.map(|s|s.fruit_available.iter().enumerate().fold(0_u8,|a,(i,&p)|a|((p as u8)<<i))).unwrap_or(0));
 }
 io::stdout().write_all(&output).unwrap();
}
'''
    harness = output / "native_shop_harness.rs"
    harness.write_text(text, encoding="utf-8")
    executable = output / ("native_shop_harness.exe" if sys.platform == "win32" else "native_shop_harness")
    subprocess.run([rustc, "--edition=2024", str(harness), "-o", str(executable)], check=True, timeout=60)
    return executable


class NativeOracle:
    def __init__(self, payload):
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
        from unicorn import arm_const as arm
        self.arm = arm
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.uc.mem_map(0x02000000, 0x40000)
        self.uc.mem_write(0x02000000, payload)
        self.uc.mem_map(0x03000000, 0x10000)
        self.uc.hook_add(UC_HOOK_CODE, self.code)
        self.uc.hook_add(UC_HOOK_INTR, self.bios_div)
        self.divisions = 0

    def code(self, uc, address, size, data):
        arm = self.arm
        if address == STOP:
            self.stopped = True
            uc.emu_stop()
        elif address == 0x02001534:
            self.rng_calls += 1
        elif address == 0x02000D04:
            # Only allocation is modeled. Native code itself writes subject IDs
            # into each new task, whose construction/rendering is out of scope.
            callback = uc.reg_read(arm.UC_ARM_REG_R0) & ~1
            if callback not in [0x02008B3C, 0x02008B7C, 0x02008BC4]:
                raise RuntimeError(f"unexpected task callback {callback:#x}")
            pointer = 0x03006000 + len(self.tasks) * 0x24
            uc.mem_write(pointer, bytes(0x24))
            self.tasks.append((callback, pointer))
            uc.reg_write(arm.UC_ARM_REG_R0, pointer)
            uc.reg_write(arm.UC_ARM_REG_PC, uc.reg_read(arm.UC_ARM_REG_LR))
        elif address == 0x02001398:
            uc.reg_write(arm.UC_ARM_REG_PC, uc.reg_read(arm.UC_ARM_REG_LR))

    def bios_div(self, uc, interrupt, data):
        arm = self.arm
        pc = uc.reg_read(arm.UC_ARM_REG_PC)
        if interrupt != 2 or bytes(uc.mem_read(pc - 2, 2)) != b"\x06\xdf":
            raise RuntimeError(f"unexpected interrupt {interrupt} at {pc:#x}")
        a = uc.reg_read(arm.UC_ARM_REG_R0)
        b = uc.reg_read(arm.UC_ARM_REG_R1)
        if a & 0x80000000: a -= 1 << 32
        if b != 5 or a < 0: raise RuntimeError("unexpected BIOS Mod inputs")
        q, r = divmod(a, b)
        for reg, value in [(arm.UC_ARM_REG_R0,q),(arm.UC_ARM_REG_R1,r),(arm.UC_ARM_REG_R3,q)]:
            uc.reg_write(reg,value)
        self.divisions += 1

    def run(self, record, seed, frame, blocked, flags, reopen):
        uc, arm = self.uc, self.arm
        uc.mem_write(0x03000000, bytes(0x10000))
        uc.mem_write(SAVE, record)
        uc.mem_write(SEED, struct.pack("<I", seed))
        uc.mem_write(GLOBAL + 8, struct.pack("<I", frame))
        uc.mem_write(GLOBAL + 15, bytes([flags]))
        uc.mem_write(0x03001501, bytes([blocked]))
        uc.mem_write(0x03001530, b"\xff")
        uc.mem_write(0x0300150C, struct.pack("<I", CURSOR))
        uc.mem_write(INPUT + 4, struct.pack("<H", 0x200))
        self.tasks = []
        self.rng_calls = 0
        for _ in range(reopen + 1):
            for index in range(13): uc.reg_write(getattr(arm, f"UC_ARM_REG_R{index}"), 0)
            uc.reg_write(arm.UC_ARM_REG_SP, 0x0300F000)
            uc.reg_write(arm.UC_ARM_REG_LR, STOP | 1)
            uc.reg_write(arm.UC_ARM_REG_R0, SHOP)
            uc.mem_write(SHOP, bytes(0x24))
            uc.mem_write(SHOP + 10, struct.pack("<h", -48))
            self.stopped = False
            uc.emu_start(0x02007689, STOP + 2, count=200_000)
            if not self.stopped: raise RuntimeError("native stock did not return within instruction bound")
        egg = toy = 255
        mask = 0
        for callback, pointer in self.tasks:
            subject = uc.mem_read(pointer + 0x1A,1)[0]
            if callback == 0x02008B3C: mask |= 1 << subject
            elif callback == 0x02008B7C: egg = subject
            else: toy = subject
        return bytes(uc.mem_read(SAVE,FIELDS)) + bytes(uc.mem_read(SEED,4)) + bytes(uc.mem_read(0x03001530,1)) + bytes([egg,toy,self.rng_calls,mask])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--unicorn-dir", type=Path)
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--random-cases", type=int, default=2048)
    args = parser.parse_args()
    if not 1 <= args.random_cases <= 100_000: parser.error("--random-cases must be 1..100000")
    payload = args.payload.read_bytes()
    if len(payload) != LENGTH or hashlib.sha1(payload).hexdigest() != SHA1:
        raise ValueError("refusing unverified native payload")
    output = args.output_dir.resolve()
    if ".local" not in [part.casefold() for part in output.parts] or output.exists():
        raise ValueError("--output-dir must be NEW beneath ignored .local")
    if args.unicorn_dir: sys.path.insert(0,str(args.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__ != "2.1.4": raise ValueError("requires unicorn==2.1.4")
    rustc = shutil.which(args.rustc)
    if rustc is None: raise ValueError("provide --rustc or activate the existing Rust environment")
    output.mkdir(parents=True,exist_ok=False)
    core = Path(__file__).resolve().parents[1]
    executable = compile_harness(core,output,rustc)
    corpus = list(cases(args.random_cases))
    raw = b"".join(record + struct.pack("<IIBBB",seed,frame,blocked,flags,reopen)
        for record,seed,frame,blocked,flags,reopen in corpus)
    started = time.monotonic()
    rust = subprocess.run([str(executable)],input=raw,capture_output=True,check=True,timeout=60).stdout
    if len(rust) != len(corpus)*OUTPUT_LEN: raise RuntimeError("wrong Rust record count")
    oracle = NativeOracle(payload)
    for index, case in enumerate(corpus):
        native = oracle.run(*case)
        actual = rust[index*OUTPUT_LEN:(index+1)*OUTPUT_LEN]
        if native != actual:
            offsets = [i for i,(a,b) in enumerate(zip(native,actual)) if a!=b]
            raise AssertionError(f"case{index} seed{case[1]:#x}, different offsets{offsets}")
    result = {"result":"pass","cases":len(corpus),"random_cases":args.random_cases,
        "native_entry":"0x02007688","payload_sha1":SHA1,"payload_length":LENGTH,
        "native_bios_mod_calls":oracle.divisions,"corpus_sha256":hashlib.sha256(raw).hexdigest(),
        "rust_source_sha256":hashlib.sha256((core/"src/native_shop_state.rs").read_bytes()).hexdigest(),
        "verifier_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "compared_bytes_per_case":OUTPUT_LEN,"unicorn_version":unicorn.__version__,
        "elapsed_seconds":round(time.monotonic()-started,3),
        "scope":"Actual native stock, RNG, cache, timer and save writes across first/reopened shop; fruit/egg/toy IDs captured from native task writes.",
        "modeled_primitives":["task allocation only","sprite queue no-op","BIOS signed Div/Mod for nonnegative frame and divisor5"],
        "limits":["Does not execute purchases or held-item controllers","Reader gates checked separately with the real Rust reader","Does not execute task constructors, sounds, panel rendering, hatching, toy AI, transfer or durable MMO service"]}
    (output/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(result,indent=2))


if __name__ == "__main__": main()
