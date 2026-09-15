from datetime import datetime, timedelta, timezone
import unittest

from release_age import ADVISORY_EXCEPTIONS, advisory_exception, old_enough, registry_packages

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

    def test_rustls_exception_is_narrow_and_expires(self):
        # Only the patched release for the advisory is excused.
        inside = datetime(2026, 9, 16, tzinfo=timezone.utc)
        entry = advisory_exception("rustls", "0.23.45", inside)
        self.assertIsNotNone(entry)
        self.assertEqual(entry["advisory"], "RUSTSEC-2026-0285")
        self.assertEqual(entry["alias"], "GHSA-2mjx-qc3c-rqvc")
        self.assertEqual(entry["expires"], "2026-09-21")
        # The unpatched release is never excused.
        self.assertIsNone(advisory_exception("rustls", "0.23.44", inside))
        # Another crate is unaffected.
        self.assertIsNone(advisory_exception("serde", "0.23.45", inside))

    def test_rustls_exception_stops_applying_after_its_expiry(self):
        expiry = datetime(2026, 9, 21, tzinfo=timezone.utc)
        # The day it expires it is no longer honoured: the release must have
        # aged out on its own by then.
        self.assertIsNone(advisory_exception("rustls", "0.23.45", expiry))
        self.assertIsNone(
            advisory_exception("rustls", "0.23.45", expiry + timedelta(seconds=1))
        )
        self.assertIsNotNone(
            advisory_exception("rustls", "0.23.45", expiry - timedelta(seconds=1))
        )

    def test_every_exception_names_an_advisory_and_an_expiry(self):
        for (name, version), entry in ADVISORY_EXCEPTIONS.items():
            self.assertTrue(name and version, (name, version))
            self.assertTrue(entry["advisory"].startswith("RUSTSEC-"), entry)
            self.assertTrue(entry["alias"].startswith("GHSA-"), entry)
            # A timezone-aware expiry parses; an exception must not be open ended.
            datetime.fromisoformat(entry["expires"]).replace(tzinfo=timezone.utc)
