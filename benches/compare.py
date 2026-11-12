"""Compare Rustine (Rust and Python) with Python Gelatin on the same inputs.

Each engine converts each input to JSON in its own process. Per engine and
workload the script reports instructions (Valgrind, Linux only; they do not
depend on the CPU), peak memory, CPU time and wall time.

    python benches/compare.py --rust target/release/examples/bench_run \
        --rustine .venv/bin/python --gelatin /usr/bin/python3.11 \
        --workload scale data/scale.gel data/scale-25mb.txt \
        --small fixtures/parity --instructions

Engines: --rust takes the `bench_run` example binary, --rustine and --gelatin
take a Python interpreter that can import Rustine or Gelatin. Large workloads
run once per process; the small fixtures are measured per conversion, with
startup and grammar compilation subtracted.
"""

import argparse
import os
import re
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

PY_MODULES = {"rustine": "Rustine.util", "gelatin": "Gelatin.util"}


def child(engine, grammar, input_path, repeat):
    """Compile `grammar` and convert `input_path` to JSON `repeat` times."""
    import importlib

    def read(path):
        # newline="" keeps CRLF, so all engines see the same bytes.
        with open(path, encoding="utf-8", newline="") as f:
            return f.read()

    util = importlib.import_module(PY_MODULES[engine])
    converter = util.compile_string(read(grammar))
    text = read(input_path)
    for _ in range(repeat):
        util.generate_string(converter, text, "json")


def command(engine, exe, grammar, input_path, repeat):
    if engine == "rust":
        return [exe, grammar, input_path, str(repeat), "--quiet"]
    return [exe, os.path.abspath(__file__), "--child", engine, grammar, input_path, str(repeat)]


def run(cmd):
    """Run `cmd`; return (wall s, CPU s, peak memory MB)."""
    start = time.perf_counter()
    proc = subprocess.Popen(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if os.name == "nt":
        proc.wait()
        wall = time.perf_counter() - start
        cpu, peak = windows_usage(proc)
    else:
        _, status, usage = os.wait4(proc.pid, 0)
        wall = time.perf_counter() - start
        proc.returncode = os.waitstatus_to_exitcode(status)
        cpu = usage.ru_utime + usage.ru_stime
        peak = usage.ru_maxrss / (1024 * 1024 if sys.platform == "darwin" else 1024)
    if proc.returncode:
        sys.exit(f"failed ({proc.returncode}): {' '.join(cmd)}")
    return wall, cpu, peak


def windows_usage(proc):
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("faults", wintypes.DWORD), ("peak", ctypes.c_size_t)]
        _fields_ += [(f"f{i}", ctypes.c_size_t) for i in range(7)]

    handle = wintypes.HANDLE(int(proc._handle))
    times = [wintypes.FILETIME() for _ in range(4)]
    ctypes.windll.kernel32.GetProcessTimes(handle, *[ctypes.byref(t) for t in times])
    cpu = sum((t.dwHighDateTime << 32 | t.dwLowDateTime) / 1e7 for t in times[2:])
    counters = Counters(cb=ctypes.sizeof(Counters))
    ctypes.windll.kernel32.K32GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb)
    return cpu, counters.peak / (1024 * 1024)


def instructions(cmd):
    # One Rayon thread (idle threads spin a varying number of instructions) and a fixed
    # Python hash seed make the count repeatable.
    env = {**os.environ, "RAYON_NUM_THREADS": "1", "PYTHONHASHSEED": "0"}
    out = subprocess.run(
        ["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", *cmd],
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.PIPE,
        text=True,
        errors="replace",
    )
    match = re.search(r"Collected : (\d+)", out.stderr)
    if out.returncode or not match:
        sys.exit(f"valgrind failed: {' '.join(cmd)}\n{out.stderr[-2000:]}")
    return int(match.group(1))


def measure_large(engine, exe, grammar, input_path, runs, count_instr):
    cmd = command(engine, exe, grammar, input_path, 1)
    run(cmd)  # warm-up
    results = [run(cmd) for _ in range(runs)]
    return {
        "wall": statistics.median(r[0] for r in results),
        "cpu": statistics.median(r[1] for r in results),
        "peak": max(r[2] for r in results),
        "instr": instructions(cmd) if count_instr else None,
    }


def measure_small(engine, exe, fixtures, runs, count_instr):
    """Per-conversion cost of all fixtures, without startup and compilation."""
    total = {"wall": 0.0, "cpu": 0.0, "peak": 0.0, "instr": 0 if count_instr else None}
    for grammar, input_path in fixtures:
        one = command(engine, exe, grammar, input_path, 1)
        run(one)  # warm-up
        startup = statistics.median(run(one)[0] for _ in range(3))
        repeat = 101
        while run(command(engine, exe, grammar, input_path, repeat))[0] - startup < 0.5 and repeat < 10**7:
            repeat *= 4
        many = command(engine, exe, grammar, input_path, repeat)
        base = [run(one) for _ in range(runs)]
        loop = [run(many) for _ in range(runs)]
        for key, idx in (("wall", 0), ("cpu", 1)):
            per = statistics.median(r[idx] for r in loop) - statistics.median(r[idx] for r in base)
            total[key] += max(per, 0.0) / (repeat - 1)
        total["peak"] = max(total["peak"], max(r[2] for r in loop))
        if count_instr:
            total["instr"] += (instructions(command(engine, exe, grammar, input_path, 11)) - instructions(one)) // 10
    return total


def fmt_instr(n):
    if n is None:
        return "–"
    for unit, size in (("G", 1e9), ("M", 1e6), ("K", 1e3)):
        if n >= size:
            return f"{n / size:.3g} {unit}"
    return str(n)


def fmt_time(s):
    if s < 1e-3:
        return f"{s * 1e6:.3g} µs"
    return f"{s * 1e3:.3g} ms" if s < 1 else f"{s:.3g} s"


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--child":
        child(sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]))
        return
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--rust", help="bench_run example binary")
    ap.add_argument("--rustine", help="Python interpreter with Rustine installed")
    ap.add_argument("--gelatin", help="Python interpreter with Gelatin installed")
    ap.add_argument("--workload", nargs=3, action="append", default=[], metavar=("NAME", "GRAMMAR", "INPUT"))
    ap.add_argument("--small", metavar="DIR", help="directory with */syntax1.gel + */input1.txt fixtures")
    ap.add_argument("--runs", type=int, default=5, help="timed runs per measurement (median)")
    ap.add_argument("--instructions", action="store_true", help="count instructions with Valgrind")
    args = ap.parse_args()
    if args.instructions and not shutil.which("valgrind"):
        sys.exit("--instructions needs valgrind")
    engines = [(n, e) for n, e in (("rust", args.rust), ("rustine", args.rustine), ("gelatin", args.gelatin)) if e]
    fixtures = []
    if args.small:
        for d in sorted(Path(args.small).iterdir()):
            if (d / "syntax1.gel").exists() and (d / "input1.txt").exists():
                fixtures.append((str(d / "syntax1.gel"), str(d / "input1.txt")))

    print("| Workload | Engine | Instructions | Peak memory | CPU time | Time |")
    print("|---|---|---|---|---|---|")
    for name, grammar, input_path in args.workload:
        for engine, exe in engines:
            r = measure_large(engine, exe, grammar, input_path, args.runs, args.instructions)
            print(f"| {name} | {engine} | {fmt_instr(r['instr'])} | {r['peak']:.0f} MB | "
                  f"{fmt_time(r['cpu'])} | {fmt_time(r['wall'])} |", flush=True)
    if fixtures:
        for engine, exe in engines:
            r = measure_small(engine, exe, fixtures, args.runs, args.instructions)
            print(f"| small ({len(fixtures)} files, per conversion) | {engine} | {fmt_instr(r['instr'])} | "
                  f"{r['peak']:.0f} MB | {fmt_time(r['cpu'])} | {fmt_time(r['wall'])} |", flush=True)


if __name__ == "__main__":
    main()
