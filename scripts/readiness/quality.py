#!/usr/bin/env python3
"""Source-quality reports with a no-new-debt gate against a Git base."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[2]
MAX_BYTES = 5 * 1024 * 1024
MAX_LINES = 1000
MAX_COMPLEXITY = 25
MIN_CLONE_TOKENS = 100
FLAG_REGISTRIES = {
    "src/cortex-experimental/src/registry.rs",
    "src/cortex-engine/src/features.rs",
}


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args])


def source_paths():
    paths = git("ls-files", "--cached", "--others", "--exclude-standard", "-z")
    return sorted({
        p.decode() for p in paths.split(b"\0")
        if p and not p.startswith(b".git/") and (ROOT / p.decode()).is_file()
    })


def rust_metrics(path, text):
    import lizard
    from lizard_languages.rust import RustReader

    result = {}
    occurrences = Counter()
    lines = text.splitlines(keepends=True)
    for function in lizard.analyze_file.analyze_source_code(path, text).function_list:
        # Trait implementations can have identical signatures in the same file.
        # Keep every occurrence rather than silently overwriting a measurement.
        occurrences[function.long_name] += 1
        key = f"{path}:{function.long_name}#{occurrences[function.long_name]}"
        tokens = [
            t for t in RustReader.generate_tokens(
                "".join(lines[function.start_line - 1:function.end_line])
            )
            if t.strip() and not t.startswith(("//", "/*"))
        ]
        # Compare complete bodies, not signatures or common short boilerplate.
        body = tokens[tokens.index("{"):] if "{" in tokens else []
        fingerprint = None
        if len(body) >= MIN_CLONE_TOKENS:
            fingerprint = hashlib.sha256(
                json.dumps(body, separators=(",", ":")).encode()
            ).hexdigest()
        result[key] = {
            "path": path,
            "line": function.start_line,
            "complexity": function.cyclomatic_complexity,
            "clone": fingerprint,
        }
    return result


def clones(metrics):
    groups = {}
    for key, item in metrics.items():
        if item["clone"]:
            groups.setdefault(item["clone"], set()).add(key)
    return {key: members for key, members in groups.items() if len(members) > 1}


def quality_findings(current, previous):
    findings = []
    for key, item in current.items():
        old = previous.get(key, {}).get("complexity", MAX_COMPLEXITY)
        if item["complexity"] > MAX_COMPLEXITY:
            findings.append({
                "kind": "complexity", "path": item["path"], "line": item["line"],
                "message": f"{key}: complexity {item['complexity']} (target {MAX_COMPLEXITY})",
                "regression": item["complexity"] > max(MAX_COMPLEXITY, old),
            })
    old_clones = clones(previous)
    for fingerprint, members in clones(current).items():
        added = members - old_clones.get(fingerprint, set())
        for key in sorted(members):
            item = current[key]
            findings.append({
                "kind": "duplication", "path": item["path"], "line": item["line"],
                "message": f"Duplicate function body: {key}; also {', '.join(sorted(members - {key}))}",
                "regression": key in added,
            })
    return findings

def flag_inventory(sources):
    """Conservative literal-use analysis; dynamic registrations need review."""
    from lizard_languages.rust import RustReader

    definitions, consumers = {}, set()
    for path, text in sources.items():
        # Built-in registries and their consumers put test modules at the end.
        # Test-only references must not keep a retired production flag alive.
        production = re.split(r"#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]", text, maxsplit=1)[0]
        tokens = " ".join(
            t for t in RustReader.generate_tokens(production)
            if t.strip() and not t.startswith(("//", "/*"))
        )
        if path in FLAG_REGISTRIES:
            for name in re.findall(r'Feature\s*::\s*new\s*\(\s*"([^"]+)"', tokens):
                definitions[f"{path}:{name}"] = (path, name)
        else:
            consumers.update(re.findall(
                r'(?:is_enabled|is_feature_enabled)\s*\(\s*"([^"]+)"', tokens
            ))
    return {
        key: {"path": path, "name": name}
        for key, (path, name) in definitions.items() if name not in consumers
    }

def flag_findings(current, previous):
    old = flag_inventory(previous)
    return [
        {
            "kind": "unused-feature-flag", "path": item["path"], "line": 1,
            "message": f"{item['name']}: no production literal consumer; remove it or document and test its dynamic consumer",
            "regression": key not in old,
        }
        for key, item in flag_inventory(current).items()
    ]


def dependency_tables(manifest):
    for section in ("dependencies", "dev-dependencies", "build-dependencies"):
        yield section, manifest.get(section, {})
    for target, manifest in manifest.get("target", {}).items():
        for section in ("dependencies", "dev-dependencies", "build-dependencies"):
            yield f"target.{target}.{section}", manifest.get(section, {})


def dependency_findings(root_manifest, manifests, exceptions):
    findings = []
    shared = root_manifest["workspace"]["dependencies"]
    used = set()
    for path, manifest in manifests.items():
        for section, dependencies in dependency_tables(manifest):
            for name, declaration in dependencies.items():
                if isinstance(declaration, dict) and declaration.get("workspace") is True:
                    continue
                package = declaration.get("package", name) if isinstance(declaration, dict) else name
                if package not in shared:
                    continue
                # Renamed internal crates retain their alias in consumers. Their
                # package version is already inherited from this workspace.
                canonical = shared[package]
                if isinstance(declaration, dict) and isinstance(canonical, dict):
                    local = declaration.get("path")
                    shared_path = canonical.get("path")
                    if local and shared_path and "version" not in declaration:
                        if (ROOT / path).parent.joinpath(local).resolve() == (ROOT / shared_path).resolve():
                            continue
                key = f"{path}:{section}:{name}"
                exception = exceptions.get(key)
                if exception and exception.get("declaration") == declaration and exception.get("reason"):
                    used.add(key)
                    continue
                findings.append(f"{key}: inherit the workspace dependency or document an exact compatibility constraint")
    for key in exceptions.keys() - used:
        findings.append(f"{key}: unused or stale dependency exception")
    return findings


def check_dependencies():
    root = tomllib.loads((ROOT / "Cargo.toml").read_text())
    manifests = {
        f"{member}/Cargo.toml": tomllib.loads((ROOT / member / "Cargo.toml").read_text())
        for member in root["workspace"]["members"]
    }
    path = ROOT / ".quality/dependency-compatibility.json"
    exceptions = json.loads(path.read_text()) if path.exists() else {}
    return dependency_findings(root, manifests, exceptions)


def local_links(text):
    return re.findall(r"\[[^\]]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)", text)


def check_agents():
    path = ROOT / "AGENTS.md"
    failures = []
    for link in local_links(path.read_text()):
        if "://" in link or link.startswith("#"):
            continue
        target = (path.parent / link.split("#", 1)[0]).resolve()
        if not target.is_relative_to(ROOT) or not target.exists():
            failures.append(f"AGENTS.md: broken or out-of-repository link: {link}")
    for script in re.findall(r"\./(scripts/[\w/.-]+)", path.read_text()):
        if not (ROOT / script).is_file():
            failures.append(f"AGENTS.md: missing command {script}")
    return failures


def annotation(level, path, line, message):
    # GitHub workflow-command escaping, including filenames controlled by a PR.
    def escape(text):
        return str(text).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A").replace(",", "%2C").replace(":", "%3A")
    print(f"::{level} file={escape(path)},line={line}::{escape(message)}")


def run(base, output):
    base = git("rev-parse", "--verify", f"{base}^{{commit}}").decode().strip()
    previous_paths = set(git("ls-tree", "-r", "--name-only", base).decode().splitlines())
    metrics, previous, findings = {}, {}, []
    sources, old_sources = {}, {}
    for path in source_paths():
        data = (ROOT / path).read_bytes()
        before = git("show", f"{base}:{path}") if path in previous_paths else b""
        if len(data) > MAX_BYTES:
            findings.append({
                "kind": "file-size", "path": path, "line": 1,
                "message": f"File is {len(data)} bytes (limit {MAX_BYTES})",
                "regression": True,
            })
        if not path.endswith(".rs"):
            continue
        line_count = len(data.splitlines())
        if line_count > MAX_LINES:
            findings.append({
                "kind": "file-lines", "path": path, "line": 1,
                "message": f"Rust file has {line_count} lines (target {MAX_LINES})",
                "regression": line_count > max(MAX_LINES, len(before.splitlines())),
            })
        sources[path] = data.decode()
        metrics.update(rust_metrics(path, sources[path]))
        if before:
            old_sources[path] = before.decode()
            previous.update(rust_metrics(path, old_sources[path]))
    for path in sorted(previous_paths - sources.keys()):
        if path.endswith(".rs"):
            old_sources[path] = git("show", f"{base}:{path}").decode()
            previous.update(rust_metrics(path, old_sources[path]))
    findings.extend(quality_findings(metrics, previous))
    findings.extend(flag_findings(sources, old_sources))
    failures = check_dependencies() + check_agents()
    output.mkdir(parents=True, exist_ok=True)
    report = {
        "base": base, "functions_analyzed": len(metrics),
        "thresholds": {"complexity": MAX_COMPLEXITY, "source_lines": MAX_LINES, "file_bytes": MAX_BYTES, "clone_tokens": MIN_CLONE_TOKENS},
        "findings": findings, "policy_failures": failures,
    }
    (output / "quality.json").write_text(json.dumps(report, indent=2) + "\n")
    for finding in findings:
        if finding["regression"]:
            annotation("error", finding["path"], finding["line"], finding["message"])
    for failure in failures:
        annotation("error", "Cargo.toml" if "dependency" in failure else "AGENTS.md", 1, failure)
    regressions = sum(f["regression"] for f in findings)
    debt = len(findings) - regressions
    print(f"Analyzed {len(metrics)} Rust functions: {regressions} regressions, {len(failures)} policy failures, {debt} existing findings (see quality.json).")
    return int(bool(regressions or failures))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="origin/main", help="Git baseline; CI must supply the PR base commit")
    parser.add_argument("--output", type=Path, default=ROOT / "target/readiness")
    args = parser.parse_args()
    sys.exit(run(args.base, args.output))
