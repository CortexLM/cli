#!/usr/bin/env python3
"""Enforce 80% line coverage on changed executable application/shared lines."""

import argparse
import json
from pathlib import Path
import re

from quality import ROOT, git

# Gate production sources, not test harnesses or schema-export examples.
APPS = ("src/cortex-cli/src/", "src/cortex-app-server/src/", "src/cortex-common/src/")
MIN_PERCENT = 80

def parse_lcov(text):
    files, current = {}, None
    for line in text.splitlines():
        if line.startswith("SF:"):
            path = Path(line[3:])
            current = str(path.relative_to(ROOT)) if path.is_absolute() else str(path)
            files.setdefault(current, {})
        elif line.startswith("DA:") and current:
            number, count, *_ = line[3:].split(",")
            values = files[current]
            values[int(number)] = values.get(int(number), 0) + int(count)
    if not files:
        raise ValueError("Coverage report contains no source files")
    return files

def changed_lines(diff):
    result, path = {}, None
    for line in diff.splitlines():
        if line.startswith("+++ b/"):
            path = line[6:]
        elif line.startswith("+++ "):
            path = None
        elif path and line.startswith("@@ "):
            match = re.search(r"\+(\d+)(?:,(\d+))? @@", line)
            if match:
                start = int(match[1])
                count = int(match[2] or 1)
                result.setdefault(path, set()).update(range(start, start + count))
    return result

def evaluate(files, changes):
    covered, total, missing, absent = 0, 0, [], []
    for path, lines in changes.items():
        if not path.startswith(APPS) or not path.endswith(".rs"):
            continue
        executable = files.get(path, {})
        if not executable and (ROOT / path).exists():
            text = (ROOT / path).read_text()
            if re.search(r"\bfn\s+\w+", text):
                absent.append(path)
        for line in sorted(lines & executable.keys()):
            total += 1
            if executable[line] > 0:
                covered += 1
            else:
                missing.append(f"{path}:{line}")
    return {
        "covered": covered, "executable_changed_lines": total,
        "required_percent": MIN_PERCENT, "uncovered": missing, "absent_files": absent,
        "passed": not absent and (not total or covered * 100 >= MIN_PERCENT * total),
    }

def run(base, report):
    base = git("rev-parse", "--verify", f"{base}^{{commit}}").decode().strip()
    changes = changed_lines(git("diff", "--no-ext-diff", "--unified=0", base, "--", *APPS).decode())
    for path in git("ls-files", "--others", "--exclude-standard").decode().splitlines():
        if path.startswith(APPS) and path.endswith(".rs"):
            changes[path] = set(range(1, len((ROOT / path).read_text().splitlines()) + 1))
    result = evaluate(parse_lcov(report.read_text()), changes)
    output = ROOT / "target/readiness/coverage.json"
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return int(not result["passed"])

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--report", type=Path, default=ROOT / "target/readiness/lcov.info")
    args = parser.parse_args()
    raise SystemExit(run(args.base, args.report))
