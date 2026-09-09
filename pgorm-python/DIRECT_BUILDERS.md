# Direct Python builder integration

This suite verifies the original integration objective: install `pgorm`, compose
its real Rust builders in Python, and execute them against PostgreSQL in the
same process. The public API and database queries do not need HTTP, a query
dispatcher or compilation per query.

From the repository root, after installing the pinned build environment from
the package README:

```sh
target/python-dev/bin/python pgorm-python/tests/with_postgres.py \
  target/python-dev/bin/python pgorm-python/checks/direct_builders.py
```

The wrapper provisions and removes its own PostgreSQL fixture. An existing
disposable database can instead supply `PGORM_TEST_DSN` directly to
`checks/direct_builders.py`. Optional `PGORM_DIRECT_SCHEMA`,
`PGORM_DIRECT_ACCOUNTS` and `PGORM_DIRECT_EVENTS` choose the application names;
the schema must not already exist. Defaults include fresh names with quotes,
Unicode, percent signs and backslashes. The suite drops only the schema it
successfully created.

The runner first compiles the independent Rust parity executable and builds
one wheel. It installs that wheel into a new temporary Python environment,
with user site packages and `PYTHONPATH` disabled. Query execution runs with
only that environment's executable directory on `PATH`; a Python audit hook
rejects process launches during the query phase. Native builders delegate to
the existing Rust APIs and use the installed extension throughout.

The four programs combine two structural variants with bound and literal
value paths. Each program tests seven query shapes, giving 28 exact SQL and
parameter comparisons:

| Query shape | Database checks |
| --- | --- |
| INSERT application accounts | Three affected rows; runtime qualified table |
| INSERT application events | Three affected rows; changing quoted/Unicode values |
| SELECT with a left join and nested predicates | Expected records; absent joined fields retain typed NULLs |
| UPDATE with RETURNING | Exact updated record and visits count |
| DELETE | One affected row, then zero on repeating the same builder |
| SELECT with no matching row | `fetch_optional` returns None |
| SELECT final state | Expected surviving accounts and updated values |

Structural variants change predicates and a limit inside the same loaded
extension. Values include quote/semicolon/comment-like text; every query under
test uses public builders. Explicit RawSQL is confined to fixture DDL and
resetting test state.

Literal expression values remain SQL literals. The typed `limit(2)` still
contributes the bound u64 parameter used by Rust's limit API; parity checks
retain that distinction rather than rewriting the Rust builder's behavior.

`tests/direct_builders.rs` independently constructs equivalent `pgorm_query`
statements using the run's runtime inputs. It compares their SQL and tagged
Rust parameters against the Python report. Only the stable value snapshot
encoder is shared; query construction is independent. The runner then changes
one SQL string and one parameter list in temporary copies of the report and
requires the precompiled verifier to fail for both mutations.

Successful runs leave these reviewable artifacts in
`target/python-direct-builders` (override with `--output`):

- The installed wheel.
- `queries.json`: all runtime inputs, SQL, tagged parameters and verified
  database outcomes, without connection credentials.
- `summary.json`: pass status, 28 program comparisons, negative-check results
  and SHA-256 hashes of the wheel and query report.

The temporary Python environment is removed afterward. Construction, database,
result, parity or negative-control failures exit nonzero; the summary is marked
unfinished at the start, so a failed rerun does not leave an old passing status.
The ordinary installed-wheel unittest discovery also includes the public
database test. Full package acceptance must additionally run this clean-install
runner so the independent Rust parity and mutation checks remain required.

This milestone proves dynamic statement construction and execution. It does
not claim completion of the separate transaction, dynamic model, compiled
entity/graph, pipeline, schema, codegen or distribution WBS nodes.
