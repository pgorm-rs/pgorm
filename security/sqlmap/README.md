# sqlmap verification

The Rust runner drives the upstream scanner against a loopback HTTP adapter
and a disposable PostgreSQL container. The case manifest names the actual
pgorm API exercised by each protected route and its vulnerable control.

## Run locally

Use Docker, Rust (CI uses 1.97.1), a C compiler/libclang, and the exact Python
version in `pins.json`. The runner verifies the upstream archive digest and
uses the pinned PostgreSQL image. It accepts no external scan target.

```sh
cargo test --locked --manifest-path security/sqlmap/adapter/Cargo.toml --tests
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
The runner also runs and records the direct regressions. Python is needed only
to execute the pinned scanner itself; the harness around it is entirely Rust.
`sqlmap-verdict <profile> <artifact directory>` turns a finished run's
`run/report.json` into the `ci-status.json` and `ci-summary.txt` CI publishes.

Every setting in `profiles.json` reaches the run. `dbms`, `level`, `risk`,
`concurrency`, `retries`, `http_timeout_seconds` and `time_sec` become scanner
options, `postgres_statement_timeout_seconds` is set on the disposable
server, and `case_timeout_seconds` bounds each scan. Each case's `field` in
`cases.json` is the parameter under test. The runner refuses a profile with
a missing or unrecognised field, or with timeouts that do not strictly
increase from the timed sleep to the scan deadline.

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
proof that every possible ORM query is injection-free, and for identifier
positions it is not evidence at all. See
[Identifier positions](#identifier-positions-the-render-oracle).

The [2026-09-09 acceptance attempt](acceptance/2026-09-09.md) completed all
420 scans and passed the direct regressions, but failed acceptance because
105 required controls were not detected. Its case matrix records the remaining
work; it is not a passing acceptance report.

## Declared inapplicability

[CONTEXTS.md](CONTEXTS.md) derives, from the pinned scanner's own payload and
boundary XML, which case/technique pairs no faithful control can ever reach.
Each case in `cases.json` carries an `inapplicable` map keyed by technique:

```json
"Q": {
  "reason": "<why no faithful control exists at this injection point>",
  "evidence": {
    "kind": "no-boundary",          /* or no-attachable-position */
    "payload": "inline_query.xml",  /* must define the exempted technique */
    "where": [3],                   /* qualifying <where> at this level/risk */
    "clause": [1, 2, 3, 8],         /* qualifying <clause> at this level/risk */
    "boundary": "<the boundary the context would need, and why it is absent>",
    "contexts": "4"                 /* the CONTEXTS.md section that derives it */
  }
}
```

`kind` names which argument the reviewer is checking. `no-boundary` means the
catalogue holds no boundary that can pair with those `where` values and still
escape this context — the inline-query case, where exactly one of the 53
boundaries serves `where=3` and it carries an empty prefix and suffix.
`no-attachable-position` means a boundary does pair, but the control's grammar
offers no slot the technique's vector can attach to.

Both runners refuse a declaration that omits a reason or any evidence field,
carries a surplus field, cites a payload file that does not define the
technique, or names a technique the case does not declare. A declared pair the
scanner nonetheless detects is recorded in `falsified_exemptions` and fails the
run: if it fires, it was never inapplicable. Exempted pairs leave the scheduled
work, appear under `inapplicable` in the report and CI summary with their
evidence, and are never counted as passes.

Of 210 pairs, 93 are declared inapplicable (Q 35, U 17, E 12, T 12, B 11, S 6),
leaving 117 scheduled. `schema`, `function`, `column`, `group`,
`pipeline-projection` and `stored-identifier` have no scheduled technique left
at all; the suite makes no detection claim about them, and their positions are
covered by the identifier render oracle described below. Only CONTEXTS.md
sections 7a and 7d are exempted. The section-7c control-shape cells are
reshaped to fire at level 3 — `insert` (INSERT … SELECT … WHERE),
`update-value` (value in the WHERE), `cast` (CAST target inside a WHERE) and
`enum` (an enum column filtered on the cast) now reach every scheduled
technique.

Double-quoted identifier contexts are where the exemptions concentrate, because
every boundary that closes a `"` below level 5 appends a comparison between two
invented double-quoted identifiers. A technique whose tests carry no `<comment>`
at level 3 — error-based and time-based — can only use that suffix, so it never
produces a statement PostgreSQL will resolve, whatever the surrounding SQL.

## Identifier positions: the render oracle

The identifier positions this suite cannot reach are judged by a separate,
structural instrument: the identifier render oracle
(`tests/identifier_oracle_tests.rs`, specified in
[`docs/spec/ident-oracle.md`](../../docs/spec/ident-oracle.md)). It covers:

- the six cases with no scheduled technique;
- the cases scheduled for only some techniques (`graph-alias`, `table`,
  `alias`, `order`, `enum-ddl`);
- every other public API that renders a caller-supplied name.

Scanner silence says nothing about any of these positions. The oracle does
not probe them with payloads. It renders each registered name position with a
hostile-name corpus, parses the statement with libpg_query, and requires the
name to come back as exactly the identifiers the position should produce,
with the rest of the parse tree unchanged. A live leg runs the nastiest names
against a real server.

That instrument, not this one, is where identifier-injection coverage is
claimed. It has found defects this suite could not reach:

- keyword names read as grammar at type and function positions;
- pipeline names beginning with `$`, which become placeholders or dollar
  quotes;
- a pipeline cast type written verbatim.

Each is filed as a plan node and pinned in the oracle until fixed.
