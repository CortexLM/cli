import unittest

from insights import summarize

def event(**values):
    return {"schema":1, "version":"0.1.7", "operation":"server.request", "status":200, "duration_ms":10, **values}

class InsightTests(unittest.TestCase):
    def test_error_and_latency_alerts_require_real_samples(self):
        events = [event() for _ in range(19)]
        events.append(event(status=500))
        self.assertEqual(summarize(events)["0.1.7"]["alerts"], ["error_rate"])
        self.assertEqual(summarize(events[:19])["0.1.7"]["alerts"], [])
        self.assertIn("request_latency", summarize([event(duration_ms=3000) for _ in range(20)])["0.1.7"]["alerts"])

    def test_versions_remain_separate_and_private_fields_are_omitted(self):
        result = summarize([event(prompt="private"), event(version="0.1.8", status=500)])
        self.assertEqual(result["0.1.7"]["errors"], 0)
        self.assertEqual(result["0.1.8"]["errors"], 1)
        self.assertNotIn("private", str(result))

    def test_unknown_events_cannot_disclose_arbitrary_text(self):
        for bad in [event(operation="private"), event(version="private"), event(status="private")]:
            with self.assertRaises(ValueError):
                summarize([bad])
