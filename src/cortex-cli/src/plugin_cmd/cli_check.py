"""Offline product-path check: python3 cli_check.py /absolute/path/to/Cortex."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile


def check(binary):
    with tempfile.TemporaryDirectory(prefix="cortex-plugin-cli-check-") as temporary:
        root = Path(temporary)
        home = root / "home"
        home.mkdir()
        environment = {
            "HOME": str(home),
            "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
            "LANG": "C.UTF-8",
            "TERM": "dumb",
        }

        def run(*args, status=0, json_output=False):
            result = subprocess.run(
                [binary, "plugin", *args], cwd=root, env=environment,
                text=True, capture_output=True, timeout=30,
            )
            assert result.returncode == status, (args, result.returncode, result.stdout, result.stderr)
            return json.loads(result.stdout) if json_output else result.stdout

        run("new", "example", "--typescript", "--output", str(root / "projects"))
        source = root / "projects/example"
        invalid = run("validate", "--path", str(source), "--json", status=1, json_output=True)
        assert invalid["valid"] is False
        run("build", "--path", str(source))
        valid = run("validate", "--path", str(source), "--json", json_output=True)
        assert valid["valid"] is True and valid["executed"] is False
        run("install", str(source))
        denied = run("run", "example", "hello", "--json", status=1, json_output=True)
        assert denied["success"] is False
        run("trust", "example", "--yes")
        result = run("run", "example", "hello", "Ada", "--json", json_output=True)
        assert result["success"] is True
        assert "Hello, Ada!" in result["result"]["data"]["message"]
        assert result["result"]["notifications"][0]["message"] == "TypeScript plugin session started"
        result = run("run", "example", "--tool", "greet", "--input", '{"name":"  Lin  "}', "--json", json_output=True)
        assert result["result"]["data"]["greeting"] == "Hello, Lin!"
        veto = run("run", "example", "--tool", "greet", "--input", '{"name":"blocked"}', "--json", status=1, json_output=True)
        assert veto["success"] is False and "blocked" in veto["error"]
        run("run", "example", "--tool", "greet", "--input", '{"name":false}', "--json", status=1, json_output=True)
        archive = root / "example.tar.gz"
        run("publish", "--path", str(source), "--output", str(archive))
        run("remove", "example", "--yes")
        run("install", str(archive), "--trust-code")
        run("run", "example", "hello", "Roundtrip", "--json", json_output=True)
        installed = home / ".cortex/plugins/example"
        before = (installed / "dist/plugin.mjs").read_bytes()
        (source / "dist/plugin.mjs").write_text("invalid javascript !")
        run("update", "example", "--source", str(source), status=1)
        assert (installed / "dist/plugin.mjs").read_bytes() == before
        run("run", "example", "hello", "Retained", "--json", json_output=True)
        invalid = run("validate", "--path", str(source), "--json", status=1, json_output=True)
        assert invalid["valid"] is False
        run("validate", "--path", str(source), status=1)
        (source / "plugin.toml").write_text("invalid toml !")
        assert run("validate", "--path", str(source), "--json", status=1, json_output=True)["valid"] is False
        run("remove", "../outside", "--yes", status=1)
        run("disable", "example")
        assert run("run", "example", "hello", "--json", status=1, json_output=True)["success"] is False
        listed = run("list", "--json", json_output=True)
        assert listed[0]["enabled"] is False
        run("enable", "example")
        assert run("run", "example", "hello", "--json", json_output=True)["success"] is True
        run("remove", "example", "--yes")
        assert run("list", "--json", json_output=True) == []
    print("Plugin CLI checks passed: scaffold/build/validate/trust/run/hooks/veto/package roundtrip/rollback/disable")


if __name__ == "__main__":
    assert len(sys.argv) == 2 and Path(sys.argv[1]).is_absolute(), "Pass an absolute Cortex binary path"
    check(sys.argv[1])
