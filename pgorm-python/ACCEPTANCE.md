# Native Python API acceptance

Python applications import `pgorm`, compose native Rust builders and execute
them against PostgreSQL in the same process. No HTTP server, adapter or query
dispatcher is required. The [ordinary application](examples/application.py)
creates its own schema, performs CRUD and a join, uses transactions and a
savepoint, executes a pipeline and streams records using the public package.

The `python` WBS subtree implements the [Python specification](../docs/spec/python.md).
Its acceptance checker verifies the installed standalone package and application
extensions, with independent Rust comparisons for builder and execution results.
Runtime model declarations work entirely in Python. Rust `EntityTrait` models,
typed graphs and source tuples require the explicit compiled registration
workflow described in [ENTITIES.md](ENTITIES.md) and [CODEGEN.md](CODEGEN.md).

## Run the complete check

From the repository root, using CPython 3.14, Rust, a C compiler, libclang,
OpenSSL, `uv` and the repository's `nplan` installation:

```sh
uv venv target/python-dev
uv pip install --python target/python-dev/bin/python 'maturin==1.15.0'
PGORM_TEST_PG_BIN=/path/to/postgresql/bin \
  target/python-dev/bin/python pgorm-python/tests/with_local_postgres.py \
  target/python-dev/bin/python pgorm-python/checks/acceptance.py
```

The PostgreSQL 16 wrapper creates a disposable cluster and CA, supplies
`PGORM_TEST_DSN` and `PGORM_TEST_CA`, and removes its cluster afterward. The
Docker-based `tests/with_postgres.py` wrapper accepts the same command. An
existing test database can supply those variables directly; it must allow
creation and deletion of the test schemas and tables.

`target/python-acceptance/summary.json` reports success only after every phase
passes. It records hashes of the phase reports; the directory retains package
artifacts, recorded query results and individual logs. `--output PATH` selects
another evidence directory. Standalone extension builds use their own Cargo
target directory because application extensions have a different module entry
point. Package installation occurs in fresh Python environments.

## Coverage and independent comparisons

| API or contract | Evidence run by acceptance |
| --- | --- |
| No HTTP; direct SELECT/CRUD | `checks/direct_builders.py` installs a wheel and executes 28 query programs against PostgreSQL. `tests/direct_builders.rs` independently constructs the Rust SQL and tagged parameters. Python's query phase rejects process launches; deliberately altered SQL and parameters must fail comparison. |
| Values and expressions | Rust tests compare the Python wrappers with Rust value variants and expression constructors. Installed `test_values.py` and `test_expressions.py` cover exact conversion, typed NULL, enum identity, precision rejection and owned builder state. |
| Statements and raw SQL | Native statement tests compare SQL and parameter order for SELECT, INSERT, UPDATE, DELETE, conflicts and raw templates. Installed statement and direct-builder tests exercise their public Python construction and execution. |
| Runtime models and schema | Installed model tests cover mapped fields, defaults, CRUD, composite keys and decoding constraints. Rust `tests/schema.rs` compares explicit DDL; application `schema_parity.rs` compares generated statements with real entity traits. Both schema paths execute against PostgreSQL. |
| Registered entities, graphs and cursors | The application wheel runs real entity queries, ActiveModel writes and hooks. Native parity tests compare typed query/graph SQL and ActiveValue states. Registered graph tests cover source arities 1–7; `graph_oracle` compares eight cursor programs with independent Rust builders. |
| Pipelines and registered sources | `tests/pipeline.rs` compares Python pipeline SQL and parameters with independent Rust pipelines. Installed tests exercise stages, bound callbacks, scope rejection and source tuples of arity 1–6, including absent joined models. |
| Connection and transaction execution | `registered_execution.py` records runtime builders, compiled queries, bound raw SQL, entity reads/writes, graph reads/cursors, plain pipelines and typed source terminals through both connection and transaction objects. `execution_oracle` independently repeats the operations in Rust and compares outcomes, including hooks, DDL, commit, rollback, a nested savepoint, read-only mode and connection streaming. A deliberately changed outcome must be rejected. |
| Results, runtime and lifecycle | Installed tests exercise pool and connection execution, result cardinality, type decoding, TLS verification and rejection, errors, checkout timeout, cancellation, event-loop ownership, transaction reservation and bounded streaming cleanup. Native lifecycle tests verify Rust pool close/discard behavior underlying the Python lifecycle policy. |
| Code generation and typing | `checks/codegen.py` builds and installs generated application bindings, checks deterministic output, executes registered entities/graphs and checks their concrete stubs. Distribution checks compare installed runtime signatures with stubs, type-check the package and runnable example, and require the expected invalid programs to fail. |
| Distribution and optional Rust integration | `checks/distribution.py` builds an isolated wheel and source archive, builds another wheel directly from that archive, and tests both fresh installations including TLS and typing. It verifies package payloads, native links and notices. The acceptance runner checks that the default Rust dependency graph excludes PyO3, then builds the workspace with Python tooling replaced by failing probes. |
| Specification tracking | `nplan spec validate` checks references under Rust binding code, Python sources and tests. The acceptance rule has implementation and verification annotations in executable code. |

The execution comparison uses PostgreSQL's actual results, separately produced
by Rust and Python. SQL/parameter comparisons cover construction choices that
could produce the same rows. Negative comparisons ensure an oracle cannot
silently accept a mismatched report.

## Recorded result and limits

The complete checker passed locally on 2026-09-10 with CPython 3.14.4 and the GIL,
macOS 26.5.1 arm64, and PostgreSQL 16. The run included 23 native binding tests,
eight application Rust tests, 28 direct-builder query programs, registered
application checks and 110 standalone Python tests on each of two installations
(direct wheel and source-derived wheel). Both installations passed strict
package/example typing, installed signature checks and 13 expected invalid
typing cases. The generated application also passed its package typing and
four expected invalid cases. The default Rust workspace build and nspec
validation passed.

These results establish the tested combination recorded in
[support.json](support.json). The configured macOS 15 and Linux CI jobs still
need their own passing runs; registry publication is separate. See
[DISTRIBUTION.md](DISTRIBUTION.md) for those candidate platforms and artifact
requirements.

The public capability manifest and specification define the supported surface.
Conversions that cannot preserve a value fail explicitly; see
[VALUES.md](VALUES.md) and [RESULTS.md](RESULTS.md). Streams belong to a pool or
connection; transactions expose the documented query terminals in
[TRANSACTIONS.md](TRANSACTIONS.md). Free-threaded interpreters and subinterpreters
remain outside the support matrix.
