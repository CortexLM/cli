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

# Security-advisory exceptions. A patched release published inside the
# seven-day window is allowed only while no older release carries the fix,
# and only until the release ages out on its own. Each entry is temporary:
# delete it once `expires` passes. The general age rule is not weakened, and
# yanked releases are never excused here.
ADVISORY_EXCEPTIONS = {
    ("rustls", "0.23.45"): {
        "advisory": "RUSTSEC-2026-0285",
        "alias": "GHSA-2mjx-qc3c-rqvc",
        "expires": "2026-09-21",
    },
}

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

def advisory_exception(name, version, now):
    """Return the live advisory exception for a release, or None.

    The caller checks `yanked` first: this only excuses the age rule, and only
    until the exception's expiry date.
    """
    entry = ADVISORY_EXCEPTIONS.get((name, version))
    if entry is None:
        return None
    expires = datetime.fromisoformat(entry["expires"]).replace(tzinfo=timezone.utc)
    return entry if now < expires else None

def run(base):
    base = subprocess.check_output(["git", "-C", str(ROOT), "rev-parse", "--verify", f"{base}^{{commit}}"], text=True).strip()
    previous = subprocess.check_output(["git", "-C", str(ROOT), "show", f"{base}:Cargo.lock"], text=True)
    added = registry_packages(tomllib.loads((ROOT / "Cargo.lock").read_text())) - registry_packages(tomllib.loads(previous))
    now = datetime.now(timezone.utc)
    failures = []
    excused = []
    for name, version in sorted(added):
        request = Request(
            f"https://crates.io/api/v1/crates/{name}/{version}",
            headers={"User-Agent": "CortexLM-cli-dependency-policy (github.com/CortexLM/cli)"},
        )
        # Fail closed when registry evidence is unavailable. Never substitute now.
        with urlopen(request, timeout=30) as response:
            release = json.load(response)["version"]
        if release["yanked"]:
            failures.append(f"{name}@{version}: yanked or younger than seven days")
            continue
        if not old_enough(release["created_at"], now):
            exception = advisory_exception(name, version, now)
            if exception is None:
                failures.append(f"{name}@{version}: yanked or younger than seven days")
            else:
                excused.append(
                    f"{name}@{version}: younger than seven days, allowed by "
                    f"{exception['advisory']} ({exception['alias']}) until {exception['expires']}"
                )
        time.sleep(1)
    for line in excused:
        print(f"advisory exception: {line}")
    print("\n".join(failures) or f"Release-age policy passed for {len(added)} newly locked releases")
    return int(bool(failures))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    raise SystemExit(run(parser.parse_args().base))
