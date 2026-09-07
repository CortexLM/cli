import unittest
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest.mock import patch

from coverage import changed_lines, evaluate, parse_lcov

class CoverageTests(unittest.TestCase):
    def test_changed_lines_not_deleted_lines(self):
        diff = "+++ b/src/cortex-cli/a.rs\n@@ -1 +1,2 @@\n@@ -8,2 +9,0 @@"
        self.assertEqual(changed_lines(diff), {"src/cortex-cli/a.rs": {1, 2}})

    def test_real_threshold_cannot_round_up(self):
        path = "src/cortex-cli/src/example.rs"
        lines = {i: int(i < 8) for i in range(10)}
        self.assertTrue(evaluate({path: lines}, {path: set(lines)})["passed"])
        lines[7] = 0
        self.assertFalse(evaluate({path: lines}, {path: set(lines)})["passed"])

    def test_lcov_merges_counts_and_rejects_empty_data(self):
        path = "src/cortex-cli/example.rs"
        report = f"SF:{path}\nDA:1,0\nend_of_record\nSF:{path}\nDA:1,2\n"
        self.assertEqual(parse_lcov(report)[path][1], 2)
        with self.assertRaises(ValueError):
            parse_lcov("")

    def test_missing_production_coverage_fails_but_harnesses_are_not_gated(self):
        paths = [
            "src/cortex-cli/src/missing.rs",
            "src/cortex-cli/tests/check.rs",
            "src/cortex-app-server/examples/export-schema.rs",
        ]
        with TemporaryDirectory() as directory:
            root = Path(directory)
            for path in paths:
                source = root / path
                source.parent.mkdir(parents=True, exist_ok=True)
                source.write_text("fn uncovered() {}\n")
            with patch("coverage.ROOT", root):
                result = evaluate({}, {path: {1} for path in paths})
        self.assertFalse(result["passed"])
        self.assertEqual(result["absent_files"], paths[:1])
