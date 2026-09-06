from pathlib import Path
import tempfile
import unittest

from tests import parse_junit, summarize

class ReportTests(unittest.TestCase):
    def test_failed_then_passed_is_flaky_and_fails(self):
        runs = [
            {"exit_code": code, "tests": [{"name": "case", "status": status}]}
            for code, status in [(100, "fail"), (0, "pass")]
        ]
        result = summarize(runs)
        self.assertEqual(result["flaky_tests"], ["case"])
        self.assertTrue(result["failed"])

    def test_build_failure_without_tests_fails(self):
        self.assertTrue(summarize([{"exit_code": 1, "tests": []}])["failed"])

    def test_junit_timings_and_failure_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "junit.xml"
            path.write_text('<testsuites><testsuite><testcase classname="app" name="case" time="1.5"><failure/></testcase></testsuite></testsuites>')
            self.assertEqual(parse_junit(path), [{"name": "app::case", "status": "fail", "seconds": 1.5}])
            path.write_text("<testsuites/>")
            with self.assertRaises(ValueError):
                parse_junit(path)
