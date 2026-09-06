from datetime import datetime, timedelta, timezone
import unittest

from release_age import old_enough, registry_packages

class ReleaseAgeTests(unittest.TestCase):
    def test_boundary_and_future_releases(self):
        now = datetime(2026, 1, 20, tzinfo=timezone.utc)
        self.assertTrue(old_enough((now - timedelta(days=7)).isoformat(), now))
        self.assertFalse(old_enough((now - timedelta(days=6)).isoformat(), now))
        self.assertFalse(old_enough((now + timedelta(days=1)).isoformat(), now))
        with self.assertRaises(ValueError):
            old_enough("2026-01-01T00:00:00", now)

    def test_only_crates_io_releases_are_queried(self):
        lock = {"package": [
            {"name": "local", "version": "1"},
            {"name": "crate", "version": "2", "source": "registry+https://github.com/rust-lang/crates.io-index"},
        ]}
        self.assertEqual(registry_packages(lock), {("crate", "2")})
