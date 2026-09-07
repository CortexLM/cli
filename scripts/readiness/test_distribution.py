"""Exercise local installer fixtures; never use a live release or user prefix."""

import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import threading
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[2]
VERSION = "9.8.7"
ASSETS = {
    "linux-x86_64": "cortex-cli-linux-x64",
    "linux-aarch64": "cortex-cli-linux-arm64",
    "linux-x86_64-musl": "cortex-cli-linux-x64-static",
    "linux-aarch64-musl": "cortex-cli-linux-arm64-static",
    "darwin-x86_64": "cortex-cli-macos-x64",
    "darwin-aarch64": "cortex-cli-macos-arm64",
    "windows-x86_64": "cortex-cli-windows-x64",
}


def archive(entries):
    data = io.BytesIO()
    with tarfile.open(fileobj=data, mode="w:gz") as package:
        for name, body, kind in entries:
            info = tarfile.TarInfo(name)
            info.type = kind
            info.mode = 0o755
            info.size = len(body) if kind == tarfile.REGTYPE else 0
            if kind == tarfile.SYMTYPE:
                info.linkname = "/tmp/not-a-cortex-binary"
            package.addfile(info, io.BytesIO(body))
    return data.getvalue()


@unittest.skipIf(os.name == "nt", "POSIX shell installer; PowerShell tested separately")
class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cortex-distribution-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.prefix = self.root / "prefix with spaces"
        self.bindir = self.prefix / "bin"
        self.bindir.mkdir(parents=True)
        self.old = self.bindir / "Cortex"
        self.old.write_bytes(b"previous binary")
        self.requests = []
        self.payload = archive([
            ("Cortex", f"#!/bin/sh\nprintf 'Cortex {VERSION}\\n'\n".encode(), tarfile.REGTYPE)
        ])
        self.platform = "linux-x86_64"
        self.response_status = 200
        self.chunked = False
        fixture = self

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                fixture.requests.append(self.path)
                if self.path.endswith("manifest.json"):
                    body = json.dumps({"stable": fixture.release}).encode()
                elif self.path.endswith(".json"):
                    body = json.dumps(fixture.release).encode()
                else:
                    body = fixture.payload
                self.send_response(fixture.response_status)
                if fixture.response_status == 302:
                    self.send_header("Location", fixture.origin + "/redirected")
                if not fixture.chunked:
                    self.send_header("Content-Length", str(len(body)))
                self.end_headers()
                try:
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, *_):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.addCleanup(self.server.server_close)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.thread.join, 5)
        self.addCleanup(self.server.shutdown)
        self.origin = f"http://127.0.0.1:{self.server.server_port}"
        self.release = {"version": VERSION, "channel": "stable", "assets": {}}
        self.set_asset()
        tools = self.root / "tools"
        tools.mkdir()
        self.tools = tools
        self.write_tool("uname", 'case "$1" in -s) echo Linux;; -m) echo x86_64;; esac')
        self.write_tool("getconf", "echo 'glibc 2.39'")
        self.write_tool("ldd", "echo fixture-no-libc; exit 1")
        scratch = self.root / "scratch"
        scratch.mkdir()
        # Do not inherit tokens, application configuration, or proxy settings.
        self.env = {
            "HOME": str(self.root),
            "PATH": os.pathsep.join([str(tools), os.environ["PATH"]]),
            "TMPDIR": str(scratch),
            "CORTEX_INSTALL_DIR": str(self.prefix),
            "CORTEX_SOFTWARE_URL": self.origin,
            "CORTEX_VERSION": VERSION,
        }

    def write_tool(self, name, body):
        path = self.tools / name
        path.write_text("#!/bin/sh\n" + body + "\n")
        path.chmod(0o755)

    def set_asset(self):
        self.release["assets"] = {
            self.platform: {
                "url": f"{self.origin}/v1/assets/{self.platform}/{VERSION}/cortex.tar.gz",
                "sha256": hashlib.sha256(self.payload).hexdigest(),
                "size": len(self.payload),
            }
        }

    def install(self, success):
        result = subprocess.run(["sh", str(ROOT / "scripts/install.sh")], env=self.env,
                                text=True, capture_output=True, timeout=30)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(self.old.read_bytes(), b"previous binary")
        self.assertEqual(list((self.root / "scratch").iterdir()), [])
        self.assertEqual(list(self.bindir.glob(".cortex-install-*")), [])
        return result

    def test_verified_install_retains_old_binary_and_aliases(self):
        self.install(True)
        self.assertEqual((self.bindir / "Cortex.old").read_bytes(), b"previous binary")
        self.assertEqual(os.readlink(self.bindir / "cortex"), "Cortex")
        self.assertEqual(os.readlink(self.bindir / "agent"), "Cortex")

    def test_channel_manifest_install(self):
        del self.env["CORTEX_VERSION"]
        self.install(True)
        self.assertIn("/releases/manifest.json", self.requests)

    def test_sha_mismatch_never_executes_or_replaces(self):
        self.release["assets"][self.platform]["sha256"] = "0" * 64
        self.assertIn("SHA-256 mismatch", self.install(False).stderr)
        self.assertFalse((self.bindir / "Cortex.old").exists())

    def test_wrong_binary_version_preserves_previous(self):
        self.payload = archive([("Cortex", b"#!/bin/sh\necho Cortex 0.0.0\n", tarfile.REGTYPE)])
        self.set_asset()
        self.install(False)

    def test_post_install_failure_rolls_back(self):
        self.payload = archive([("Cortex", (
            f'#!/bin/sh\ncase "$0" in */.cortex-install-*/Cortex) echo Cortex {VERSION};; *) exit 1;; esac\n'
        ).encode(), tarfile.REGTYPE)])
        self.set_asset()
        self.install(False)

    def test_missing_binary_and_unsafe_archive_members_fail(self):
        for name, kind in [("../Cortex", tarfile.REGTYPE), ("/Cortex", tarfile.REGTYPE),
                           ("Cortex", tarfile.SYMTYPE), ("other", tarfile.REGTYPE)]:
            with self.subTest(name=name, kind=kind):
                self.payload = archive([(name, b"not executable", kind)])
                self.set_asset()
                self.install(False)

    def test_extra_archive_member_is_rejected(self):
        self.payload = archive([("Cortex", b"not executable", tarfile.REGTYPE),
                                ("extra", b"unexpected", tarfile.REGTYPE)])
        self.set_asset()
        self.install(False)

    def test_truncated_archive_is_rejected(self):
        self.payload = b"not a gzip archive"
        self.set_asset()
        self.install(False)

    def test_size_bound_with_and_without_content_length(self):
        for chunked in (False, True):
            with self.subTest(chunked=chunked):
                self.chunked = chunked
                self.release["assets"][self.platform]["size"] = 8
                self.install(False)

    def test_release_metadata_is_validated_before_archive_download(self):
        for key, value in [("sha256", "not-a-sha"), ("size", 536870913),
                           ("url", "https://unconfigured.invalid/cortex.tar.gz")]:
            with self.subTest(key=key):
                self.set_asset()
                self.requests.clear()
                self.release["assets"][self.platform][key] = value
                self.install(False)
                self.assertFalse(any("/v1/assets/" in item for item in self.requests))

    def test_release_version_and_channel_must_match(self):
        for key, value in [("version", "1.0.0"), ("channel", "beta")]:
            with self.subTest(key=key):
                original = self.release[key]
                self.release[key] = value
                self.install(False)
                self.release[key] = original

    def test_redirects_are_not_followed(self):
        self.response_status = 302
        self.install(False)
        self.assertNotIn("/redirected", self.requests)

    def test_unsupported_os_arch_and_libc_fail_before_network(self):
        cases = [("FreeBSD", "x86_64", True), ("Linux", "riscv64", True),
                 ("Linux", "x86_64", False)]
        for system, arch, glibc in cases:
            with self.subTest(system=system, arch=arch, glibc=glibc):
                self.write_tool("uname", f'case "$1" in -s) echo {system};; -m) echo {arch};; esac')
                self.write_tool("getconf", "exit 0" if glibc else "exit 1")
                self.install(False)
                self.assertEqual(self.requests, [])

    def test_musl_and_darwin_asset_selection(self):
        for system, arch, platform in [("Linux", "aarch64", "linux-aarch64-musl"),
                                       ("Darwin", "arm64", "darwin-aarch64")]:
            with self.subTest(system=system):
                self.write_tool("uname", f'case "$1" in -s) echo {system};; -m) echo {arch};; esac')
                self.write_tool("getconf", "exit 1")
                self.write_tool("ldd", "echo musl; exit 1")
                self.platform = platform
                self.set_asset()
                self.install(True)
                self.assertTrue(any(f"/{platform}/" in item for item in self.requests))

    def test_existing_unrelated_command_is_not_overwritten(self):
        agent = self.bindir / "agent"
        agent.write_text("unrelated command")
        self.install(False)
        self.assertEqual(agent.read_text(), "unrelated command")

    def test_existing_binary_symlink_is_not_followed(self):
        other = self.root / "other"
        other.write_bytes(b"previous binary")
        self.old.unlink()
        self.old.symlink_to(other)
        self.install(False)
        self.assertTrue(self.old.is_symlink())
        self.assertEqual(other.read_bytes(), b"previous binary")


class PackageContractTests(unittest.TestCase):
    def test_release_matrix_and_publish_mapping_match(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
        matrix = workflow["jobs"]["build-cli"]["strategy"]["matrix"]["include"]
        self.assertEqual({row["artifact"] for row in matrix}, set(ASSETS.values()))
        publish = (ROOT / ".github/workflows/publish-r2.yml").read_text()
        for platform, artifact in ASSETS.items():
            self.assertIn(f"{platform}:{artifact}", publish)
            self.assertIn(f'"{platform}":', publish)

    def test_publish_preparation_and_metadata_against_exact_local_assets(self):
        if os.name == "nt" or not shutil.which("jq"):
            self.skipTest("Release packaging fixture needs POSIX bash and jq")
        workflow = yaml.safe_load((ROOT / ".github/workflows/publish-r2.yml").read_text())
        steps = {step.get("name"): step for step in workflow["jobs"]["publish"]["steps"]}
        with tempfile.TemporaryDirectory(prefix="cortex-package-fixture-") as directory:
            root = Path(directory)
            bodies = {}
            for platform, artifact in ASSETS.items():
                extension = "zip" if platform.startswith("windows") else "tar.gz"
                path = root / "artifacts" / artifact / f"{artifact}.{extension}"
                path.parent.mkdir(parents=True)
                # Publication copies opaque archives; this fixture tests exact bytes,
                # platform mapping, digest and size rather than binary execution.
                bodies[platform] = artifact.encode()
                path.write_bytes(bodies[platform])
            environment = {
                "PATH": os.environ["PATH"], "HOME": directory,
                "VERSION": VERSION, "CHANNEL": "stable", "RELEASE_NOTES": "fixture",
                "REPOSITORY": "CortexLM/cli", "PUBLIC_HOST": "https://software.cortex.foundation",
            }
            for name in ("Verify artifacts exist", "Prepare archives and checksums",
                         "Generate release JSON and update manifest"):
                result = subprocess.run(["bash", "-eu", "-c", steps[name]["run"]],
                                        cwd=root, env=environment, text=True,
                                        capture_output=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            release = json.loads((root / "release.json").read_text())
            self.assertEqual(set(release["assets"]), set(ASSETS))
            for platform, asset in release["assets"].items():
                extension = "zip" if platform.startswith("windows") else "tar.gz"
                self.assertEqual(asset["url"], f"https://software.cortex.foundation/v1/assets/{platform}/{VERSION}/cortex.{extension}")
                self.assertEqual(asset["sha256"], hashlib.sha256(bodies[platform]).hexdigest())
                self.assertEqual(asset["size"], len(bodies[platform]))

    def test_package_manager_names_match_published_artifacts(self):
        homebrew = (ROOT / ".github/workflows/homebrew.yml").read_text()
        winget = (ROOT / ".github/workflows/winget.yml").read_text()
        self.assertIn("cortex-cli-macos-arm64.tar.gz", homebrew)
        self.assertIn("cortex-cli-windows-x64.zip", winget)
        self.assertNotIn("Architecture: arm64", winget)
        self.assertNotIn("aka.ms/wingetcreate/latest", winget)
        self.assertIn("RelativeFilePath: Cortex.exe", winget)

    def test_all_release_builds_are_locked_and_static_check_fails(self):
        workflow = yaml.safe_load((ROOT / ".github/workflows/release.yml").read_text())
        steps = workflow["jobs"]["build-cli"]["steps"]
        for step in steps:
            if "cargo +nightly build" in step.get("run", ""):
                self.assertIn("build --locked", step["run"])
        static = next(step["run"] for step in steps if step.get("name") == "Verify static binary")
        self.assertIn("readelf -l", static)
        self.assertIn("readelf -d", static)
        self.assertIn("exit 1", static)

    def test_posix_shell_syntax(self):
        if os.name == "nt":
            self.skipTest("POSIX shell check runs on Linux and macOS")
        subprocess.run(["sh", "-n", str(ROOT / "scripts/install.sh")], check=True)

    def test_powershell_installer_ast_and_architecture_cases(self):
        powershell = shutil.which("pwsh") or shutil.which("powershell")
        if powershell is None:
            self.skipTest("PowerShell runtime unavailable; native acceptance remains required")
        subprocess.run([powershell, "-NoProfile", "-NonInteractive", "-File",
                        str(ROOT / "scripts/test-install-ps1.ps1")], check=True, timeout=60)


if __name__ == "__main__":
    unittest.main()
