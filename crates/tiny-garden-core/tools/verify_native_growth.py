#!/usr/bin/env python3
"""Opt-in parity proof against an explicitly supplied, private A2NE payload.

Requires unicorn==2.1.4 and rustc. No ROM/table bytes are bundled or downloaded.
Runs the actual Thumb growth and RNG instructions, modeling only BIOS Div.
Creates a NEW .local output directory and never contacts a game service.
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

PAYLOAD_LENGTH = 191_340
PAYLOAD_SHA1 = "2c7da8e1c9af85986eecf4effae39197cdac9abf"
ENTRY = 0x0200493C
STOP = 0x02004A4A
SAVE = 0x03003B80
SEED = 0x030013A8
ACTOR = 0x03002000
RECORD_LEN = 0x9A
TABLE = 0x12510
INPUT_LEN = RECORD_LEN + 4 + 7
OUTPUT_LEN = RECORD_LEN + 4


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--rustc", default="rustc")
    parser.add_argument("--unicorn-dir", type=Path)
    parser.add_argument("--random-cases", type=int, default=4096)
    args = parser.parse_args()
    if not 1 <= args.random_cases <= 100_000:
        parser.error("--random-cases must be between 1 and 100000")
    return args


def record(
    generator: random.Random, mood: int, belly: int, experience: int,
    grade: int, level: int, points: int,
) -> bytes:
    data = bytearray(generator.randbytes(RECORD_LEN))
    data[0x73] = mood
    data[0x74] = belly
    data[0x78:0x7D] = bytes([experience] * 5)
    data[0x80:0x85] = bytes([grade] * 5)
    data[0x88:0x8D] = bytes([level] * 5)
    data[0x90:0x9A] = points.to_bytes(2, "big") * 5
    return bytes(data)


def cases(random_count: int):
    generator = random.Random(0xA2_4_93C)
    # Cross the actual branch boundaries without copying native fruit bytes.
    for fruit, seed, mood, experience, grade, level, points in itertools.product(
        range(7), [0, 0xFFFFFFFF], [0, 160, 180, 181, 200], [0, 89, 90, 99],
        [0, 5], [0, 98, 99], [0, 9988, 9999],
    ):
        yield record(generator, mood, 100, experience, grade, level, points), seed, fruit
    for index in range(random_count):
        data = bytearray(generator.randbytes(RECORD_LEN))
        # Half the random corpus is valid gameplay state; half exercises exact
        # byte-load/store behavior of malformed native fields, never accepted
        # by the authoritative checked Rust boundary.
        if index % 2 == 0:
            data[0x73] = generator.randrange(201)
            data[0x74] = generator.randrange(201)
            for skill in range(5):
                data[0x78 + skill] = generator.randrange(100)
                data[0x80 + skill] = generator.randrange(6)
                data[0x88 + skill] = generator.randrange(100)
                data[0x90 + 2 * skill:0x92 + 2 * skill] = generator.randrange(10_000).to_bytes(2, "big")
        yield bytes(data), generator.getrandbits(32), generator.randrange(7)


def compile_harness(source: Path, output: Path, rustc: str) -> Path:
    # Rust raw string path is safe even when the checkout contains spaces.
    source_path = str(source.resolve()).replace("\\", "/")
    if '"#' in source_path:
        raise ValueError("unsupported source path delimiter")
    harness = output / "native_growth_harness.rs"
    harness.write_text(
        '#![allow(dead_code)]\n'
        f'#[path = r#"{source_path}"#] mod native_growth;\n'
        'use native_growth::*;\n'
        'use std::io::{self, Read, Write};\n'
        'fn main() {\n'
        ' let mut data=Vec::new(); io::stdin().read_to_end(&mut data).unwrap();\n'
        f' assert_eq!(data.len()%{INPUT_LEN},0,"partial input");\n'
        ' let mut out=Vec::new();\n'
        f' for c in data.chunks_exact({INPUT_LEN}) {{\n'
        f'  let mut record=c[..{RECORD_LEN}].to_vec();\n'
        f'  let seed=u32::from_le_bytes(c[{RECORD_LEN}..{RECORD_LEN + 4}].try_into().unwrap());\n'
        f'  let ratings:[i8;7]=std::array::from_fn(|i|c[{RECORD_LEN + 4}+i] as i8);\n'
        '  let mut growth=NativeGrowth::read_raw_fields(&record).unwrap();\n'
        '  let mut rng=NativeRng::new(seed);\n'
        '  apply_fruit(&mut growth,ratings,&mut rng);\n'
        '  growth.write_raw_fields(&mut record).unwrap();\n'
        '  out.extend_from_slice(&record);out.extend_from_slice(&rng.state().to_le_bytes());\n'
        ' }\n'
        ' io::stdout().write_all(&out).unwrap();\n'
        '}\n', encoding="utf-8",
    )
    executable = output / ("native_growth_harness.exe" if sys.platform == "win32" else "native_growth_harness")
    subprocess.run([rustc, "--edition=2024", "-O", str(harness), "-o", str(executable)], check=True, timeout=120)
    return executable


class NativeOracle:
    def __init__(self, payload: bytes):
        from unicorn import Uc, UC_ARCH_ARM, UC_MODE_THUMB, UC_HOOK_CODE, UC_HOOK_INTR
        from unicorn import arm_const as arm

        self.arm = arm
        self.uc = Uc(UC_ARCH_ARM, UC_MODE_THUMB)
        self.uc.mem_map(0x02000000, 0x40000)
        self.uc.mem_write(0x02000000, payload)
        self.uc.mem_map(0x03000000, 0x10000)
        self.stopped = False
        self.divisions = 0
        self.uc.hook_add(UC_HOOK_CODE, self._stop, begin=STOP, end=STOP)
        self.uc.hook_add(UC_HOOK_INTR, self._bios_div)

    def _stop(self, uc, address, size, user_data):
        if address != STOP:
            raise RuntimeError("unexpected stop hook")
        self.stopped = True
        uc.emu_stop()

    def _bios_div(self, uc, interrupt, user_data):
        # The only external primitive on this path is the GBA BIOS signed Div
        # SWI6. Growth supplies a nonnegative 15-bit dividend and divisor 5.
        arm = self.arm
        pc = uc.reg_read(arm.UC_ARM_REG_PC)
        if interrupt != 2 or bytes(uc.mem_read(pc - 2, 2)) != b"\x06\xdf":
            raise RuntimeError(f"unexpected interrupt {interrupt} at {pc:#x}")
        dividend = uc.reg_read(arm.UC_ARM_REG_R0)
        divisor = uc.reg_read(arm.UC_ARM_REG_R1)
        if not 0 <= dividend <= 0x7FFF or divisor != 5:
            raise RuntimeError("unexpected BIOS Div inputs")
        quotient, remainder = divmod(dividend, divisor)
        uc.reg_write(arm.UC_ARM_REG_R0, quotient)
        uc.reg_write(arm.UC_ARM_REG_R1, remainder)
        uc.reg_write(arm.UC_ARM_REG_R3, quotient)
        self.divisions += 1

    def run(self, data: bytes, seed: int, fruit: int) -> bytes:
        arm = self.arm
        uc = self.uc
        uc.mem_write(0x03000000, bytes(0x10000))
        for number in range(13):
            uc.reg_write(getattr(arm, f"UC_ARM_REG_R{number}"), 0)
        uc.reg_write(arm.UC_ARM_REG_SP, 0x0300F000)
        uc.reg_write(arm.UC_ARM_REG_LR, 0)
        uc.reg_write(arm.UC_ARM_REG_R6, ACTOR)
        uc.mem_write(ACTOR + 0x1B, bytes([fruit]))
        uc.mem_write(SAVE, data)
        uc.mem_write(SEED, struct.pack("<I", seed))
        self.stopped = False
        uc.emu_start(ENTRY | 1, STOP + 2, count=200_000)
        if not self.stopped or uc.reg_read(arm.UC_ARM_REG_PC) != STOP:
            raise RuntimeError("native operation failed to reach its bounded stop")
        return bytes(uc.mem_read(SAVE, RECORD_LEN)) + bytes(uc.mem_read(SEED, 4))


def main() -> int:
    args = parse_args()
    payload = args.payload.read_bytes()
    if len(payload) != PAYLOAD_LENGTH or hashlib.sha1(payload).hexdigest() != PAYLOAD_SHA1:
        raise ValueError("payload identity mismatch; refusing unverified offsets")
    output = args.output_dir.resolve()
    if ".local" not in [part.casefold() for part in output.parts] or output.exists():
        raise ValueError("--output-dir must be a NEW directory beneath .local")
    if args.unicorn_dir:
        sys.path.insert(0, str(args.unicorn_dir.resolve()))
    import unicorn

    if unicorn.__version__ != "2.1.4":
        raise ValueError("this proof requires unicorn==2.1.4")
    rustc = shutil.which(args.rustc)
    if rustc is None:
        raise ValueError("rustc was not found; provide --rustc explicitly")
    output.mkdir(parents=True, exist_ok=False)
    source = Path(__file__).resolve().parents[1] / "src" / "native_growth.rs"
    executable = compile_harness(source, output, rustc)
    corpus = list(cases(args.random_cases))
    raw_input = b"".join(
        data + struct.pack("<I", seed) + payload[TABLE + 7 * fruit:TABLE + 7 * fruit + 7]
        for data, seed, fruit in corpus
    )
    started = time.monotonic()
    rust = subprocess.run([str(executable)], input=raw_input, capture_output=True, check=True, timeout=60).stdout
    if len(rust) != len(corpus) * OUTPUT_LEN:
        raise RuntimeError("Rust harness returned the wrong number of complete records")
    oracle = NativeOracle(payload)
    for index, (data, seed, fruit) in enumerate(corpus):
        native = oracle.run(data, seed, fruit)
        actual = rust[index * OUTPUT_LEN:(index + 1) * OUTPUT_LEN]
        if native != actual:
            different = [offset for offset, (a, b) in enumerate(zip(native, actual)) if a != b]
            raise AssertionError(f"case {index}, fruit {fruit}, seed {seed:#x}: differing offsets {different}")
    result = {
        "result": "pass", "cases": len(corpus), "random_cases": args.random_cases,
        "native_bios_div_calls": oracle.divisions,
        "payload_length": len(payload), "payload_sha1": PAYLOAD_SHA1,
        "native_entry": hex(ENTRY), "native_stop_before_animation": hex(STOP),
        "rust_source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "verifier_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "compared_bytes_per_case": OUTPUT_LEN,
        "corpus_sha256": hashlib.sha256(raw_input).hexdigest(),
        "unicorn_version": unicorn.__version__,
        "elapsed_seconds": round(time.monotonic() - started, 3),
        "scope": "Actual Thumb growth and RNG instructions; only BIOS signed Div SWI6 is modeled for nonnegative dividend/divisor5.",
        "limits": ["Stops before animation allocation", "Not full game emulation", "Not server integration, save checksum validation, ownership or durable persistence"],
    }
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
