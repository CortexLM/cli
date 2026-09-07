#!/usr/bin/env python3
"""Summarize allowlisted local events; never upload events or open issues."""

import argparse
from collections import Counter, defaultdict
import json
import math
from pathlib import Path
import re

OPERATIONS = {
    "cli.command", "cli.interactive", "cli.debug", "server.request",
    "session.created", "session.deleted", "server.started", "health.check",
}
ERROR_PERCENT = 5
P95_MILLISECONDS = 2000
MIN_REQUESTS = 20

def summarize(events):
    versions = defaultdict(lambda: {"operations": Counter(), "requests": 0, "errors": 0, "durations": []})
    for event in events:
        if event.get("schema") != 1 or event.get("operation") not in OPERATIONS:
            raise ValueError("Unknown local diagnostic event schema or operation")
        version = event.get("version", "")
        if not isinstance(version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise ValueError("Invalid application version")
        status, duration = event.get("status"), event.get("duration_ms")
        if type(status) is not int or not 100 <= status <= 599 or type(duration) is not int or duration < 0:
            raise ValueError("Invalid numeric diagnostic fields")
        group = versions[version]
        group["operations"][event["operation"]] += 1
        if event["operation"] in {"server.request", "cli.command", "cli.debug", "cli.interactive"}:
            group["requests"] += 1
            group["errors"] += status >= 500
        if event["operation"] == "server.request":
            group["durations"].append(duration)
    result = {}
    for version, group in sorted(versions.items()):
        durations = sorted(group.pop("durations"))
        p95 = durations[math.ceil(len(durations) * .95) - 1] if durations else None
        group["p95_request_ms"] = p95
        group["alerts"] = []
        if group["requests"] >= MIN_REQUESTS and group["errors"] * 100 >= ERROR_PERCENT * group["requests"]:
            group["alerts"].append("error_rate")
        if len(durations) >= MIN_REQUESTS and p95 > P95_MILLISECONDS:
            group["alerts"].append("request_latency")
        result[version] = group
    return result

def load(directory):
    events = []
    for path in sorted(directory.glob("run-*.jsonl")):
        if path.is_symlink() or not path.is_file() or path.stat().st_size > 2 * 1024 * 1024:
            raise ValueError("Unsafe or oversized local diagnostics file")
        for line in path.read_text().splitlines():
            events.append(json.loads(line))
    if not events:
        raise ValueError("No local events found, cannot claim a healthy deployment")
    return events

def run(directory):
    report = summarize(load(directory))
    print(json.dumps({
        "local_only": True,
        "thresholds": {"minimum_samples": MIN_REQUESTS, "error_percent": ERROR_PERCENT, "p95_request_ms": P95_MILLISECONDS},
        "versions": report,
        "next_step": "For alerts, follow docs/guides/operations.md. Reproduce with local QA before drafting a redacted issue. Never attach raw sessions or logs.",
    }, indent=2))
    return int(any(group["alerts"] for group in report.values()))

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    raise SystemExit(run(parser.parse_args().directory))
