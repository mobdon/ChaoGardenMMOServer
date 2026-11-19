#!/usr/bin/env python3
"""Compare pure Rust operations against Thumb instructions from a local A3VE payload.

Requires Python 3.10+, Unicorn 2.1.4 and an offline Rust toolchain. No service
or save access. Native instructions, boards, vectors and subprocess builds
remain in memory or a temporary directory.
"""

import argparse
from functools import lru_cache
import hashlib
import json
import os
from pathlib import Path
import random
import struct
import subprocess
import sys
import tempfile

DIGEST = "33dcbab013c9007ad838d8cbff693eef140768ca4110c9617c4055aa75659efb"
BASE = 0x02000000
IWRAM = 0x03000000
OBJECT = IWRAM + 0x5400
END = BASE + 0x3F000
GRID = IWRAM + 0x1AE8
FLAGS = IWRAM + 0x1AE6
MAIN = IWRAM + 0x4250
FEED = IWRAM + 0x1DF0


class Oracle:
    def __init__(self, payload, scheduler=False):
        import unicorn as u
        from unicorn import arm_const as reg

        if u.__version__ != "2.1.4":
            raise ValueError("expected Unicorn 2.1.4")
        self.reg = reg
        self.cpu = u.Uc(u.UC_ARCH_ARM, u.UC_MODE_THUMB)
        for start, size in (
            (BASE, 0x40000),
            (IWRAM, 0x8000),
            (0x04000000, 0x1000),
            (0x05000000, 0x1000),
            (0x06000000, 0x20000),
            (0x07000000, 0x1000),
        ):
            self.cpu.mem_map(start, size)
        self.cpu.mem_write(BASE, payload)
        self.spawned = []
        self.removed = False

        # External sprite submission, sound, register writes, and result/save
        # notifications. Game logic under comparison runs as Thumb. Task
        # allocation returns separate synthetic slots for observing spawn data.
        def host(cpu, pc, size, _):
            if pc == BASE + 0x11AE8:  # BIOS signed division, positive operands here
                a, b = self.r(0), self.r(1)
                self.setr(0, a // b)
                self.setr(1, a % b)
                self.setr(3, a // b)
            elif pc == BASE + 0xD04:
                dst = IWRAM + 0x3000 + 36 * len(self.spawned)
                self.spawned.append(dst)
                cpu.mem_write(dst, bytes(36))
                self.setr(0, dst)
            elif pc == BASE + 0xED4:
                self.removed = True
            cpu.reg_write(reg.UC_ARM_REG_PC, cpu.reg_read(reg.UC_ARM_REG_LR))

        for target in (
            0x11AF4,
            0x10698,
            0xDB0,
            0xDB28,
            0x1718,
            0x163C,
            0x117C,
            0x1088,
            0x1374,
            0x10804,
            0x11AE0,
            0x167C,
            0x17B0,
            0x107A0,
            0x107D8,
            0x11AE8,
            0xD04,
            0xED4,
            0x1398,
            0x1254,
            0x1698,
            0x1066C,
            0xE888,
            0xE6D8,
            0x11AE4,
            0x16D8,
            0x12E0,
            0x1420,
            0xE28,
        ):
            if scheduler and target in (0xD04, 0xED4):
                continue
            self.cpu.hook_add(
                u.UC_HOOK_CODE, host, begin=BASE + target, end=BASE + target
            )

    def setr(self, i, value):
        self.cpu.reg_write(getattr(self.reg, f"UC_ARM_REG_R{i}"), value)

    def r(self, i):
        return self.cpu.reg_read(getattr(self.reg, f"UC_ARM_REG_R{i}"))

    def write(self, address, fmt, *values):
        self.cpu.mem_write(address, struct.pack("<" + fmt, *values))

    def read(self, address, fmt):
        return struct.unpack(
            "<" + fmt, self.cpu.mem_read(address, struct.calcsize("<" + fmt))
        )[0]

    def run(self, offset, stop=END, args=()):
        self.cpu.reg_write(self.reg.UC_ARM_REG_SP, IWRAM + 0x7E00)
        self.cpu.reg_write(self.reg.UC_ARM_REG_LR, END | 1)
        for i, v in enumerate(args):
            self.setr(i, v)
        self.cpu.emu_start(BASE + offset | 1, stop, count=200000)
        pc = self.cpu.reg_read(self.reg.UC_ARM_REG_PC)
        if pc != stop:
            raise AssertionError(f"incomplete Thumb operation at {pc:#x}")

    def board(self, address):
        return [list(self.cpu.mem_read(address + 12 * r + 1, 10)) for r in range(1, 11)]

    def setup_stage(self, stage, seed):
        self.write(IWRAM + 0x1AD1, "B", stage)
        self.write(IWRAM + 0x13A8, "I", seed)
        self.cpu.mem_write(FEED, b"\xcc" * 512)
        self.run(0xD504, BASE + 0xD6C4)
        self.run(0xE3B4, args=(OBJECT,))
        self.spawned.clear()

    def eval(self, v):
        self.cpu.mem_write(IWRAM, bytes(0x8000))
        self.spawned.clear()
        self.removed = False
        op, *a = v
        if op in ("stage", "drop"):
            self.setup_stage(*a[:2])
        if op == "stage":
            feed = list(bytes(self.cpu.mem_read(FEED, 512)).rstrip(b"\xcc"))
            return [
                feed,
                self.read(IWRAM + 0x13A8, "I"),
                self.read(FLAGS - 2, "B"),
                self.read(OBJECT + 28, "B"),
                self.read(OBJECT + 25, "B"),
            ]
        if op in ("flip", "drop", "shot", "control", "resolve"):
            board = v[
                {"flip": 3, "drop": 3, "shot": 6, "control": 12, "resolve": 4}[op]
            ]
            self.cpu.mem_write(GRID + 1, bytes([0xF0]) * 10)
            for i, row in enumerate(board):
                self.cpu.mem_write(GRID + 13 + 12 * i, bytes(row))
        if op == "flip":
            self.run(0xEA5C, args=(a[0] + 1, a[1] + 1))
            effect = self.board(IWRAM + 0x1B70)
            if bool(self.r(0)) != any(any(r) for r in effect):
                raise AssertionError("native flip return")
            return [self.board(GRID), effect]
        if op == "drop":
            self.write(FLAGS - 2, "BBB", *a[3:6])
            self.write(IWRAM + 0x29F0, "h", a[6])
            self.write(OBJECT + 25, "B", a[7])
            self.write(MAIN + 15, "B", a[8])
            self.run(0xE1F0, args=(OBJECT,))
            drops = [
                [
                    self.read(p + 30, "B"),
                    self.read(p + 10, "h"),
                    self.read(p + 14, "h"),
                    self.read(p + 26, "B"),
                ]
                for p in self.spawned
            ]
            return [
                self.board(GRID),
                *self.cpu.mem_read(FLAGS - 2, 3),
                self.read(IWRAM + 0x29F0, "h"),
                self.read(OBJECT + 25, "B"),
                drops,
            ]
        if op == "shot":
            column, color, y, scroll, scene, _ = a
            self.write(OBJECT + 25, "B", column)
            self.write(OBJECT + 7, "B", color - 1)
            self.write(OBJECT + 14, "h", y)
            self.write(IWRAM + 0x29F0, "h", scroll)
            self.write(MAIN + 15, "B", scene)
            self.run(0xE08C, args=(OBJECT,))
            if self.spawned:
                p = self.spawned[0]
                if self.read(p + 10, "h") != column or self.read(p + 25, "B") != color:
                    raise AssertionError("native impact record")
                return [None, self.read(p + 14, "h")]
            return [self.read(OBJECT + 14, "h"), 0]
        if op == "control":
            (
                x,
                col,
                phase,
                facing,
                timer,
                frame,
                anim,
                held,
                pressed,
                scene,
                flags,
                _,
            ) = a
            self.write(OBJECT + 10, "h", x)
            self.write(OBJECT + 25, "BB", col, timer)
            self.write(OBJECT + 7, "B", facing | (0x80 if phase in [5, 6] else 0))
            self.write(OBJECT + 22, "B", phase)
            child = IWRAM + 0x5320
            self.write(child + 25, "BB", frame, anim)
            self.write(IWRAM + 0x5310, "H", held)
            self.write(IWRAM + 0x5314, "H", pressed)
            self.write(MAIN + 15, "B", scene)
            self.write(FLAGS, "B", flags)
            self.run(0xDBA8, args=(OBJECT,))
            newphase = self.read(OBJECT + 22, "B")
            direction = self.read(OBJECT + 7, "B") & 1
            return [
                self.read(OBJECT + 10, "h"),
                self.read(OBJECT + 25, "B"),
                newphase,
                direction,
                self.read(OBJECT + 26, "B"),
                *self.cpu.mem_read(child + 25, 2),
                1 + direction if self.spawned and newphase == 4 else 0,
            ]
        if op == "resolve":
            row, col, color, _, remaining = a
            self.write(OBJECT + 10, "h", col)
            self.write(OBJECT + 14, "h", row)
            self.write(OBJECT + 25, "B", color)
            self.write(FLAGS - 2, "B", remaining)
            digest = hashlib.sha256()
            for tick in range(1, 2049):
                self.run(0xE430, args=(OBJECT,))
                digest.update(
                    bytes(
                        [
                            6 if self.removed else self.read(OBJECT + 22, "B"),
                            self.read(OBJECT + 26, "B"),
                            self.read(FLAGS, "B"),
                            self.read(FLAGS - 2, "B"),
                        ]
                    )
                )
                for line in self.board(GRID):
                    digest.update(bytes(line))
                if self.removed:
                    for p in self.spawned:
                        digest.update(
                            struct.pack(
                                "<BBhh",
                                self.read(p + 30, "B"),
                                self.read(p + 26, "B"),
                                self.read(p + 10, "h"),
                                self.read(p + 14, "h"),
                            )
                        )
                    return [tick, digest.hexdigest()]
            raise AssertionError("native resolution exceeded frame limit")
        if op == "wallet":
            self.write(MAIN + 12, "H", 15)
            self.write(IWRAM + 0x6194, "I", a[0])
            self.write(IWRAM + 0x1AD2, "H", a[1])
            self.run(0xF624)
            return self.read(IWRAM + 0x6194, "I")
        x, y, vel, delay, player, scene, flags = a
        self.write(OBJECT + 10, "h", x)
        self.write(OBJECT + 12, "i", y)
        self.write(OBJECT + 32, "h", vel)
        self.write(OBJECT + 26, "B", delay)
        self.write(IWRAM + 0x29F6, "h", player)
        self.write(MAIN + 15, "B", scene)
        self.write(FLAGS, "B", flags)
        self.run(0xF218, args=(OBJECT,))
        caught = self.read(IWRAM + 0x1AD2, "H")
        return [
            self.read(OBJECT + 12, "i"),
            self.read(OBJECT + 32, "h"),
            self.read(OBJECT + 26, "B"),
            2 if self.removed else caught,
        ]


def vectors(n):
    r = random.Random(0xA3BE)
    for i in range(n):
        stage = i % 4
        seed = r.getrandbits(32)
        yield ["stage", stage, seed]
        board = [
            [r.choice((0, 1, 1, 1, 2, 2, 2, 16, 80, 160)) for _ in range(10)]
            for _ in range(10)
        ]
        row, col = r.randrange(9), r.randrange(10)
        board[row][col] = r.choice([1, 2])
        yield ["flip", row, col, board]
        scroll = r.randrange(0, 16, 2)
        yield [
            "shot",
            col + 1,
            r.choice([1, 2]),
            r.randrange(4, 140),
            scroll,
            r.choice([0, 0, 128]),
            board,
        ]
        for drop_scroll in (0, 2):
            yield [
                "drop",
                stage,
                seed,
                board,
                r.randint(15, 30),
                r.randint(0, 1),
                r.choice([0, 1, 2, 4, 64, 128]),
                drop_scroll,
                r.choice([1, 20]),
                r.choice([0, 0, 0, 128]),
            ]
        x = r.randrange(0, 192)
        yield [
            "ring",
            x,
            r.choice([135, 136, 167, 168, r.randrange(20, 170)]) * 65536
            + r.randrange(65536),
            r.choice([-32, 0, 128, r.randrange(3000)]),
            r.choice([0, 0, 0, 1, 2]),
            x + r.choice([-12, -11, 0, 11, 12]),
            r.choice([0, 0, 0, 128]),
            r.choice([0, 0, 64]),
        ]
        yield [
            "control",
            24 + 16 * col,
            col + 1,
            r.randrange(7),
            r.randrange(2),
            r.randrange(1, 10),
            r.randrange(4),
            r.randrange(8),
            r.choice([0, 16, 32, 48]),
            r.randrange(4),
            r.choice([0, 0, 0, 128]),
            r.choice([0, 0, 0, 1, 2, 4, 64, 128]),
            board,
        ]
        cells = [
            [r.choice((1, 1, 2, 2, 0, 16)) if k < 5 else 0 for _ in range(10)]
            for k in range(10)
        ]
        if i % 3 == 0:
            cells[0] = [1] * 10
        if i % 4 == 0:
            cells[2] = [2] * 10
        yield [
            "resolve",
            r.randrange(1, 6),
            r.randrange(1, 11),
            r.choice([1, 2]),
            cells,
            r.choice([5, 8, 13]),
        ]
        yield [
            "wallet",
            r.choice([0, 99998, 99999, 0xFFFFFFFF, r.getrandbits(32)]),
            r.randrange(65536),
        ]


@lru_cache(maxsize=4096)
def choose_move(cells):
    board = [list(line) for line in cells]
    choices = []
    for c in range(10):
        r = next((k for k in reversed(range(9)) if board[k][c]), -1)
        if r >= 7:
            continue
        for color in (1, 2):
            board[r + 1][c] = color
            converted = set()
            for dy, dx in [
                (1, -1),
                (0, -1),
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, 1),
                (1, 1),
            ]:
                rr, cc = r + 1 + dy, c + dx
                ray = []
                while 0 <= rr < 10 and 0 <= cc < 10 and board[rr][cc] in (1, 2):
                    if board[rr][cc] == color:
                        converted.update(ray)
                        break
                    ray.append((rr, cc))
                    rr += dy
                    cc += dx
            seen = set()
            queue = [(r + 1, c)]
            while queue:
                y, x = queue.pop()
                if (y, x) in seen or not (0 <= x < 10 and 0 <= y < 10):
                    continue
                if board[y][x] != color and (y, x) not in converted:
                    continue
                seen.add((y, x))
                queue.extend([(y + 1, x), (y - 1, x), (y, x - 1), (y, x + 1)])
            if len(seen) >= 10:
                score = 200 + len(seen) + sum(y * y / 12 for y, x in seen)
            else:
                score = 2.3 * len(seen) + len(converted) - 1.2 * (r + 1) ** 2
            choices.append((score, c, color))
            board[r + 1][c] = 0
    return choices


def play_case(oracle, stage, seed, mode):
    # Run the real linked-list scheduler, allocator, stage and input actors.
    # Fifteen intro frames elapse before controls first become active.
    oracle.cpu.mem_write(IWRAM, bytes(0x8000))
    oracle.write(IWRAM + 0x13A8, "I", seed)
    oracle.write(IWRAM + 0x1AD1, "B", stage)
    oracle.run(0xD504)
    for _ in range(15):
        oracle.run(0xEAC)
    held_prev = 0
    frames = []
    trace = []
    for tick in range(30000):
        obj = IWRAM + 0x5320 + 2 * 36
        desc = obj + 36
        if mode == "idle":
            held = 0
        elif mode == "sweep":
            held = (16 if tick // 80 % 2 == 0 else 32) | (
                (1 if tick // 35 % 2 == 0 else 2) if tick % 10 == 0 else 0
            )
        else:
            board = oracle.board(GRID)
            col = oracle.read(obj + 25, "B") - 1
            choices = choose_move(tuple(tuple(x) for x in board))
            choices = [
                (score - 0.12 * abs(c - col), c, color) for score, c, color in choices
            ]
            if choices:
                _, target, color = max(choices)
                held = (
                    16
                    if target > col
                    else (
                        32
                        if target < col
                        else ((2 if color == 1 else 1) if tick % 2 == 0 else 0)
                    )
                )
            else:
                held = 0
        frames.append(held)
        oracle.write(IWRAM + 0x5310, "H", held)
        oracle.write(IWRAM + 0x5314, "H", held & ~held_prev)
        held_prev = held
        oracle.run(0xEAC)
        board_hash = hashlib.sha256(
            b"".join(bytes(row) for row in oracle.board(GRID))
        ).hexdigest()
        trace.append(
            [
                board_hash,
                oracle.read(FLAGS, "B"),
                oracle.read(FLAGS - 2, "B"),
                oracle.read(desc + 25, "B"),
                oracle.read(IWRAM + 0x29F0, "h"),
                oracle.read(obj + 10, "h"),
                oracle.read(obj + 25, "B"),
                oracle.read(obj + 22, "B"),
                oracle.read(IWRAM + 0x1AD2, "H"),
            ]
        )
        if trace[-1][7] in (5, 6):
            break
    else:
        raise AssertionError(f"no terminal signal for {mode}/{stage}")
    return ["play", stage, seed, frames], trace


def session_case(o, mode, seed):
    o.cpu.mem_write(IWRAM, bytes(0x8000))
    o.write(IWRAM + 0x13A8, "I", seed)
    o.run(0xD294)
    frames = []
    events = []
    full = hashlib.sha256()
    board_changes = []
    current_board = None
    prev = 0
    last_stage = 0
    last_rings = 0
    finish = 0
    for tick in range(1, 65000):
        obj = IWRAM + 0x5320 + 2 * 36
        held = 0
        if mode == "quit":
            held = {48: 8, 52: 128, 54: 1}.get(tick, 0)
        elif mode == "loss":
            held = 1 if tick > 1090 and tick % 3 == 0 else 0
        elif 48 <= tick <= 56:
            held = {48: 8, 52: 128, 54: 2}.get(tick, 0)
        else:
            col = o.read(obj + 25, "B") - 1
            choices = choose_move(tuple(tuple(row) for row in o.board(GRID)))
            if choices and 0 <= col < 10:
                _, target, color = max(
                    (score - 0.12 * abs(c - col), c, color)
                    for score, c, color in choices
                )
                held = (
                    16
                    if target > col
                    else (
                        32
                        if target < col
                        else ((2 if color == 1 else 1) if tick % 2 == 0 else 0)
                    )
                )
        frames.append(held)
        o.write(IWRAM + 0x5310, "H", held)
        o.write(IWRAM + 0x5314, "H", held & ~prev)
        prev = held
        callback = o.read(MAIN, "I") - BASE - 1
        o.run(callback)
        stage = o.read(IWRAM + 0x1AD1, "B")
        rings = o.read(IWRAM + 0x1AD2, "H")
        phase = o.read(obj + 22, "B")
        if phase == 5:
            finish = 2
        elif phase == 6 and stage == 3:
            finish = 3
        done = o.read(MAIN, "I") == BASE + 0x1BF9
        full.update(bytes([stage]) + struct.pack("<H", rings))
        b = bytes(v for row in o.board(GRID) for v in row)
        full.update(b)
        if b != current_board:
            board_changes.append([tick, hashlib.sha256(b).hexdigest()])
            current_board = b
        if done and not finish:
            finish = 1
        if done or stage != last_stage or rings != last_rings:
            events.append([tick, stage, rings, finish if done else 0])
        last_stage = stage
        last_rings = rings
        if done:
            if o.read(IWRAM + 0x6194, "I") != rings:
                raise AssertionError("native return wallet mismatch")
            return ["session", seed, frames], [events, full.hexdigest(), board_changes]
    raise AssertionError("session never returned to Garden")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--payload", required=True, type=Path)
    p.add_argument("--unicorn-dir", type=Path)
    p.add_argument("--cases", type=int, default=128)
    p.add_argument("--cargo", default="cargo")
    a = p.parse_args()
    if not 4 <= a.cases <= 4096:
        p.error("cases must be 4..4096")
    data = a.payload.read_bytes()
    if len(data) != 188716 or hashlib.sha256(data).hexdigest() != DIGEST:
        p.error("wrong A3VE payload")
    if a.unicorn_dir:
        sys.path.insert(0, str(a.unicorn_dir.resolve()))
    oracle = Oracle(data)
    cases = list(vectors(a.cases))
    scheduler = Oracle(data, scheduler=True)
    plays = {}
    for stage, mode in [
        (0, "idle"),
        (1, "sweep"),
        (2, "sweep"),
        (3, "sweep"),
        (0, "clear"),
        (1, "clear"),
        (2, "clear"),
        (3, "clear"),
    ]:
        case, trace = play_case(scheduler, stage, 0x13579 + stage, mode)
        plays[len(cases)] = trace
        cases.append(case)
    sessions = {}
    for mode, seed in [
        ("quit", 21),
        ("loss", 23),
        ("late_loss", 0x13579),
        ("clear", 1),
    ]:
        case, events = session_case(scheduler, mode, seed)
        if events[0][-1][3] != {"quit": 1, "loss": 2, "late_loss": 2, "clear": 3}[mode]:
            raise AssertionError("wrong native ending for " + mode)
        sessions[len(cases)] = events
        cases.append(case)
    crate = Path(__file__).resolve().parents[1]
    with tempfile.TemporaryDirectory(prefix="native-ball-proof-") as tmp:
        root = Path(tmp)
        manifest = root / "Cargo.toml"
        manifest.write_text(
            '[package]\nname="ball-proof"\nversion="0.0.0"\nedition="2024"\n[workspace]\n'
            + '[dependencies]\nserde_json="1"\nsha2="0.10"\ntiny-garden-core={path='
            + json.dumps(crate.as_posix())
            + "}\n"
            + '[[bin]]\nname="ball-proof"\npath='
            + json.dumps((crate / "tools/native_ball_probe.rs").as_posix())
            + "\n"
        )
        target = root / "target"
        subprocess.run(
            [
                a.cargo,
                "build",
                "--quiet",
                "--offline",
                "--manifest-path",
                str(manifest),
                "--target-dir",
                str(target),
            ],
            check=True,
            timeout=180,
        )
        exe = target / "debug" / ("ball-proof.exe" if os.name == "nt" else "ball-proof")
        output = subprocess.run(
            [str(exe), str(a.payload.resolve())],
            input="".join(json.dumps(v) + "\n" for v in cases),
            text=True,
            capture_output=True,
            check=True,
            timeout=180,
        )
        actual = [json.loads(s) for s in output.stdout.splitlines()]
    if len(actual) != len(cases):
        raise AssertionError("missing Rust results")
    for i, (v, result) in enumerate(zip(cases, actual)):
        expected = (
            sessions[i] if i in sessions else plays[i] if i in plays else oracle.eval(v)
        )
        if i in plays and result != expected:
            for t, (x, y) in enumerate(zip(result, expected)):
                if x != y:
                    raise AssertionError(
                        f"play stage {v[1]} frame {t} keys {v[3][t]}: {x} != {y}"
                    )
            raise AssertionError("length mismatch")
        if v[0] == "shot" and result[1]:
            result[0] = (
                None  # the impact animation immediately takes ownership of screen y
            )
        if i in sessions and result != expected:
            for k, (x, y) in enumerate(zip(result[2], expected[2])):
                if x != y:
                    raise AssertionError(
                        f"session {v[1]}, board change {k}, Rust {x} vs native {y}"
                    )
            raise AssertionError(
                f"session {v[1]} totals {result[0][-1]}, {expected[0][-1]} or extra board change"
            )
        if result != expected:
            raise AssertionError(f"{v[0]} mismatch, case {i}: {result} != {expected}")
    from collections import Counter

    print(
        json.dumps(
            {
                "payload_sha256": DIGEST,
                "matched": Counter(v[0] for v in cases),
                "total": len(cases),
                "sessions": [e[0][-1] for e in sessions.values()],
                "stage_traces": [[len(t), t[-1][7], t[-1][-1]] for t in plays.values()],
            }
        )
    )


if __name__ == "__main__":
    main()
