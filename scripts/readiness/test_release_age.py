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
        self.assertEqual(entry["expires"], "2026-09-21T15:11:18+00:00")
        # The unpatched release is never excused.
        self.assertIsNone(advisory_exception("rustls", "0.23.44", inside))
        # Another crate is unaffected.
        self.assertIsNone(advisory_exception("serde", "0.23.45", inside))

    def test_rustls_exception_stops_applying_after_its_expiry(self):
        # Exception must cover the full seven-day age window (~15:11:17Z), not
        # midnight on the calendar day.
        just_before = datetime(2026, 9, 21, 15, 11, 17, tzinfo=timezone.utc)
        at_expiry = datetime(2026, 9, 21, 15, 11, 18, tzinfo=timezone.utc)
        self.assertIsNotNone(advisory_exception("rustls", "0.23.45", just_before))
        self.assertIsNone(advisory_exception("rustls", "0.23.45", at_expiry))
        self.assertIsNone(
            advisory_exception("rustls", "0.23.45", at_expiry + timedelta(seconds=1))
        )
        # Midnight on the expiry calendar day is still inside the window.
        midnight = datetime(2026, 9, 21, 0, 0, 0, tzinfo=timezone.utc)
        self.assertIsNotNone(advisory_exception("rustls", "0.23.45", midnight))

    def test_every_exception_names_an_advisory_and_an_expiry(self):
        for (name, version), entry in ADVISORY_EXCEPTIONS.items():
            self.assertTrue(name and version, (name, version))
            self.assertTrue(entry["advisory"].startswith("RUSTSEC-"), entry)
            self.assertTrue(entry["alias"].startswith("GHSA-"), entry)
            # A timezone-aware expiry parses; an exception must not be open ended.
            raw = entry["expires"]
            if "T" in raw:
                parsed = datetime.fromisoformat(raw.replace("Z", "+00:00"))
                self.assertIsNotNone(parsed.tzinfo or timezone.utc)
            else:
                datetime.fromisoformat(raw).replace(tzinfo=timezone.utc)
