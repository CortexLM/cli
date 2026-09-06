import unittest

from quality import check_agents, dependency_findings, flag_findings, quality_findings, rust_metrics


class QualityTests(unittest.TestCase):
    def test_rust_complexity_counts_branches(self):
        metrics = rust_metrics("sample.rs", "fn choose(a: bool, b: bool) { if a && b { run(); } }")
        self.assertEqual(len(metrics), 1)
        self.assertGreaterEqual(next(iter(metrics.values()))["complexity"], 3)

    def test_new_complexity_fails_but_reduction_does_not(self):
        item = {"path": "a.rs", "line": 1, "complexity": 30, "clone": None}
        current = {"a.rs:run()": item}
        self.assertTrue(quality_findings(current, {})[0]["regression"])
        previous = {"a.rs:run()": dict(item, complexity=31)}
        self.assertFalse(quality_findings(current, previous)[0]["regression"])
        previous["a.rs:run()"]["complexity"] = 29
        self.assertTrue(quality_findings(current, previous)[0]["regression"])

    def test_new_duplicate_is_detected(self):
        body = " ".join(f"work({i});" for i in range(40))
        a = rust_metrics("a.rs", f"fn first() {{ {body} }}")
        b = rust_metrics("b.rs", f"fn second() {{ {body} }}")
        findings = quality_findings(a | b, a)
        self.assertEqual(len([f for f in findings if f["kind"] == "duplication"]), 2)
        self.assertTrue(all(f["regression"] for f in findings))
        self.assertFalse(any(f["regression"] for f in quality_findings(a | b, a | b)))

    def test_small_boilerplate_is_not_a_clone(self):
        a = rust_metrics("a.rs", "fn first() { true }")
        b = rust_metrics("b.rs", "fn second() { true }")
        self.assertFalse(quality_findings(a | b, {}))

    def test_repeated_trait_method_signatures_are_not_lost(self):
        metrics = rust_metrics("a.rs", """
impl A { fn run(&self) { first(); } }
impl B { fn run(&self) { if condition() { second(); } } }
""")
        self.assertEqual(len(metrics), 2)
        self.assertEqual(sorted(f["complexity"] for f in metrics.values()), [1, 2])

    def test_renamed_dependencies_cannot_bypass_version_policy(self):
        root = {"workspace": {"dependencies": {"serde": "1"}}}
        manifests = {"a": {"dependencies": {"serde_old": {"package": "serde", "version": "0.9"}}}}
        self.assertTrue(dependency_findings(root, manifests, {}))

    def test_renamed_internal_crate_must_point_to_workspace_package(self):
        root = {"workspace": {"dependencies": {"shared": {"path": "src/shared"}}}}
        manifests = {"src/app/Cargo.toml": {"dependencies": {"shared_ext": {"package": "shared", "path": "../shared"}}}}
        self.assertEqual(dependency_findings(root, manifests, {}), [])
        manifests["src/app/Cargo.toml"]["dependencies"]["shared_ext"]["path"] = "../other"
        self.assertTrue(dependency_findings(root, manifests, {}))

    def test_dependency_overrides_are_rejected(self):
        root = {"workspace": {"dependencies": {"serde": "1"}}}
        self.assertTrue(dependency_findings(root, {"a": {"dependencies": {"serde": "2"}}}, {}))
        self.assertFalse(dependency_findings(root, {"a": {"dependencies": {"serde": {"workspace": True}}}}, {}))

    def test_exact_compatibility_exception_cannot_hide_new_drift(self):
        root = {"workspace": {"dependencies": {"rand": "0.9"}}}
        manifests = {"a": {"dependencies": {"rand": "0.8"}}}
        exception = {"a:dependencies:rand": {"declaration": "0.8", "reason": "Older RNG API"}}
        self.assertFalse(dependency_findings(root, manifests, exception))
        manifests["a"]["dependencies"]["rand"] = "0.7"
        self.assertTrue(dependency_findings(root, manifests, exception))
        self.assertTrue(dependency_findings(root, {}, exception))

    def test_target_dependencies_are_checked(self):
        root = {"workspace": {"dependencies": {"libc": "0.2"}}}
        manifests = {"a": {"target": {"cfg(unix)": {"dependencies": {"libc": "0.1"}}}}}
        self.assertTrue(dependency_findings(root, manifests, {}))

    def test_agents_links_and_scripts_resolve(self):
        self.assertEqual(check_agents(), [])

    def test_flags_need_production_consumers_not_comments_or_tests(self):
        registry = "src/cortex-experimental/src/registry.rs"
        sources = {
            registry: 'Feature::new("retired", "Name", "Description");',
            "consumer.rs": '// flags.is_enabled("retired")\n#[cfg(test)] mod tests { flags.is_enabled("retired"); }',
        }
        self.assertTrue(flag_findings(sources, {})[0]["regression"])
        self.assertFalse(flag_findings(sources, sources)[0]["regression"])
        sources["consumer.rs"] = 'flags.is_enabled("retired");'
        self.assertEqual(flag_findings(sources, {}), [])

    def test_removing_the_last_flag_consumer_fails(self):
        registry = "src/cortex-experimental/src/registry.rs"
        before = {registry: 'Feature::new("flag", "Name", "Description");', "a.rs": 'flags.is_enabled("flag");'}
        self.assertTrue(flag_findings({registry: before[registry]}, before)[0]["regression"])


if __name__ == "__main__":
    unittest.main()
