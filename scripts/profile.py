#!/usr/bin/env python3
"""Bounded local stress test. Saves counts and process memory, never screenshots."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument("--pid", type=int, required=True)
parser.add_argument("--count", type=int, default=3000)
parser.add_argument("--duration", type=int, default=660)
parser.add_argument("--label", default="soak")
parser.add_argument("--exe", type=Path)
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
out = root / "diagnostics"
out.mkdir(exist_ok=True)
exe = args.exe or root / "dist/Rowla.app/Contents/MacOS/taskbar-rs"
metrics = Path.home() / "Library/Application Support/Taskbar Rust/diagnostics.json"
benchmark_output = (out / f"capture-{args.label}.json").open("w") if args.count else None
benchmark = subprocess.Popen([str(exe), "--benchmark", str(args.count)], stdout=benchmark_output) if args.count else None
pids = [args.pid] + ([benchmark.pid] if benchmark else [])
for name in ("replayd", "WindowServer"):
    result = subprocess.run(["pgrep", "-x", name], capture_output=True, text=True)
    pids.extend(int(p) for p in result.stdout.split() if p.isdigit())
start = time.monotonic()
with (out / f"memory-{args.label}.jsonl").open("w") as f:
    while time.monotonic() - start < args.duration:
        try:
            app = json.loads(metrics.read_text())
            if app.get("pid") not in pids:
                pids.append(app["pid"])
        except (OSError, json.JSONDecodeError):
            app = None
        command = ["top", "-l", "1", "-n", "10", "-stats", "pid,command,mem,threads"]
        for pid in pids:
            command += ["-pid", str(pid)]
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        processes = []
        for line in result.stdout.splitlines():
            match = re.match(r"\s*(\d+)\s+(\S+)\s+(\S+)\s+(\S+)", line)
            if match and int(match[1]) in pids:
                processes.append({"pid": int(match[1]), "name": match[2],
                                  "physical_footprint": match[3], "threads": match[4]})
        record = {"elapsed": round(time.monotonic() - start, 2),
                  "benchmark_running": benchmark is not None and benchmark.poll() is None,
                  "processes": processes, "app": app}
        f.write(json.dumps(record) + "\n")
        f.flush()
        time.sleep(10)
if benchmark is not None and benchmark.poll() is None:
    benchmark.terminate()
    benchmark.wait(timeout=10)
    print("Capture test exceeded its duration limit")
elif benchmark is not None:
    print(f"Capture test exit={benchmark.returncode}; memory timeline saved")
if benchmark_output:
    benchmark_output.close()
