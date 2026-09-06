# Maintenance and ownership

`CODEOWNERS` names a verified repository administrator. Ownership changes need
the new owner's agreement. Main requires a pull request and at least one
independent approval; the author cannot satisfy their own approval requirement.
Do not bypass that rule to publish readiness work.

## Weekly issue triage

Use the existing type labels (`bug`, `enhancement`, `documentation`,
`dependencies`). Add one priority and at least one area after investigating:

| Priority | Meaning |
| --- | --- |
| `priority:p0` | Active security/data-loss incident |
| `priority:p1` | Major user workflow blocked |
| `priority:p2` | Normal planned work |
| `priority:p3` | Low-impact improvement |

Areas: `area:cli`, `area:tui`, `area:server`, `area:build`, `area:security`,
and `area:docs`. Do not assign a priority solely to satisfy a coverage metric.
Check new reports within two working days. Revisit issues without a maintainer
update for 30 days; close only with an explanation or a verified resolution,
not automatically because they are old. Link reproductions, owning code, and
fixes. Do not create artificial issues when the backlog is empty.

## Weekly engineering/release review

- Review source-quality artifacts and prioritize a real complexity, duplication,
  or large-file hotspot. No-new-debt controls do not eliminate existing debt.
- Review feature-flag candidates. Decide whether to implement a tested consumer,
  remove a retired flag, or document a genuine dynamic consumer.
- Inspect slowest/flaky-test reports. Reproduce and fix failures; never add
  ignored tests or successful retries to conceal them.
- Review Dependabot and advisory findings. Preserve lockfiles and minimum-age
  policy; security exceptions require evidence and review.
- Review actual completed release work. Use the existing release workflow when
  there is a useful, tested release, not a calendar-driven empty version bump.

Backlog health and deployment frequency are historical outcomes. A taxonomy,
runbook, or empty backlog is not proof that those outcomes have improved.
