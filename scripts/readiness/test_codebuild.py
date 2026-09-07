"""Public-safe CodeBuild CI wiring. No live AWS calls."""

import json
import re
import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[2]
DEPLOY = ROOT / "deploy/aws/codebuild"
WORKFLOWS = ROOT / ".github/workflows"

ACCOUNT_ID = re.compile(r"(?<![A-Za-z0-9])\d{12}(?![A-Za-z0-9])")
ACCESS_KEY = re.compile(r"AKIA[0-9A-Z]{16}")
SECRET_KEY = re.compile(r"(?i)aws_secret_access_key\s*[:=]\s*\S+")
HARDCODED_ROLE = re.compile(r"arn:aws:iam::\d{12}:role/")
PLACEHOLDER = ("ACCOUNT_ID", "REGION", "${AWS::AccountId}", "${AWS::Region}")

REQUIRED_WORKFLOWS = {
    "ci.yml",
    "release.yml",
    "publish-r2.yml",
    "homebrew.yml",
    "winget.yml",
    "version-bump.yml",
    "test-stability.yml",
    "codebuild.yml",
}

CI_JOBS = {
    "quality",
    "version-check",
    "fmt",
    "clippy",
    "test",
    "coverage",
    "tui",
    "audit",
    "ci-success",
}


def read(path):
    return path.read_text()


class CodeBuildPublicSafetyTests(unittest.TestCase):
    def setUp(self):
        self.workflow = yaml.safe_load(read(WORKFLOWS / "codebuild.yml"))
        self.buildspec = yaml.safe_load(read(DEPLOY / "buildspec-ci.yml"))
        self.trust = json.loads(read(DEPLOY / "iam-trust-policy.json"))
        self.gha_policy = json.loads(read(DEPLOY / "iam-gha-permissions.json"))
        self.template = read(DEPLOY / "cloudformation.yaml")

    def test_existing_workflows_are_not_removed(self):
        names = {path.name for path in WORKFLOWS.glob("*.yml")}
        self.assertTrue(REQUIRED_WORKFLOWS <= names)

    def test_ci_yml_required_jobs_remain(self):
        ci = yaml.safe_load(read(WORKFLOWS / "ci.yml"))
        self.assertEqual(CI_JOBS, set(ci["jobs"]))
        self.assertEqual(
            ["version-check", "fmt", "clippy", "test", "tui", "audit", "quality", "coverage"],
            ci["jobs"]["ci-success"]["needs"],
        )

    def test_workflow_uses_oidc_and_not_access_keys(self):
        text = read(WORKFLOWS / "codebuild.yml")
        self.assertIn("id-token: write", text)
        self.assertIn("aws-actions/configure-aws-credentials@", text)
        self.assertIn("role-to-assume: ${{ vars.AWS_CODEBUILD_ROLE_ARN }}", text)
        self.assertIn("aws-actions/aws-codebuild-run-build@", text)
        self.assertNotIn("AWS_ACCESS_KEY_ID", text)
        self.assertNotIn("AWS_SECRET_ACCESS_KEY", text)
        self.assertNotIn("secrets.AWS_", text)
        self.assertIsNone(ACCESS_KEY.search(text))
        self.assertIsNone(HARDCODED_ROLE.search(text))

    def test_status_check_names_match_backend_style_projects(self):
        contexts = {row["context"] for row in self.workflow["jobs"]["codebuild"]["strategy"]["matrix"]["include"]}
        self.assertEqual(contexts, {"cortex-cli-gha-x64", "cortex-cli-gha-arm64"})
        text = read(WORKFLOWS / "codebuild.yml")
        self.assertIn("STATUS_CONTEXT", text)
        self.assertIn('context="${STATUS_CONTEXT}"', text)
        self.assertIn("state=pending", text)
        self.assertIn("state=success", text)
        self.assertIn("state=failure", text)

    def test_start_build_is_skipped_without_role_variable(self):
        gate = read(WORKFLOWS / "codebuild.yml")
        self.assertIn("AWS_CODEBUILD_ROLE_ARN is unset", gate)
        self.assertIn("enabled=false", gate)
        self.assertIn("needs.wiring.outputs.enabled == 'true'", gate)
        self.assertIn("same_repo", gate)

    def test_buildspec_caches_cargo_and_runs_real_gates(self):
        cache = "\n".join(self.buildspec["cache"]["paths"])
        self.assertIn(".cargo/registry", cache)
        self.assertIn("target/", cache)
        runner = read(DEPLOY / "run-ci.sh")
        for token in (
            "./scripts/clippy.sh",
            "scripts/readiness/tests.py",
            "cargo test --locked --workspace --doc",
            "scripts/readiness/schema.py",
            "scripts/readiness/qa.py",
            "scripts/readiness/coverage.py",
            "cortex-tui-syntax",
            "QUALITY_BASE",
        ):
            self.assertIn(token, runner)
        self.assertIn("exit 1", runner)
        self.assertNotIn("exit 0\n# mock", runner)

    def test_iam_templates_use_placeholders_and_cli_trust_only(self):
        trust_text = read(DEPLOY / "iam-trust-policy.json")
        perm_text = read(DEPLOY / "iam-gha-permissions.json")
        self.assertIn("repo:CortexLM/cli:*", trust_text)
        self.assertIn("ACCOUNT_ID", trust_text)
        self.assertIn("sts:AssumeRoleWithWebIdentity", trust_text)
        self.assertIn("cortex-cli-gha-x64", perm_text)
        self.assertIn("cortex-cli-gha-arm64", perm_text)
        self.assertEqual(
            {"codebuild:StartBuild", "codebuild:BatchGetBuilds"},
            set(self.gha_policy["Statement"][0]["Action"]),
        )
        self.assertEqual(["logs:GetLogEvents"], self.gha_policy["Statement"][1]["Action"])
        for text in (trust_text, perm_text, self.template):
            leftover = ACCOUNT_ID.search(text)
            if leftover:
                start = max(0, leftover.start() - 40)
                window = text[start:leftover.end() + 40]
                self.assertTrue(
                    any(token in window for token in PLACEHOLDER),
                    f"bare account id in {window!r}",
                )
            self.assertIsNone(ACCESS_KEY.search(text))
            self.assertIsNone(SECRET_KEY.search(text))

    def test_cloudformation_projects_and_oidc_role(self):
        # CloudFormation tags (!Sub / !Ref) are not PyYAML constructors; assert text.
        self.assertIn("Type: AWS::CodeBuild::Project", self.template)
        self.assertIn("ARM_CONTAINER", self.template)
        self.assertIn("LINUX_CONTAINER", self.template)
        self.assertIn("Type: NO_SOURCE", self.template)
        self.assertIn("Type: S3", self.template)
        self.assertIn("BUILD_GENERAL1_LARGE", self.template)
        self.assertIn("repo:${GitHubOrgRepo}:*", self.template)
        self.assertIn("token.actions.githubusercontent.com", self.template)
        self.assertIn("cortex-cli-codebuild-gha", self.template)
        self.assertNotIn("R2_", self.template)
        self.assertNotIn("WORKOS", self.template)

    def test_docs_name_variables_not_secret_values(self):
        docs = read(DEPLOY / "README.md") + read(ROOT / "docs/CI_SECRETS.md")
        self.assertIn("AWS_CODEBUILD_ROLE_ARN", docs)
        self.assertIn("cortex-cli-gha-x64", docs)
        self.assertIn("repo:CortexLM/cli:*", docs)
        self.assertIn("CLI_CODEBUILD_CI_READY", docs)
        self.assertIn("Windows", docs)
        self.assertNotIn("AKIA", docs)
        self.assertIsNone(HARDCODED_ROLE.search(docs))
