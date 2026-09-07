import shlex
import tomllib
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]


class WorkflowTests(unittest.TestCase):
    def setUp(self):
        self.ci = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())

    def test_policy_job_installs_required_toolchain_components(self):
        toolchain = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]
        step = next(
            step for step in self.ci["jobs"]["quality"]["steps"]
            if step.get("uses", "").startswith("dtolnay/rust-toolchain@")
        )
        self.assertEqual(step["uses"], f"dtolnay/rust-toolchain@{toolchain['channel']}")
        components = {value.strip() for value in step["with"]["components"].split(",")}
        self.assertTrue(set(toolchain["components"]) <= components)

    def test_doctor_jobs_install_real_tools_before_tests(self):
        stability = yaml.safe_load(
            (ROOT / ".github/workflows/test-stability.yml").read_text()
        )
        jobs = [
            self.ci["jobs"]["test"],
            self.ci["jobs"]["coverage"],
            stability["jobs"]["repeat"],
        ]
        for job in jobs:
            installed = set()
            found_tests = False
            for step in job["steps"]:
                command = step.get("run", "")
                for line in command.splitlines():
                    if "apt-get install" in line:
                        installed.update(shlex.split(line))
                if "scripts/readiness/tests.py" in command or "cargo llvm-cov nextest" in command:
                    self.assertTrue({"git", "ripgrep"} <= installed)
                    found_tests = True
            self.assertTrue(found_tests)

    def test_coverage_creates_report_directory_before_export(self):
        commands = "\n".join(
            step.get("run", "") for step in self.ci["jobs"]["coverage"]["steps"]
        )
        lines = [line.strip() for line in commands.splitlines()]
        directory = lines.index("mkdir -p target/readiness")
        report = next(
            index for index, line in enumerate(lines)
            if "cargo llvm-cov nextest" in line
            and "--output-path target/readiness/lcov.info" in line
        )
        self.assertLess(directory, report)
