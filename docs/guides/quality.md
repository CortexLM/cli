# Source and dependency policy

```bash
export PATH="$PWD/target/readiness-tools/bin:$PATH"
python -B -m unittest discover -s scripts/readiness -p 'test_*.py'
python scripts/readiness/quality.py --base origin/main
cargo machete --with-metadata
python scripts/readiness/release_age.py --base origin/main
python scripts/readiness/schema.py
```

CI supplies the actual PR base SHA (or previous push SHA), not the current
checkout, and fetches history. Never change the baseline to the current commit
to conceal regressions.

## Enforced controls

- Lizard 1.17.31 analyzes Rust functions. The complexity target is 25.
- Rust source files target 1,000 lines; all repository files have a 5 MiB cap.
- Duplicate complete function bodies of at least 100 tokens are reported.
- Existing complexity/line-count/duplicate debt is retained in
  `target/readiness/quality.json`. New or worsened findings fail. This is a
  regression gate, not a claim that the existing tree is debt-free.
- Built-in feature registries are compared to production literal consumers.
  Test-only references and comments do not keep a flag alive. Removing the last
  consumer or adding an unconsumed flag fails. Existing candidates remain visible.
  Dynamic/custom feature consumers need explicit review; lexical analysis cannot
  prove that a flag is dead in every external caller.
- New flags need an owner, intended lifetime, rollout/removal decision, and tests
  in their PR. Review inactive flags during weekly maintenance; remove abandoned
  declarations rather than adding dummy consumers.
- Cargo-machete 0.9.1 checks unused direct dependencies.
- Dependencies already centralized in the workspace must be inherited.
  Renamed internal crates must point to the same workspace path. Exact,
  justified older-API constraints live in `.quality/dependency-compatibility.json`.
  Changed or unused exceptions fail. This preserves compatibility rather than
  pretending every major-version migration has been validated.
- Newly locked crates.io versions must be at least seven days old and not yanked.
  Unavailable registry evidence fails closed. Existing locked versions are not
  re-dated. Git dependencies stay pinned by `Cargo.lock`; review their provenance
  separately. Dependabot also has a seven-day Cargo update cooldown.
- Local links and referenced scripts in `AGENTS.md` must exist.
- Generated CLI/API contracts must match their Rust sources.

For an urgent security upgrade younger than seven days, maintainers must review
a narrow policy change with an advisory reference. Do not bypass the gate, alter
registry dates, or remove `--locked`.

The existing formatting, Clippy, audit, version, and TUI gates remain required.
CI Success also depends on source policy and changed-line coverage. Test and
coverage artifacts are retained for 14–30 days, not sent to a third-party service.
