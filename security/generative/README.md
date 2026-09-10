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

## Native executor

Build the private application extension with the pinned Maturin interpreter,
then run probes in its installed environment:

```sh
PYTHONPATH=security/generative/src target/python-dev/bin/python -m pgorm_campaign.build
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_wire.py
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_executor.py
```

The first command records native source/lock hashes, revision, features, ABI,
toolchain, wheel/extension identity and build timing. Repeating it with the same
identity verifies and reuses the installed artifact. Native builds use a separate
Cargo target to avoid mixing application and standalone extension outputs.

The private Rust crate registers `campaign.Account`, `campaign.Note`, required,
optional and self-join graphs through seven sources, and pipeline `SourceList`
tuples through six. Its Rust modules remain usable by standalone replay without
Python registration code. The Account projection includes scalar, enum, JSON,
decimal, UUID and temporal fields. The nullable-element `tags` array is exercised
through public records/runtime descriptors; it is not a field of this compiled
entity shape. Account's native `before_save` hook appends `|hook` on insert and
increments a supplied rank on update; Note uses the default hook behavior.

`Executor` accepts a validated `Program` and an owned `Fixture`. It retains the
same asyncio loop and bounded pools between programs, resolves earlier results
at their consuming effects, and creates private binder expressions inside the
actual public callbacks. Runtime code is prohibited from launching subprocesses.
Trace records preserve selected native paths, native value snapshots, tagged
rows, optional tuple members, stream state, errors and affected-row counts.
`executed` means observations were collected; an independent oracle must still
decide correctness. It is not a passing campaign verdict.

The 41 handwritten live probes exercise all 89 declared instructions and eight
effects, plus timeout, cancellation, invalid-input, inactive-dispatch and build
prohibition checks. They verify 125 exact scalar/null/array value snapshots and
pool reuse separately. Each executor run has a fresh evidence directory under
`target/generative-executor/`. This is executor validation, not the generated
full campaign or million-program stress proof.

Open findings are retained under [findings](findings/): the distinct/append
composition fails in Python and standalone Rust, and a chained set-operation
program loses its expected grouping. Neither is resolved by successful individual
instruction probes. Full generation, independent oracles, controls, shrinking,
general Rust emission, compile campaigns and acceptance remain separate WBS work.
