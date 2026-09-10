# Generated pgorm campaigns

This test package implements the `generative` WBS and
[generated-program specification](../../docs/spec/generative.md). Its target is
generated public Python builder compositions, independent PostgreSQL checks,
reproducible Python/Rust failures and a separate bounded Rust compile suite.
The Python module is supplied by `pgorm-python`; this package is test
infrastructure and is excluded from its distributions.

The [portable program format](PROGRAMS.md) defines typed operation references,
exact value tags, binder/transaction scopes and the versioned coverage matrix.

## Fixture checks

From the repository root:

```sh
PYTHONPATH=security/generative/src python3 -m unittest discover \
  -s security/generative/tests -p 'test_*.py' -v
PYTHONPATH=security/generative/src target/python-check/bin/python \
  security/generative/tests/live_fixtures.py
```

The live command requires Docker and an installed native `pgorm` package in the
selected interpreter. It creates its own pinned PostgreSQL container; it has
no external database URL option. Two subject/reference database pairs verify
baseline equivalence, independent state, reset, restricted privileges and
statement deadlines. Its evidence is in `target/generative-fixtures/`.

`Fixture` owns the container and anonymous data volume, with memory, CPU,
process and command limits. It publishes PostgreSQL only on loopback. Programs
receive connections for the restricted `campaign` role. Administrative access
stays in the fixture controller; credentials are excluded from its report and
the portable baseline definition. Cleanup verifies the ownership label and
removes the container and volume even after startup failure or cancellation.
Failure to establish cleanup fails the command.

`baseline.default()` supplies multiple tenants, NULLs, duplicate sort keys,
matched and unmatched joins, protected sentinels, schema collisions, quoted
identifiers and qualified enums, plus exact numeric, JSON, array and temporal
data. Subject and reference databases receive the same portable definition
through independent setup SQL. `Fixture.reset()` restores rows on both sides
before a new attempt, retaining enum identities for reusable connections.
Schema-changing programs require `rebuild=True`; close both pools before that
rebuild so PostgreSQL type metadata cannot outlive the replaced enum types.
State persists between operations within a program.

The live report records the pinned image and actual database image/version,
encoding, collation, timezone, search path, string mode, deadlines, restricted
role flags and baseline hash. Fixture success alone does not establish
generated-program coverage or the complete campaign's acceptance.
