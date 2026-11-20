#!/usr/bin/env python3
"""Compare pure Rust new-body construction to actual private A2NE Thumb code.

Explicit verified payload only. Pinned Unicorn2.1.4; cargo on PATH. No game
service calls, live saves, table downloads or original assets in this tool.
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

PAYLOAD_LEN = 191_340
PAYLOAD_SHA1 = "2c7da8e1c9af85986eecf4effae39197cdac9abf"
ENTRY = 0x02001E74
RETURN = 0x0203F000
SAVE = 0x03003B80
SEED = 0x030013A8
RECORD_LEN = 0x858
INPUT_LEN = RECORD_LEN + 9
OUTPUT_LEN = RECORD_LEN + 9


def arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--unicorn-dir", type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--random-cases", type=int, default=2048)
    args = parser.parse_args()
    if not 1 <= args.random_cases <= 100_000:
        parser.error("--random-cases must be between1 and100000")
    return args


def corpus(random_count):
    generator = random.Random(0xA2_1_E74)
    for egg in range(12):
        for alternate in [0, 1]:
            for seed in [0, 1, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF]:
                for frame in [0, 0x7FFFFFFF, 0xFFFFFFFF]:
                    data = bytearray(generator.randbytes(RECORD_LEN))
                    data[0x58] = 0xFF
                    data[0x59] = egg
                    yield bytes(data), seed, alternate, frame
    for _ in range(random_count):
        data = bytearray(generator.randbytes(RECORD_LEN))
        data[0x58] = 0xFF
        data[0x59] = generator.randrange(12)
        yield bytes(data), generator.getrandbits(32), generator.randrange(2), generator.getrandbits(32)


def build_harness(output, cargo):
    crate = Path(__file__).resolve().parents[1]
    module = str(crate / "src" / "native_hatching.rs").replace("\\", "/")
    if '"#' in module:
        raise ValueError("unsupported module path delimiter")
    (output / "src").mkdir()
    manifest = (
        '[package]\nname="native-hatch-proof"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\ntiny-garden-core={path='
        + json.dumps(str(crate).replace("\\", "/")) + '}\n'
    )
    (output / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (output / "src" / "main.rs").write_text(
        '#![allow(dead_code)]\n'
        'use tiny_garden_core::{gba,native_growth,shop};\n'
        f'#[path=r#"{module}"#] mod native_hatching;\n'
        'use native_hatching::*;use native_growth::NativeRng;\n'
        'use std::{io::{self,Read,Write},env,fs};\n'
        'fn main(){\n'
        ' let payload=fs::read(env::args_os().nth(1).expect("payload")).unwrap();\n'
        ' let source=NativeHatchSource::from_verified_payload(&payload).unwrap();\n'
        ' let mut input=Vec::new();io::stdin().read_to_end(&mut input).unwrap();\n'
        f' assert_eq!(input.len()%{INPUT_LEN},0);let mut out=Vec::new();\n'
        f' for c in input.chunks_exact({INPUT_LEN}){{\n'
        f'  let mut record=c[..{RECORD_LEN}].to_vec();\n'
        f'  let mut rng=NativeRng::new(u32::from_le_bytes(c[{RECORD_LEN}..{RECORD_LEN + 4}].try_into().unwrap()));\n'
        f'  let context=NativeHatchContext{{alternate_name_table:c[{RECORD_LEN + 4}]!=0,frame_counter:u32::from_le_bytes(c[{RECORD_LEN + 5}..].try_into().unwrap())}};\n'
        '  let result=hatch_into_empty_native_slot(&mut record,&source,context,&mut rng).unwrap();\n'
        '  out.extend_from_slice(&record);out.extend_from_slice(&rng.state().to_le_bytes());\n'
        '  for (_,value) in result.transient_writes{out.push(value);}\n'
        '  out.extend_from_slice(&result.random_calls.to_le_bytes());\n'
        ' }io::stdout().write_all(&out).unwrap();}\n', encoding="utf-8",
    )
    subprocess.run(
        [cargo, "build", "--offline", "--quiet", "--manifest-path", str(output / "Cargo.toml"), "--target-dir", str(output / "target")],
        check=True, timeout=120,
    )
    return output / "target" / "debug" / ("native-hatch-proof.exe" if sys.platform == "win32" else "native-hatch-proof")


class Oracle:
    def __init__(self, payload):
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
        from unicorn import arm_const as arm
        self.arm = arm
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.uc.mem_map(0x02000000, 0x40000)
        self.uc.mem_write(0x02000000, payload)
        self.uc.mem_map(0x03000000, 0x10000)
        self.stopped = False
        self.random_calls = 0
        self.divisions = 0
        self.fills = 0
        self.uc.hook_add(UC_HOOK_CODE, self.stop, begin=RETURN, end=RETURN)
        self.uc.hook_add(UC_HOOK_CODE, self.random, begin=0x02001534, end=0x02001534)
        self.uc.hook_add(UC_HOOK_CODE, self.random, begin=0x02001554, end=0x02001554)
        self.uc.hook_add(UC_HOOK_INTR, self.bios)

    def stop(self, uc, address, size, user_data):
        self.stopped = True
        uc.emu_stop()

    def random(self, uc, address, size, user_data):
        self.random_calls += 1

    def bios(self, uc, interrupt, user_data):
        arm = self.arm
        pc = uc.reg_read(arm.UC_ARM_REG_PC)
        op = bytes(uc.mem_read(pc - 2, 2))
        first = uc.reg_read(arm.UC_ARM_REG_R0)
        second = uc.reg_read(arm.UC_ARM_REG_R1)
        third = uc.reg_read(arm.UC_ARM_REG_R2)
        if interrupt != 2:
            raise RuntimeError("unexpected interrupt")
        if op == b"\x06\xdf":
            if not 0 <= first <= 0x7FFF or not 1 <= second <= 10:
                raise RuntimeError("unexpected BIOS Div input")
            quotient, remainder = divmod(first, second)
            uc.reg_write(arm.UC_ARM_REG_R0, quotient)
            uc.reg_write(arm.UC_ARM_REG_R1, remainder)
            uc.reg_write(arm.UC_ARM_REG_R3, quotient)
            self.divisions += 1
        elif op == b"\x0b\xdf":
            if not 0x0300EF00 <= first < 0x0300F000 or second != SAVE + 0x6A or third != 0x010003F7:
                raise RuntimeError("unexpected BIOS CpuSet arguments")
            if bytes(uc.mem_read(first, 2)) != b"\x00\x00":
                raise RuntimeError("native fill source was not zero")
            uc.mem_write(second, bytes(0x7EE))
            self.fills += 1
        else:
            raise RuntimeError(f"unexpected SWI opcode {op.hex()} at {pc:#x}")

    def run(self, data, seed, alternate, frame):
        arm = self.arm
        uc = self.uc
        uc.mem_write(0x03000000, bytes(0x10000))
        for number in range(13):
            uc.reg_write(getattr(arm, f"UC_ARM_REG_R{number}"), 0)
        uc.reg_write(arm.UC_ARM_REG_SP, 0x0300F000)
        uc.reg_write(arm.UC_ARM_REG_LR, RETURN | 1)
        uc.mem_write(SAVE, data)
        uc.mem_write(SEED, struct.pack("<I", seed))
        uc.mem_write(0x03003340, bytes([alternate]))
        uc.mem_write(0x03003338, struct.pack("<I", frame))
        self.stopped = False
        self.random_calls = 0
        uc.emu_start(ENTRY | 1, RETURN + 2, count=500_000)
        if not self.stopped or uc.reg_read(arm.UC_ARM_REG_PC) != RETURN:
            raise RuntimeError("initializer failed to return within instruction bound")
        return (
            bytes(uc.mem_read(SAVE, RECORD_LEN)) + bytes(uc.mem_read(SEED, 4))
            + bytes(uc.mem_read(0x03001501, 1)) + bytes(uc.mem_read(0x0300152F, 1))
            + bytes(uc.mem_read(0x0300152D, 1)) + struct.pack("<H", self.random_calls)
        )


def main():
    args = arguments()
    payload = args.payload.read_bytes()
    if len(payload) != PAYLOAD_LEN or hashlib.sha1(payload).hexdigest() != PAYLOAD_SHA1:
        raise ValueError("payload identity mismatch; refusing unverified offsets")
    output = args.output_dir.resolve()
    if ".local" not in [part.casefold() for part in output.parts] or output.exists():
        raise ValueError("output must be a NEW directory beneath .local")
    if args.unicorn_dir:
        sys.path.insert(0, str(args.unicorn_dir.resolve()))
    import unicorn
    if unicorn.__version__ != "2.1.4":
        raise ValueError("this proof requires unicorn==2.1.4")
    cargo = shutil.which(args.cargo)
    if cargo is None:
        raise ValueError("cargo unavailable; supply --cargo")
    output.mkdir(parents=True, exist_ok=False)
    executable = build_harness(output, cargo)
    inputs = list(corpus(args.random_cases))
    encoded = b"".join(data + struct.pack("<IBI", seed, alternate, frame) for data, seed, alternate, frame in inputs)
    started = time.monotonic()
    rust = subprocess.run([str(executable), str(args.payload.resolve())], input=encoded, capture_output=True, check=True, timeout=60).stdout
    if len(rust) != len(inputs) * OUTPUT_LEN:
        raise RuntimeError("wrong Rust output length")
    oracle = Oracle(payload)
    total_random = 0
    for index, case in enumerate(inputs):
        native = oracle.run(*case)
        total_random += oracle.random_calls
        actual = rust[index * OUTPUT_LEN:(index + 1) * OUTPUT_LEN]
        if actual != native:
            offsets = [offset for offset, (a, b) in enumerate(zip(actual, native)) if a != b]
            raise AssertionError(f"case{index}, egg{case[0][0x59]}, seed{case[1]:#x}: differing offsets{offsets}")
    source = Path(__file__).resolve().parents[1] / "src" / "native_hatching.rs"
    result = {
        "result": "pass", "cases": len(inputs), "random_cases": args.random_cases,
        "payload_sha1": PAYLOAD_SHA1, "native_entry": hex(ENTRY),
        "compared_native_record_bytes": RECORD_LEN, "compared_rng_bytes": 4,
        "compared_transient_bytes": 3, "compared_rng_call_count": True,
        "native_random_calls": total_random, "bios_div_calls": oracle.divisions,
        "bios_zero_fill_calls": oracle.fills, "unicorn_version": unicorn.__version__,
        "rust_source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "verifier_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "corpus_sha256": hashlib.sha256(encoded).hexdigest(),
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "scope": "Actual native initializer and RNG instructions; BIOS Div and the exact zero-fill CpuSet primitive modeled.",
        "limits": ["No hatch animation/timer", "No native checksum resealing", "No canonical MMO ownership, adoption, selection, durable save or client integration"],
    }
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
