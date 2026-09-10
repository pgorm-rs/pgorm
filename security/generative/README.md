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

Findings are retained under [findings](findings/). The distinct/append fix now
passes Python and standalone Rust reproduction plus independent live inspection
and eight-row fetch regressions. A chained set-operation program still loses its
expected grouping. New native count/window discrepancies are retained under
[window-semantics](findings/window-semantics/README.md): a nullable-column count
counts NULL rows, and first/last omit an explicitly authored row frame.
Full generation, complete oracle coverage, controls, shrinking,
general Rust emission, compile campaigns and acceptance remain separate WBS work.

## Independent checks in development

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_oracles.py
```

`Checker` runs the public native executor and a separate Psycopg 3.3.5 reference
against the paired fixture databases. The reference interprets the operation
graph using its own SQL algebra, quoting, bound values and binary result decoder.
It observes final rows and schema definitions on both sides through Psycopg.
The native build preparation installs the pinned oracle dependency separately;
dependency preparation does not invalidate the native compilation cache.

The current probes cover CRUD and result reuse, runtime and compiled models,
ActiveModel states/hooks, graph/source tuple shapes, cursors, pipeline stages,
windows and set operations, enum/table/index DDL, patterns, transactions and
streams. Typed scalar/array probes retain NULLs, enum identity, float bits,
decimal scale and temporal precision. Rows compare as multisets unless ordering
is explicitly asserted. An unordered partial stream must be a submultiset of
the reference rows with the required count; its closure/cancellation state has
an explicit invariant accounting for the cancellation race.

Literal reference values remain bound, with independent PostgreSQL type
inference for integers, finite floats, decimals, unknown strings/NULLs and
arrays. Literal temporal values follow the whole-second conversion explicitly
specified by `sql.render.value-literals+2`; bound temporal values retain their
fractional seconds. This conversion happens in the reference semantics, never
in result comparison. Ordinary and pipeline float literals have separate type
inference rules (for example, inline SQL `1` versus pipeline `1.0`).

Pipeline sort keys survive filtering, projection, take, joins and embedded
sources as private reference columns removed at the terminal. Group/distinct
reset output order. Ranked windows, NULL aggregates and empty frames have
dedicated live probes. A development probe wrongly assumed distinct retained
order; that reference error was corrected, and its original run was retained.

The template helper checks the existing `RawSQL` placeholder API. PostgreSQL
first infers parameter types from the original owned SQL; the independent
execution then binds values without borrowing the subject's substitution lexer.
Inspected SELECT/condition/expression SQL can be checked by a read-only probe
against independently computed results. Inspection amid writes or transactions
needs an oracle at the inspection point and currently reports incomplete.

Enum utility statements cannot bind labels directly. The reference creates a
uniquely named helper function in its own fixture schema, binds names/labels as
arguments, uses PostgreSQL's `format('%I', ...)` and `format('%L', ...)` inside
that function, and drops it before final state comparison. It does not require
temporary-table privileges or change the restricted execution role. Helper
cleanup failure makes the check incomplete.

Oracle development is still tracked as Doing. Non-finite inline floats, an
uninstalled vector type, some invalid conversion cases and native-parity
policies report incomplete. Ordered pipeline observations require a surviving
explicit sort. Unsupported semantics cannot establish a pass.
Further coverage and adversarial oracle validation remain necessary before
the full generated campaign can be accepted.

Each live check has a fresh directory in `target/generative-oracles/`, retaining
programs, native observations, reference results, state, comparisons and build
identity. The preserved chained-set and count/window findings produce **defect**
in this oracle self-check; detecting it verifies the checker and does not make
that program a passing campaign case. Two early probe failures were test setup
errors: PostgreSQL needs explicit parameter types for `$1 / $2`, and an i8
builder value uses a SMALLINT carrier, requiring an intermediate INTEGER cast
when deliberately decoding PostgreSQL's internal `"char"`. Their original run
artifacts remain in the earlier fresh directories.
