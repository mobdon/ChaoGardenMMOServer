#!/usr/bin/env python3
"""Private A2NE CPU parity for board placement and matching-phase updates.

Requires pinned unicorn2.1.4 and cargo; verifies the complete private payload
before interpreting offsets. New .local outputs only. No live service access.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import time

PAYLOAD_SHA1 = "2c7da8e1c9af85986eecf4effae39197cdac9abf"
RECORD_LEN = 0x89
RETURN = 0x0203F000
SOURCE_GATE_LEN = 191_340
MATCH_MASK = [0, 0x3E, 0x3F, 0x46, 0x88] + list(range(0x20, 0x3E)) + list(range(0x68, 0x7D)) + list(range(0x80, 0x85))
SHUFFLE_MASK = list(range(0x20, 0x3E)) + list(range(0x47, 0x5E)) + list(range(0x60, 0x64)) + list(range(0x68, 0x78))


def args():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--payload", required=True, type=Path)
    p.add_argument("--output-dir", required=True, type=Path)
    p.add_argument("--unicorn-dir", type=Path)
    p.add_argument("--cases", type=int, default=4096)
    p.add_argument("--cargo", default="cargo")
    a = p.parse_args()
    if not 1 <= a.cases <= 100_000:
        p.error("--cases must be1..100000")
    return a


def compile_harness(output, cargo):
    crate = Path(__file__).resolve().parents[1]
    module = str(crate / "src" / "native_memory.rs").replace("\\", "/")
    if '"#' in module:
        raise ValueError("unsupported source path delimiter")
    (output / "src").mkdir()
    (output / "Cargo.toml").write_text(
        '[package]\nname="native-memory-proof"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\ntiny-garden-core={path=' + json.dumps(str(crate).replace("\\", "/")) + '}\n', encoding="utf-8",
    )
    (output / "src" / "main.rs").write_text(
        '#![allow(dead_code)]\nuse tiny_garden_core::{native_growth,shop};\n'
        f'#[path=r#"{module}"#]mod native_memory;\n'
        'use native_memory::*;use native_growth::NativeRng;use std::{io::{self,Read,Write},env,fs};\n'
        'fn main(){let a:Vec<_>=env::args_os().collect();let payload=fs::read(&a[1]).unwrap();let source=A2neMemorySource::from_verified_payload(&payload).unwrap();\n'
        'let mut input=Vec::new();io::stdin().read_to_end(&mut input).unwrap();let mut out=Vec::new();\n'
        'if a[2]=="board"{assert_eq!(input.len()%4,0);for c in input.chunks_exact(4){let mut rng=NativeRng::new(u32::from_le_bytes(c.try_into().unwrap()));let b=source.create_board(&mut rng).unwrap();for &v in b.native_cells(){out.push(v as u8);}out.extend_from_slice(&rng.state().to_le_bytes());out.extend_from_slice(&b.placement_random_calls.to_le_bytes());}}\n'
        f'else if a[2]=="matching"{{assert_eq!(input.len()%{RECORD_LEN + 4},0);for c in input.chunks_exact({RECORD_LEN + 4}){{let mut raw=c[..{RECORD_LEN}].to_vec();let mut g=NativeMemoryMatching::read_native_fields(&raw).unwrap();g.step(NativeButtons{{pressed:u16::from_le_bytes(c[{RECORD_LEN}..{RECORD_LEN + 2}].try_into().unwrap()),held:u16::from_le_bytes(c[{RECORD_LEN + 2}..].try_into().unwrap())}}).unwrap();g.write_native_fields(&mut raw).unwrap();out.extend_from_slice(&raw);}}}}\n'
        f'else if a[2]=="shuffle"{{assert_eq!(input.len()%{RECORD_LEN + 8},0);for c in input.chunks_exact({RECORD_LEN + 8}){{let mut raw=c[..{RECORD_LEN}].to_vec();let mut rng=NativeRng::new(u32::from_le_bytes(c[{RECORD_LEN}..{RECORD_LEN + 4}].try_into().unwrap()));let mut g=NativeMemoryShuffle::read_native_fields(&raw).unwrap();g.step(NativeButtons{{pressed:u16::from_le_bytes(c[{RECORD_LEN + 4}..{RECORD_LEN + 6}].try_into().unwrap()),held:u16::from_le_bytes(c[{RECORD_LEN + 6}..].try_into().unwrap())}},&mut rng).unwrap();g.write_native_fields(&mut raw).unwrap();out.extend_from_slice(&raw);out.extend_from_slice(&rng.state().to_le_bytes());}}}}\n'
        f'else if a[2]=="shuffle_init"{{assert_eq!(input.len()%34,0);for c in input.chunks_exact(34){{let board=NativeMemoryBoard::from_native_cells(std::array::from_fn(|i|c[i] as i8)).unwrap();let mut rng=NativeRng::new(u32::from_le_bytes(c[30..].try_into().unwrap()));let g=NativeMemoryShuffle::begin(board,&mut rng);let mut raw=vec![0;{RECORD_LEN}];g.write_native_fields(&mut raw).unwrap();out.extend_from_slice(&raw);out.extend_from_slice(&rng.state().to_le_bytes());}}}}\n'
        'else if a[2]=="reveal"{let mut r=NativeMemoryReveal::default();for _ in 0..270{let done=r.step().unwrap();out.push(if done{5}else{4});out.push(if done{0}else{r.stage});out.extend_from_slice(&r.timer.to_le_bytes());for &v in &r.visible{out.push(u8::from(v));}}}\n'
        'else if a[2]=="result"{assert_eq!(input.len()%5,0);for c in input.chunks_exact(5){let mut r=NativeMemoryResult::begin(c[0]!=0,c[1],c[2]).unwrap();let press=u16::from_le_bytes(c[3..].try_into().unwrap());let mut trace=Vec::new();for frame in 0..600_u16{r.step(NativeButtons{pressed:if frame==press{1}else{0},held:0}).unwrap();trace.extend_from_slice(&r.main_state.to_le_bytes());trace.extend_from_slice(&r.stage.to_le_bytes());trace.extend_from_slice(&r.timer.to_le_bytes());trace.extend_from_slice(&[r.ui_kind,r.ui_stage]);trace.extend_from_slice(&r.ui_timer.to_le_bytes());trace.extend_from_slice(&[r.rings,u8::from(r.done)]);if r.done{break;}}assert!(r.done);out.extend_from_slice(&u16::try_from(trace.len()/18).unwrap().to_le_bytes());out.extend_from_slice(&trace);}}\n'
        'io::stdout().write_all(&out).unwrap();}\n', encoding="utf-8",
    )
    subprocess.run([cargo, "build", "--offline", "--quiet", "--manifest-path", str(output / "Cargo.toml"), "--target-dir", str(output / "target")], check=True, timeout=120)
    return output / "target" / "debug" / ("native-memory-proof.exe" if sys.platform == "win32" else "native-memory-proof")


class Oracle:
    def __init__(self, payload):
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
        from unicorn import arm_const as arm
        self.arm = arm
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        for base, size in [(0x02000000, 0x40000), (0x03000000, 0x10000), (0x04000000, 0x1000), (0x05000000, 0x1000), (0x06000000, 0x20000), (0x07000000, 0x1000)]:
            self.uc.mem_map(base, size)
        self.uc.mem_write(0x02000000, payload)
        self.stopped = False
        self.random_calls = 0
        self.sounds = 0
        self.graphics_copies = 0
        self.uc.hook_add(UC_HOOK_CODE, self.stop, begin=RETURN, end=RETURN)
        self.uc.hook_add(UC_HOOK_CODE, self.random, begin=0x02001534, end=0x02001534)
        self.uc.hook_add(UC_HOOK_CODE, self.sound, begin=0x020104E0, end=0x020104E0)
        self.uc.hook_add(UC_HOOK_CODE, self.sound, begin=0x02010678, end=0x02010678)
        self.uc.hook_add(UC_HOOK_INTR, self.bios)

    def stop(self, uc, address, size, user_data):
        self.stopped = True
        uc.emu_stop()

    def random(self, uc, address, size, user_data):
        self.random_calls += 1

    def sound(self, uc, address, size, user_data):
        self.sounds += 1
        uc.reg_write(self.arm.UC_ARM_REG_PC, uc.reg_read(self.arm.UC_ARM_REG_LR))

    def bios(self, uc, interrupt, user_data):
        arm = self.arm
        pc = uc.reg_read(arm.UC_ARM_REG_PC)
        op = bytes(uc.mem_read(pc - 2, 2))
        src = uc.reg_read(arm.UC_ARM_REG_R0)
        dest = uc.reg_read(arm.UC_ARM_REG_R1)
        control = uc.reg_read(arm.UC_ARM_REG_R2)
        if interrupt==2 and op==b"\x0b\xdf" and control==0x01000400 and dest==0x0600C000 and 0x0300EF00<=src<0x0300F000 and bytes(uc.mem_read(src,2))==b"\x00\x00":
            uc.mem_write(dest,bytes(0x800));self.graphics_copies+=1;return
        if interrupt != 2 or op != b"\x0b\xdf" or control != 0x18 or not 0x02015C00 <= src < 0x02016000 or dest not in [0x06000030, 0x06000070]:
            raise RuntimeError(f"unexpected BIOS call {op.hex()}:{src:#x},{dest:#x},{control:#x}")
        uc.mem_write(dest, bytes(uc.mem_read(src, 0x30)))
        self.graphics_copies += 1

    def reset(self):
        arm = self.arm
        self.uc.mem_write(0x03000000, bytes(0x10000))
        for i in range(13):
            self.uc.reg_write(getattr(arm, f"UC_ARM_REG_R{i}"), 0)
        self.uc.reg_write(arm.UC_ARM_REG_SP, 0x0300F000)
        self.uc.reg_write(arm.UC_ARM_REG_LR, RETURN | 1)
        self.random_calls = 0
        self.stopped = False

    def reset_registers(self):
        arm = self.arm
        for i in range(13):
            self.uc.reg_write(getattr(arm, f"UC_ARM_REG_R{i}"), 0)
        self.uc.reg_write(arm.UC_ARM_REG_SP, 0x0300F000)
        self.uc.reg_write(arm.UC_ARM_REG_LR, RETURN | 1)
        self.stopped = False

    def execute(self, entry):
        self.uc.emu_start(entry | 1, RETURN + 2, count=500_000)
        if not self.stopped or self.uc.reg_read(self.arm.UC_ARM_REG_PC) != RETURN:
            raise RuntimeError("native operation failed to return within bound")

    def board(self, seed):
        self.reset()
        self.uc.mem_write(0x030013A8, struct.pack("<I", seed))
        self.execute(0x0200A554)
        return bytes(self.uc.mem_read(0x03001990, 30)) + bytes(self.uc.mem_read(0x030013A8, 4)) + struct.pack("<H", self.random_calls)

    def matching(self, raw, pressed, held):
        self.reset()
        self.uc.mem_write(0x03001970, raw)
        self.uc.mem_write(0x030043F0, struct.pack("<H", held))
        self.uc.mem_write(0x030043F4, struct.pack("<H", pressed))
        self.execute(0x0200A8A4)
        if self.random_calls:
            raise RuntimeError("matching unexpectedly consumed RNG")
        return bytes(self.uc.mem_read(0x03001970, RECORD_LEN))

    def shuffle(self, raw, seed, pressed, held):
        self.reset()
        self.uc.mem_write(0x03001970, raw)
        self.uc.mem_write(0x030013A8, struct.pack("<I", seed))
        self.uc.mem_write(0x030043F0, struct.pack("<H", held))
        self.uc.mem_write(0x030043F4, struct.pack("<H", pressed))
        self.execute(0x0200A8A4)
        return bytes(self.uc.mem_read(0x03001970, RECORD_LEN)) + bytes(self.uc.mem_read(0x030013A8, 4))

    def shuffle_init(self, cells, seed):
        self.reset()
        self.uc.mem_write(0x03001990, cells)
        self.uc.mem_write(0x030019D8, struct.pack("<I", 5))
        self.uc.mem_write(0x030019E0, struct.pack("<I", 1))
        self.uc.mem_write(0x030013A8, struct.pack("<I", seed))
        self.execute(0x0200A7AC)
        return bytes(self.uc.mem_read(0x03001970, RECORD_LEN)) + bytes(self.uc.mem_read(0x030013A8, 4))

    def reveal(self, seed, expected):
        self.board(seed)
        cells=bytes(self.uc.mem_read(0x03001990,30))
        self.uc.mem_write(0x030019D8,struct.pack("<I",4))
        self.uc.mem_write(0x030019E0,bytes(8))
        for frame in range(270):
            self.reset_registers()
            self.execute(0x0200A8A4)
            view=expected[frame*36:(frame+1)*36]
            phase=struct.unpack("<I",bytes(self.uc.mem_read(0x030019D8,4)))[0]
            stage=struct.unpack("<I",bytes(self.uc.mem_read(0x030019E0,4)))[0]
            timer=bytes(self.uc.mem_read(0x030019E4,4))
            if phase!=view[0] or stage!=view[1] or timer!=view[2:6]:
                raise AssertionError(f"reveal frame{frame} native counters differ")
            for position,card in enumerate(cells):
                if card==255:
                    continue
                tile=struct.unpack("<H",bytes(self.uc.mem_read(0x07000004+(11+card)*8,2)))[0]&0x3ff
                if (tile!=0x6f)!=bool(view[6+position]):
                    raise AssertionError(f"reveal frame{frame} cell{position} card{card} visibility differs tile{tile:#x}")

    def result(self,won,mistakes,rings,press,expected):
        self.reset()
        self.uc.mem_write(0x03001990,bytes([255])*30)
        self.uc.mem_write(0x030019D8,struct.pack("<I",7 if won else 8))
        self.uc.mem_write(0x030019B6,bytes([mistakes]))
        self.uc.mem_write(0x03001970,bytes([rings,rings]))
        self.uc.mem_write(0x03001986,struct.pack("<BBh",1,0,17))
        frames=len(expected)//18
        for frame in range(frames):
            self.reset_registers()
            self.uc.mem_write(0x030043F4,struct.pack("<H",1 if frame==press else 0))
            self.execute(0x0200A8A4)
            main=bytes(self.uc.mem_read(0x030019D8,4))
            native=main+bytes(self.uc.mem_read(0x030019E0,8))+bytes(self.uc.mem_read(0x03001986,4))+bytes(self.uc.mem_read(0x03001970,1))+bytes([main==bytes(4)])
            actual=expected[frame*18:(frame+1)*18]
            if native!=actual:
                offsets=[off for off,(x,y)in enumerate(zip(native,actual))if x!=y]
                raise AssertionError(f"result won{won} mistakes{mistakes} rings{rings} frame{frame} offsets{offsets} native{native.hex()} rust{actual.hex()}")


def matching_cases(count):
    gen = random.Random(0xA2_B6_A4)
    for index in range(count):
        state = index % 9
        raw = bytearray(RECORD_LEN)
        board = list(range(14)) + [255] * 16
        gen.shuffle(board)
        raw[0x20:0x3E] = bytes(board)
        raw[0x3E] = gen.randrange(6)
        raw[0x3F] = gen.randrange(5)
        raw[0x46] = gen.randrange(3)
        raw[0x88] = gen.randrange(7)
        raw[0] = raw[1] = gen.randrange(43)
        struct.pack_into("<I", raw, 0x68, 6)
        struct.pack_into("<I", raw, 0x6C, state)
        # Exercise timers on both sides of native branch thresholds.
        timers = {0:[0],1:[0],2:[0],3:[0,82,83],4:[84],5:[0,15,16],6:[0,19,20],7:[0],8:[0]}
        struct.pack_into("<I", raw, 0x74, gen.choice(timers[state]))
        first = gen.randrange(14)
        second = first ^ 1 if state >= 5 else gen.choice([v for v in range(14) if v != first])
        for card_index, card in enumerate([first, second]):
            position = board.index(card)
            off = 0x78 + card_index * 8
            flip = gen.choice([0,1,14,15,28,29,30]) if state <= 2 else 30
            if state == 4:
                flip = gen.choice([0,1,29]) if card_index == 0 else raw[0x79]
            raw[off:off + 5] = bytes([card,flip,position % 6,position // 6,gen.choice([1,3,5])])
        # A third mismatch has already been counted before reverse-flip state.
        if state == 4:
            raw[0x46] = gen.randrange(1,4)
        pressed = gen.choice([0,1,16,32,64,128,16|1,32|64,16|32|64|128])
        yield bytes(raw), pressed, gen.randrange(2)


def shuffle_cases(count):
    gen=random.Random(0xA2_B154)
    for index in range(count):
        raw=bytearray(RECORD_LEN)
        board=list(range(14))+[255]*16;gen.shuffle(board);raw[0x20:0x3e]=bytes(board)
        direction=gen.randrange(4)
        column,row=gen.randrange(6),gen.randrange(5)
        available=[row,4-row,column,5-column][direction]
        if available==0:
            direction^=1
            available=[row,4-row,column,5-column][direction]
        raw[0x47]=column;raw[0x48]=row;raw[0x49]=gen.randrange(1,16);raw[0x4a]=gen.randrange(1,available+1)
        struct.pack_into("<h",raw,0x4c,gen.choice([32,21,20,1]))
        raw[0x4e]=gen.randrange(256);raw[0x4f]=gen.choice([0,0,1,2])
        if raw[0x4f]==1:
            direction=gen.choice([2,3])
        struct.pack_into("<I",raw,0x50,direction)
        struct.pack_into("<hh",raw,0x54,column*32+4,row*32+4)
        raw[0x58]=255
        if index%3==0:
            raw[0x58]=gen.randrange(14);raw[0x59]=gen.choice([1,2,31,32]);struct.pack_into("<hh",raw,0x5a,column*32,row*32)
        struct.pack_into("<I",raw,0x60,gen.randrange(4))
        struct.pack_into("<I",raw,0x68,5);struct.pack_into("<I",raw,0x70,1)
        if index%5==0:
            raw[0x49]=0;struct.pack_into("<I",raw,0x70,2);struct.pack_into("<h",raw,0x4c,gen.choice([0,-1,-30,-160]))
            struct.pack_into("<hh",raw,0x54,gen.choice([-33,-32,-31,0,191,192]),gen.choice([-33,-32,-31,0,159,160]))
        struct.pack_into("<I",raw,0x74,gen.choice([0,9,10,39,40,149,150,179,180,189,190]))
        yield bytes(raw),gen.getrandbits(32),gen.choice([0,1,2]),gen.randrange(2)


def main():
    a = args()
    payload = a.payload.read_bytes()
    if len(payload) != SOURCE_GATE_LEN or hashlib.sha1(payload).hexdigest() != PAYLOAD_SHA1:
        raise ValueError("payload identity mismatch")
    output = a.output_dir.resolve()
    if output.exists() or ".local" not in [part.casefold() for part in output.parts]:
        raise ValueError("output must be NEW and beneath .local")
    if a.unicorn_dir:
        sys.path.insert(0, str(a.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__ != "2.1.4":
        raise ValueError("requires unicorn==2.1.4")
    cargo = shutil.which(a.cargo)
    if not cargo:
        raise ValueError("cargo unavailable")
    output.mkdir(parents=True, exist_ok=False)
    executable = compile_harness(output, cargo)
    generator = random.Random(0xA2_A554)
    seeds = [0,1,0x7FFFFFFF,0x80000000,0xFFFFFFFF] + [generator.getrandbits(32) for _ in range(a.cases)]
    encoded_boards = b"".join(struct.pack("<I", seed) for seed in seeds)
    records = list(matching_cases(a.cases))
    encoded_matches = b"".join(raw + struct.pack("<HH", pressed,held) for raw,pressed,held in records)
    walks=list(shuffle_cases(a.cases))
    encoded_walks=b"".join(raw+struct.pack("<IHH",seed,pressed,held) for raw,seed,pressed,held in walks)
    initial_walks=[(bytes(list(range(14))+[255]*16),seed) for seed in seeds]
    encoded_initial=b"".join(cells+struct.pack("<I",seed) for cells,seed in initial_walks)
    started = time.monotonic()
    boards = subprocess.run([str(executable),str(a.payload.resolve()),"board"], input=encoded_boards,capture_output=True,check=True,timeout=60).stdout
    matches = subprocess.run([str(executable),str(a.payload.resolve()),"matching"],input=encoded_matches,capture_output=True,check=True,timeout=60).stdout
    walk_result=subprocess.run([str(executable),str(a.payload.resolve()),"shuffle"],input=encoded_walks,capture_output=True,check=True,timeout=60).stdout
    initial_result=subprocess.run([str(executable),str(a.payload.resolve()),"shuffle_init"],input=encoded_initial,capture_output=True,check=True,timeout=60).stdout
    reveal_result=subprocess.run([str(executable),str(a.payload.resolve()),"reveal"],input=b"",capture_output=True,check=True,timeout=60).stdout
    endings=[(won,mistakes,rings,press)for won in[True,False]for mistakes in([0,1,2]if won else[3])for rings in[0,24,42]for press in[0,60,200,1000]]
    encoded_endings=b"".join(struct.pack("<BBBH",won,mistakes,rings,press)for won,mistakes,rings,press in endings)
    result_trace=subprocess.run([str(executable),str(a.payload.resolve()),"result"],input=encoded_endings,capture_output=True,check=True,timeout=60).stdout
    if len(boards) != len(seeds)*36 or len(matches) != len(records)*RECORD_LEN or len(walk_result)!=len(walks)*(RECORD_LEN+4) or len(initial_result)!=len(initial_walks)*(RECORD_LEN+4) or len(reveal_result)!=270*36:
        raise RuntimeError("incomplete Rust output")
    oracle = Oracle(payload)
    for index,seed in enumerate(seeds):
        native = oracle.board(seed)
        if native != boards[index*36:(index+1)*36]:
            raise AssertionError(f"board case{index} seed{seed:#x} mismatch")
    for index,case in enumerate(records):
        native = oracle.matching(*case)
        actual = matches[index*RECORD_LEN:(index+1)*RECORD_LEN]
        differing = [offset for offset in MATCH_MASK if native[offset] != actual[offset]]
        if differing:
            raise AssertionError(f"matching case{index},state{case[0][0x6c]} mismatched offsets{[hex(v) for v in differing]}")
    for index,case in enumerate(initial_walks):
        native=oracle.shuffle_init(*case);actual=initial_result[index*(RECORD_LEN+4):(index+1)*(RECORD_LEN+4)]
        differing=[off for off in SHUFFLE_MASK+list(range(RECORD_LEN,RECORD_LEN+4)) if native[off]!=actual[off]]
        if differing:raise AssertionError(f"shuffle init case{index}: differing offsets{[hex(v) for v in differing]}")
    for index,case in enumerate(walks):
        native=oracle.shuffle(*case);actual=walk_result[index*(RECORD_LEN+4):(index+1)*(RECORD_LEN+4)]
        differing=[off for off in SHUFFLE_MASK+list(range(RECORD_LEN,RECORD_LEN+4)) if native[off]!=actual[off]]
        if differing:raise AssertionError(f"shuffle case{index}: differing offsets{[hex(v) for v in differing]}")
    for seed in seeds[:min(32,len(seeds))]:oracle.reveal(seed,reveal_result)
    end_offset=0;result_frames=0
    for case in endings:
        frames=struct.unpack_from("<H",result_trace,end_offset)[0];end_offset+=2
        trace=result_trace[end_offset:end_offset+frames*18];end_offset+=frames*18
        if len(trace)!=frames*18:raise RuntimeError("truncated result trace")
        oracle.result(*case,trace);result_frames+=frames
    if end_offset!=len(result_trace):raise RuntimeError("surplus result bytes")
    source = Path(__file__).resolve().parents[1]/"src"/"native_memory.rs"
    result = {
        "result":"pass","board_cases":len(seeds),"matching_frame_cases":len(records),
        "shuffle_initial_cases":len(initial_walks),"shuffle_frame_cases":len(walks),"full_reveal_traces":min(32,len(seeds)),
        "full_result_traces":len(endings),"result_frames":result_frames,
        "payload_sha1":PAYLOAD_SHA1,"board_native_entry":"0x0200A554","matching_dispatch_entry":"0x0200A8A4",
        "board_compared_bytes":36,"matching_compared_offsets":MATCH_MASK,
        "sound_calls_stubbed":oracle.sounds,"bios_graphic_copies_modeled":oracle.graphics_copies,
        "scope":"Actual Thumb board, reveal, traversal/trip, matching and result counters with native RNG/software arithmetic; audio callbacks skipped and exact BIOS graphics copies/fill modeled.",
        "limits":["Matching excludes cursor interpolation and animated score display","No pause/instruction presentation, complete service integration or durable MMO reward transaction"],
        "source_sha256":hashlib.sha256(source.read_bytes()).hexdigest(),
        "verifier_sha256":hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "corpus_sha256":hashlib.sha256(encoded_boards+encoded_matches+encoded_walks+encoded_initial+encoded_endings).hexdigest(),
        "elapsed_seconds":round(time.monotonic()-started,3),
    }
    (output/"result.json").write_text(json.dumps(result,indent=2)+"\n",encoding="utf-8")
    print(json.dumps(result,indent=2))


if __name__ == "__main__":
    main()
