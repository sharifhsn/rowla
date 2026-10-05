#!/usr/bin/env python3
"""Compile only the dependency-free preview policy tests; no Cargo/AppKit/GUI."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory(prefix="taskbar-preview-policy-") as temporary:
    directory = Path(temporary)
    source = directory / "check.rs"
    # Rust raw strings preserve paths containing spaces without shell quoting.
    source.write_text(
        f'#[path = r#"{root / "src/ui/preview_lifecycle.rs"}"#]\n'
        'mod preview_lifecycle;\n'
    )
    binary = directory / "check"
    subprocess.run([
        "rustc", "--edition=2024", "--test", "-C", "codegen-units=1",
        str(source), "-o", str(binary),
    ], check=True, timeout=90)
    subprocess.run([str(binary), "--test-threads=1"], check=True, timeout=10)
