#!/usr/bin/env python3
"""Bound the native fixture check and retain a stack sample if its child stalls."""
from pathlib import Path
import subprocess
import sys


def main():
    bundle = Path(sys.argv[1]).resolve()
    logs = bundle.parent / "ui-check"
    logs.mkdir(exist_ok=True)
    log = logs / "native-ui.log"
    with log.open("w") as output:
        process = subprocess.Popen(
            [str(bundle / "Contents/MacOS/taskbar-rs"), "--benchmark-ui", "10"],
            stdout=output, stderr=subprocess.STDOUT,
        )
        try:
            result = process.wait(timeout=60)
        except subprocess.TimeoutExpired:
            print("Native UI check timed out. Sample the owned fixture process.", flush=True)
            try:
                subprocess.run(
                    ["/usr/bin/sample", str(process.pid), "3", "-file", str(logs / "timeout.sample.txt")],
                    timeout=10, check=False,
                )
            finally:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            result = 1
    print(log.read_text(), end="")
    return 1 if result else 0


if __name__ == "__main__":
    raise SystemExit(main())
