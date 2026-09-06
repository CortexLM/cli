#!/usr/bin/env python3
"""Reject newly locked crates.io releases younger than seven days."""

import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import subprocess
import time
import tomllib
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
MIN_AGE = timedelta(days=7)

def registry_packages(lock):
    return {
        (p["name"], p["version"]) for p in lock["package"]
        if p.get("source") == "registry+https://github.com/rust-lang/crates.io-index"
    }

def old_enough(created_at, now):
    created = datetime.fromisoformat(created_at.replace("Z", "+00:00"))
    if created.tzinfo is None:
        raise ValueError("Registry timestamp is missing its timezone")
    return now - created >= MIN_AGE

def run(base):
    base = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--verify", f"{base}^{{commit}}"], text=True).strip()
    previous = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{base}:Cargo.lock"], text=True)
    added = registry_packages(tomllib.loads((ROOT / "Cargo.lock").read_text())) - registry_packages(tomllib.loads(previous))
    now = datetime.now(timezone.utc)
    failures = []
    for name, version in sorted(added):
        request = Request(
            f"https://crates.io/api/v1/crates/{name}/{version}",
            headers={"User-Agent": "CortexLM-cli-dependency-policy (github.com/CortexLM/cli)"},
        )
        # Fail closed when registry evidence is unavailable. Never substitute now.
        with urlopen(request, timeout=30) as response:
            release = json.load(response)["version"]
        if release["yanked"] or not old_enough(release["created_at"], now):
            failures.append(f"{name}@{version}: yanked or younger than seven days")
        time.sleep(1)
    print("\n".join(failures) or f"Release-age policy passed for {len(added)} newly locked releases")
    return int(bool(failures))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    raise SystemExit(run(parser.parse_args().base))
