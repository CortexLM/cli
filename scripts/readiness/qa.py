#!/usr/bin/env python3
"""Exercise the built CLI and a real isolated loopback server, including DAST."""

import argparse
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time
from urllib.error import HTTPError, URLError
from urllib.request import HTTPRedirectHandler, ProxyHandler, Request, build_opener

ROOT = Path(__file__).resolve().parents[2]

class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None

def check(condition, message):
    if not condition:
        raise AssertionError(message)

def cli_flow(binary, env, home):
    command = [str(binary), "debug", "doctor", "--json"]
    output = subprocess.run(command, cwd=home, env=env, capture_output=True, text=True, timeout=20)
    check(output.returncode == 0, "CLI local readiness failed")
    report = json.loads(output.stdout)
    check(report["ready"] is True and report["coding_service"] == "not_checked", "CLI misreported local scope")
    (home / "config.toml").write_text("invalid = [")
    output = subprocess.run(command, cwd=home, env=env, capture_output=True, text=True, timeout=20)
    check(output.returncode != 0, "CLI accepted invalid configuration")
    check(json.loads(output.stdout)["checks"]["configuration"] is False, "CLI missed the configuration error")
    (home / "config.toml").unlink()
    return ["cli.local_readiness", "cli.invalid_configuration"]

def server_flow(binary, env, workspace, output):
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    key = secrets.token_urlsafe(32)
    env = {**env, "CORTEX_SERVER_API_KEY": key}
    config = workspace / "server.json"
    config.write_text(json.dumps({
        "listen_addr": f"127.0.0.1:{port}",
        "max_body_size": 4096,
        "rate_limit": {"burst_size": 100},
    }))
    opener = build_opener(ProxyHandler({}), NoRedirect())
    base = f"http://127.0.0.1:{port}/api/v1"

    def call(method, path, body=None, authenticated=True, headers=None):
        values = {"Content-Type": "application/json"}
        if authenticated:
            values["Authorization"] = f"ApiKey {key}"
        values.update(headers or {})
        request = Request(base + path, method=method, headers=values,
                          data=json.dumps(body).encode() if body is not None else None)
        try:
            response = opener.open(request, timeout=5)
        except HTTPError as error:
            response = error
        with response:
            data = response.read(1024 * 1024)
            headers = {key.lower(): value for key, value in response.headers.items()}
            body = json.loads(data) if data and headers.get("content-type", "").startswith("application/json") else None
            return response.status, headers, body

    with (output / "server.log").open("w") as log:
        process = subprocess.Popen([str(binary), "--config", str(config), "--json-logs"],
                                   env=env, cwd=workspace, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 20
            while True:
                check(process.poll() is None, "Local server exited before becoming ready")
                try:
                    status, _, body = call("GET", "/health", authenticated=False)
                    if status == 200:
                        check(body["status"] == "ready", "Server did not report local readiness")
                        break
                except (URLError, ConnectionError, TimeoutError):
                    pass
                check(time.monotonic() < deadline, "Local server readiness timed out")
                time.sleep(.1)
            for path in ["/sessions", "/metrics", "/admin/stats", "/ws", "/health/sessions"]:
                check(call("GET", path, authenticated=False)[0] == 401, "Authentication boundary failed")
            check(call("GET", "/sessions", headers={"Authorization": "ApiKey invalid-fixture"})[0] == 401, "Invalid key was accepted")
            status, headers, session = call("POST", "/sessions", {"model": "local-qa"})
            check(status == 200, "Session creation failed")
            check(any(name.lower() == "x-request-id" for name in headers), "Missing request correlation")
            check(any(name.lower() == "traceparent" for name in headers), "Missing local trace context")
            path = f"/sessions/{session['id']}"
            check(call("POST", path + "/messages", {"content": "local QA fixture"})[0] == 200, "Message storage failed")
            check(call("GET", path + "/messages")[2][0]["content"] == "local QA fixture", "Stored message changed")
            check(call("GET", path)[2]["message_count"] == 1, "Session count did not update")
            check(call("DELETE", path)[2]["deleted"] is True, "Session deletion failed")
            check(call("GET", path)[0] == 404, "Deleted session remains visible")
            check(call("POST", "/sessions", {"model": "x" * 8192})[0] == 413, "Body size limit is not enforced")
            check(call("GET", "/sessions", headers={"Origin": "https://untrusted.example"})[1].get("access-control-allow-origin") is None, "CORS allowed an unknown origin")
            check(call("POST", "/files/read", {"path": "../outside-fixture.txt"})[0] == 403, "Read escaped the workspace")
            check(call("POST", "/files/write", {"path": "../new/outside.txt", "content": "fixture"})[0] == 400, "Write escaped the workspace")
            if os.name == "posix":
                (workspace / "escape").symlink_to(workspace.parent, target_is_directory=True)
                check(call("POST", "/files/read", {"path": "escape/outside-fixture.txt"})[0] == 403, "Symlink escaped the workspace")
            check(call("POST", "/files/mkdir", {"path": "fixture-dir"})[0] == 200, "Directory creation failed")
            check(call("POST", "/files/write", {"path": "fixture-dir/file", "content": "file fixture"})[0] == 200, "File write failed")
            check(call("POST", "/files/read", {"path": "fixture-dir/file"})[2]["content"] == "file fixture", "File readback failed")
            check(call("POST", "/files/mkdir", {"path": "../forbidden-dir"})[0] == 400, "Directory creation escaped the workspace")
            for source, target in [
                ("../outside-fixture.txt", "fixture-dir/stolen"),
                ("fixture-dir/file", "../stolen"),
                (".", "../moved-workspace"),
            ]:
                check(call("POST", "/files/rename", {"old_path": source, "new_path": target})[0] == 400, "Rename escaped or moved the workspace")
            check(call("POST", "/files/rename", {"old_path": "fixture-dir/file", "new_path": "fixture-dir/renamed"})[0] == 200, "Local rename failed")
            check(call("POST", "/files/delete", {"path": "fixture-dir/renamed"})[0] == 200, "Local deletion failed")
            check((workspace.parent / "outside-fixture.txt").exists(), "Security checks modified outside data")
            check(not (workspace.parent / "forbidden-dir").exists(), "Security checks created an outside directory")
            metrics = call("GET", "/metrics")[2]
            check(metrics["total_requests"] >= 15 and metrics["sessions_created"] == 1, "Metrics were not wired to live requests")
            check(call("GET", "/openapi.json")[2]["openapi"] == "3.1.0", "API schema unavailable")
        finally:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    return [
        "server.local_readiness", "server.authentication", "server.session_crud",
        "server.message_storage", "server.correlation", "server.metrics",
        "dast.body_limit", "dast.cors", "dast.workspace_traversal", "dast.symlink_escape",
        "server.file_crud", "dast.file_mutations",
    ]

def run(bin_dir):
    output = ROOT / "target/readiness/qa"
    output.mkdir(parents=True, exist_ok=True)
    cases = []
    with tempfile.TemporaryDirectory(prefix="cortex-local-qa-") as temporary:
        root = Path(temporary)
        home, workspace = root / "home", root / "workspace"
        home.mkdir()
        workspace.mkdir()
        (root / "outside-fixture.txt").write_text("private QA fixture")
        # Do not inherit credentials, proxies, user configuration, or mDNS settings.
        env = {key: os.environ[key] for key in ("PATH", "SYSTEMROOT", "WINDIR") if key in os.environ}
        env.update({
            "HOME": str(home), "CORTEX_HOME": str(home), "NO_COLOR": "1",
            "CORTEX_MDNS_ENABLED": "false",
            "CORTEX_DIAGNOSTICS_DIR": str(root / "diagnostics"),
        })
        try:
            cases.extend(cli_flow(bin_dir / "Cortex", env, home))
            cases.extend(server_flow(bin_dir / "cortex-server", env, workspace, output))
        except Exception:
            (output / "report.json").write_text(json.dumps({"passed": False, "completed_cases": cases}, indent=2) + "\n")
            raise
        # Validate the generated journal, but retain only aggregated, allowlisted data.
        from insights import load, summarize
        insights = summarize(load(root / "diagnostics"))
        (output / "local-insights.json").write_text(json.dumps(insights, indent=2) + "\n")
    (output / "report.json").write_text(json.dumps({"passed": True, "completed_cases": cases}, indent=2) + "\n")
    print(f"Passed {len(cases)} local functional/security cases. No model turn was simulated.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    run(parser.parse_args().bin_dir.resolve())
