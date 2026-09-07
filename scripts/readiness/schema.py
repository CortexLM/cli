#!/usr/bin/env python3
"""Regenerate or verify the checked-in schema from the actual Rust API models."""

import argparse
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
SCHEMAS = [
    ("cortex-app-server", "export-schema", "app-server.openapi.json"),
    ("cortex-cli", "export-cli-schema", "cli.commands.json"),
]

def run(write):
    for package, example, filename in SCHEMAS:
        content = subprocess.check_output([
            "cargo", "run", "--locked", "--quiet", "-p", package,
            "--example", example,
        ], cwd=ROOT, text=True)
        schema = ROOT / "docs/reference" / filename
        if write:
            schema.parent.mkdir(parents=True, exist_ok=True)
            schema.write_text(content)
        elif not schema.exists() or schema.read_text() != content:
            raise SystemExit("API schema is stale: run python3 scripts/readiness/schema.py --write")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    run(parser.parse_args().write)
