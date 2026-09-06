#!/usr/bin/env python3
"""Run real workspace tests, retain timings, and fail on any failed repetition."""

import argparse
from collections import defaultdict
import json
from pathlib import Path
import shutil
import subprocess
import time
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[2]

def parse_junit(path):
    tests = []
    for case in ET.parse(path).iter("testcase"):
        status = "pass"
        if case.find("skipped") is not None:
            status = "skip"
        if case.find("failure") is not None or case.find("error") is not None:
            status = "fail"
        tests.append({
            "name": f"{case.get('classname', '')}::{case.get('name', '')}",
            "seconds": float(case.get("time", "0")),
            "status": status,
        })
    if not tests or not any(t["status"] != "skip" for t in tests):
        raise ValueError("Test runner produced no executed tests")
    return tests

def summarize(runs):
    observations = defaultdict(set)
    for run in runs:
        for test in run["tests"]:
            observations[test["name"]].add(test["status"])
    flaky = sorted(name for name, states in observations.items() if {"pass", "fail"} <= states)
    failed = any(run["exit_code"] != 0 or any(t["status"] == "fail" for t in run["tests"]) for run in runs)
    return {"runs": runs, "flaky_tests": flaky, "failed": failed}

def run(repeat, packages):
    output = ROOT / "target/readiness/tests"
    output.mkdir(parents=True, exist_ok=True)
    runs = []
    cargo_args = ["--workspace"] if not packages else [part for p in packages for part in ("-p", p)]
    for index in range(1, repeat + 1):
        report = ROOT / "target/nextest/ci/junit.xml"
        # Remove only the generated report, never consume a stale successful run.
        report.unlink(missing_ok=True)
        start = time.monotonic()
        completed = subprocess.run(
            ["cargo", "nextest", "run", "--locked", "--profile", "ci", *cargo_args],
            cwd=ROOT, check=False,
        )
        tests = []
        error = None
        if report.exists():
            shutil.copyfile(report, output / f"junit-{index}.xml")
            try:
                tests = parse_junit(report)
            except (ET.ParseError, ValueError) as exc:
                error = str(exc)
        else:
            error = "Missing JUnit report (build or runner failure)"
        runs.append({
            "iteration": index, "exit_code": completed.returncode or int(error is not None),
            "wall_seconds": time.monotonic() - start, "tests": tests, "report_error": error,
        })
    result = summarize(runs)
    (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    slow = sorted((t for r in runs for t in r["tests"]), key=lambda t: t["seconds"], reverse=True)[:20]
    markdown = ["# Test performance", "", f"Repetitions: {repeat}", f"Flaky tests: {len(result['flaky_tests'])}", "", "| Test | Seconds | Result |", "| --- | ---: | --- |"]
    markdown.extend(f"| {t['name'].replace('|', '/')} | {t['seconds']:.3f} | {t['status']} |" for t in slow)
    (output / "summary.md").write_text("\n".join(markdown) + "\n")
    return int(result["failed"])

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repeat", type=int, choices=range(1, 11), default=1)
    parser.add_argument("-p", "--package", action="append", default=[])
    args = parser.parse_args()
    raise SystemExit(run(args.repeat, args.package))
