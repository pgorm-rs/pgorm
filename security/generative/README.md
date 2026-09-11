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
Full generation, complete oracle coverage, shrinking,
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

The independent oracle kernel is implemented. Non-finite inline floats, an
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

## Sensitivity controls and attribution

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_controls.py
```

The versioned catalog contains 22 controls across 21 mandatory comparison
categories. Each starts with a passing public pgorm execution and independent
reference. Deliberately incorrect SQL runs through a separate driver in the
owned subject database, after both databases reset to the program's fixture.
Other controls corrupt that fresh native run's observations. Control observations
carry their own dispatch evidence and cannot claim native execution.

Controls cover missing value/name escaping, broadened predicates, swapped binds,
wrong bound/decoded types, missing/duplicate/reordered rows, optional joins,
SQL/JSON NULL distinctions, arrays, enum identity, float bits, decimal scale,
temporal precision, affected counts, omitted writes/schema, committed rollbacks,
stream closure and exact error causes. A valid control requires exactly its
declared difference; arbitrary failures, inactive or unchanged mutations, missing
work and a failed native baseline fail the command. Aggregate validation
recomputes comparisons from retained evidence instead of trusting status labels.
The live check also verifies that the public wheel excludes these test modules.

Finding retention writes original programs, fixtures, observations, identities,
replay arguments and content hashes into a fresh directory. Attribution requires
completed standalone Rust evidence from the same native source identity and
rechecks both independent verdicts before comparing observed behavior. Native
reproduction locates a discrepancy below Python; reference semantics still need
review before blaming the library. Missing/inconsistent native evidence remains
unattributed and never removes the original Python failure. This machinery does
not patch production code or close findings. General Rust emission and full
Python/Rust execution parity remain the separate replay node's work.

## Input corpus

`corpus_builtin.builtin()` provides 308 repository-owned inputs: hostile text,
identifier lengths around PostgreSQL's 63-byte boundary, exact numeric limits,
IEEE float bits, typed NULLs, arrays (including empty/NULL/NULL elements), JSON,
Decimal, UUID, enum, temporal, network, bytes and vector values.
`corpus_random.sample(seed, index)` produces deterministic typed values using a
versioned SHA-256 counter, independent of iteration order or worker partitioning.
Neither function imports pgorm, starts PostgreSQL, accesses the network or needs
sqlmap. A valid portable value does not imply support in every PostgreSQL context;
the grammar must choose compatible contexts or declare a rejection check.

`Input.node(author, role="value")` inserts a value node. The `identifier` role
inserts that text through a name node. No role can turn corpus text into a raw
query or an intentional expression. Empty names, NULs and byte lengths are
reported as traits, without assuming an API-specific acceptance/rejection rule.

The optional sqlmap importer reads a local archive matching the immutable revision
and SHA-256 in `sqlmap_archive.py`. It does not run the HTTP scanner:

```sh
PYTHONPATH=security/generative/src python3 -m pgorm_campaign.sqlmap_import \
  --archive target/sqlmap-cache/sqlmap.tar.gz \
  --output target/generative-corpus/my-import --seed 20260911 --variants 2
```

The output must be a new directory. It retains the upstream LICENSE,
THIRD-PARTY notices, original XML including notices, importer source, content
hashes, exact substitution contexts, transformation rules and a complete
template/boundary inventory. Imported upstream data and its license notices stay
together in this optional artifact; the public pgorm wheel excludes the campaign.
The importer source and data hashes make the recorded transformations reviewable.
`sqlmap_import.load(directory)` validates artifact hashes, input types, identities
and counts before returning data for generation.

At the pinned revision there are 365 request templates and 53 boundaries.
329 requests are supported; 36 dynamic UNION requests need scanner-selected
projection topology and are explicitly inventoried as unsupported. Scanner
vectors and response oracles are also inventoried as omitted. With the seed and
two variants above, import produces 21,070 contextual inputs containing 18,655
distinct values. **These are input counts, with zero executed programs.**
Clause/placement compatibility and boundary metadata are retained; all DBMS
families supply hostile strings, without claiming valid PostgreSQL attack syntax.
No URL decoding, tamper processing, inference or scanner request emulation occurs.

Verify the real pinned archive, deterministic re-import, per-input reconstruction,
complete unsupported inventory, exact notice retention and corruption rejection:

```sh
PYTHONPATH=security/generative/src python3 security/generative/tests/live_corpus.py \
  --archive target/sqlmap-cache/sqlmap.tar.gz \
  --output target/generative-corpus/verified-import
```

## Generated programs

`grammar.generate(seed, index, family=..., mode=...)` composes 15 families:
SELECT, CRUD/result reuse, pipelines, registered entities, ActiveModel writes,
graphs, cursors, runtime models, typed values, schema changes, SQL grouping,
pipeline grouping/windows, set operations, compiled source tuples and owned SQL
templates. All calls go through the public Python module and its native builders.
There is no HTTP transport or scanner in this path.

Recursive expressions carry schema types and nullability. Pipeline transitions
track available projections, source aliases and binder ownership. PRQL grouping
and partitioned windows remove their keys from the inner expression scope.
Runtime-model inserts use unaliased tables, as required by that public API.
Sequences reuse observed results, actually reach both conflict actions and close
nested commit/rollback scopes. Read-only transactions and ordered stream prefixes
are generated separately. Recipes label deliberate database rejections with
exact expected SQLSTATEs; unexpected valid-program failures remain failures.

Recursive text uses NUL-free corpus entries up to 4 KiB and identifiers have a
smaller byte budget. The type family separately draws exact built-in and random
numeric, float, Decimal, JSON, array, enum and temporal boundaries, through bound
and literal paths with explicit PostgreSQL types. It pins the local-time worker
to UTC. Construction/rejection coverage for unsigned64 and uninstalled pgvector,
and complete observation-based coverage of every matrix context, remain full
profile obligations; these samples do not establish those obligations.

Fast tests check structural diversity after removing literal payloads, stable
generation, all catalog operations/effects across 2,250 generated programs under
128-node budgets, result dependencies and binder/source ownership. Generation
counts establish reachable productions, not executed database coverage. The live
command retains programs and recipes before execution, then native/reference
results, fixture evidence and the unchanged extension identity:

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_grammar.py --count 512 --seed 20260911
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_grammar.py --count 128 \
  --corpus target/generative-corpus/verified-20260911
```

`--corpus` verifies an existing offline import and records its manifest hash;
it neither downloads inputs nor runs sqlmap. `--family` selects a family for
focused verification, and `--program path/program.json` replays exact retained
instructions independently of later generator changes.

The seed `20260911` 512-program run `run-39373a66d2c1` completed with 476 passes,
32 exact expected rejections and four retained discrepancies, with no incomplete
cases. The 128-program imported-corpus run `run-37eb6fcba3f0` completed with 119
passes, eight expected rejections and one hidden-order discrepancy. Both use
the same installed extension and invoke zero builds. They correctly exit 1;
neither run is full matrix or million-program acceptance evidence.

Generated findings retain the original program, independent observations,
fixture, native identity and replay arguments with hashes. Open discrepancies
include [hidden sort columns](findings/pipeline-hidden-order/README.md),
[quoted output names](findings/pipeline-quoted-output/README.md),
[set composition](findings/set-precedence/generated-chain/README.md) and
[window frames](findings/window-semantics/generated-frame/README.md). A retained
[native panic](findings/pipeline-native-panic/README.md) is accounted as incomplete
execution while preserving cleanup and cancellation behavior. These findings
are separate from the fixed DISTINCT regression. The verifier exits unsuccessfully
for disagreements and incomplete cases; there is no passing allowlist.

## Shrinking

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_shrink.py \
  --program security/generative/findings/pipeline-hidden-order/program.json
```

`shrink.reduce(checker, program)` searches for a smaller program that still
fails the same way. It reduces effect sequences, transaction nesting, expression
and pipeline structure, bound values and the declared fixture, then re-validates
every candidate against the portable format before running it. Each candidate
goes through the same `Checker`, which restores its own declared baseline on
both databases first, so no attempt inherits the previous one's state.

The predicate is frozen from a completed independent baseline run and is more
than the comparison verdict: it records the subject's own account of the failing
step — observation category, error class and SQLSTATE. A candidate that stops
failing, fails somewhere else, or reports incomplete for a malformed,
unsupported or fixture reason is recorded and discarded, never adopted. On the
retained hidden-order finding this matters concretely: a coarser predicate
accepted a reduction whose subject failed in PRQL compilation rather than with
PostgreSQL's `42703`, which is a different defect.

Budgets bound candidates, offered rewrites, passes, wall time and per-candidate
timeout, and all of them are recorded with the result. Interruption keeps the
original program and the best reproducer found so far. No result claims global
minimality, and the reports say so in as many words.

General Rust replay, compile profiles, the full matrix runner/CI and the
million-program acceptance campaign remain separate unfinished WBS work.
