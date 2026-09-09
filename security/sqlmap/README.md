# sqlmap verification

The Rust runner drives the upstream scanner against a loopback HTTP adapter
and a disposable PostgreSQL container. The case manifest names the actual
pgorm API exercised by each protected route and its vulnerable control.

## Run locally

Use Docker, Rust (CI uses 1.97.1), a C compiler/libclang, and the exact Python
version in `pins.json`. The runner verifies the upstream archive digest and
uses the pinned PostgreSQL image. It accepts no external scan target.

```sh
python3 security/sqlmap/check.py
cargo build --locked --manifest-path security/sqlmap/adapter/Cargo.toml --bins
security/sqlmap/adapter/target/debug/sqlmap-harness --profile smoke
```

Initial full acceptance also requires the direct security regressions against
the same disposable server. Generate the root workspace lockfile first if it
does not exist; preserve it with the report for dependency reproduction.

```sh
cargo generate-lockfile
security/sqlmap/adapter/target/debug/sqlmap-harness \
  --profile full --direct-regressions --artifacts target/sqlmap/full-run
```

Artifact directories must be new. `--python PATH` selects the pinned Python
interpreter. `--case ID` runs a named subset; its result cannot stand for a
complete profile. `--baseline-only` is a diagnostic and always exits nonzero.
The Python `harness.py` remains available for comparison; CI and acceptance
use the Rust runner, which also runs and records the direct regressions.

To reproduce a report, check out its recorded source revision, restore any
recorded source changes and workspace lockfile, use the accompanying
`pins.json`, `cases.json` and `profiles.json`, then run the recorded profile
and subset into a fresh artifact directory. `command.json` files retain
individual scanner invocations; their loopback addresses belong to the old
fixture and must be replaced by a newly provisioned adapter.

## CI and evidence

`.github/workflows/sqlmap.yml` runs smoke on relevant pull requests and full
weekly or through **Run workflow**. Both call `sqlmap-run.yml`, which uses
the same local runner and profiles. Smoke and full have separate check
names. Fast runner tests do not need Docker or a live scanner.

The scan deadlines are 30 minutes for smoke and 300 minutes for full, inside
45/330-minute job limits. SIGTERM gives the runner time to stop its process
groups, clean up its fixture, and retain an incomplete report. Hitting a
deadline fails the job. Scanner work stays outside the pre-commit checks.

Jobs use hosted runners, read-only repository permissions, no repository
secrets, and no persistent checkout credentials. Fork pull requests use the
same smoke job after any required repository approval. A pending approval,
disabled workflow, setup failure, skipped check or unavailable Docker is
**not a pass**. `ci-status.json` records `not-run` when setup never reaches
the runner. Integration requires a successful smoke check for the proposed
revision; if repository policy prevents a fork run, a maintainer must run
that revision on a trusted repository branch before integrating it. This
workflow does not change branch-protection settings.

Artifacts are uploaded even on failure and retained for 14 days. They include
the report, profile/manifest/pins, scanner logs and JSON results, request
evidence, database identity/settings, source and binary identities, cleanup
status, runtime information and the workspace dependency lock. Copy acceptance
evidence to durable storage before the CI retention window expires.

A pass covers only the named, pinned manifest/profile. All scheduled scans
must complete, all required vulnerable controls must be detected, every
protected case must pass, and cleanup must succeed. Scanner silence is not
proof that every possible ORM query is injection-free.
