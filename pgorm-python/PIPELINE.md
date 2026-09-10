# Python pipelines

`pgorm.pipeline` composes the real Rust `pgorm::pipeline::Pipeline`. The native
module owns each pipeline and compiles it through pgorm's PRQL compiler. Queries
execute directly against PostgreSQL; no HTTP server is involved.

```python
import pgorm as p
from pgorm import pipeline as pl

items = p.Table("items", schema="app")
amount = pl.col("items", "amount")
category = pl.col("items", "category")
total = pl.alias("total")

query = (
    pl.from_(items)
    .filter_with(lambda binder: amount > binder.bind(10))
    .group(category)
    .aggregate(pl.sum(amount).as_(total))
    .sort(total.desc())
    .take(5)
)

async with p.Pool(dsn) as pool:
    async with pool.connection() as connection:
        records = await query.all(connection)
```

`from_` and `source` accept a native `Table`, registered `Entity`, another
`Pipeline`, named `Source`, or runtime `Model` descriptor. Models contribute their
table metadata. `pl.col(table_alias, physical_column_name)` creates a qualified
reference; it does not translate a runtime model's logical field names. Use
`pl.source(model)` when passing a model to `join`, `append`, `intersect` or `remove`.

Every stage returns a new pipeline. `filter`, `derive`, `select`, `group`,
`aggregate`, `window`, `sort` and `join` have `*_with` counterparts. Projection,
grouping and ordering methods accept positional expressions; their callbacks
return an expression, list or tuple. A bound list supports zero through 32
expressions. Plain lists have no binding-specific limit. `group` requires a
following `aggregate` before further pipeline stages.

## Literals and parameters

Plain `None`, booleans, integers, finite floats and strings become native SQL
literals. `pl.literal` makes that choice explicit. Use `binder.bind(value)` inside
a synchronous `*_with` callback for parameters. An explicit `p.Value(value, kind)`
preserves its Rust value variant; plain Python integers infer `i64`.

```python
query = pl.from_(items).filter_with(
    lambda binder: amount > binder.bind(p.Value(10, "i32"))
)
compiled = query.inspect()
assert compiled.params[0].kind == "i32"
```

A callback runs once during construction. Each binding is minted once inside the
corresponding Rust binder scope. Reusing the expression reuses its placeholder;
the Rust compiler prunes unused bindings and rebases composed pipelines' values.
Saving a binder or bound expression does not extend its lifetime: operations on
it after the callback raise `LifecycleError`. Expressions from different binder
callbacks cannot be combined, and a bound expression cannot enter an `Over` or a
plain stage.

Qualified enum and enum-array `Value` tags are rejected by the pipeline binder:
Rust's pipeline has no qualified enum cast API. A plain string can use PostgreSQL
type inference when that is intended. Other supported `Value` kinds, including
typed nulls, are available through the binder; literal conversion is limited to
the native pipeline's scalar literal vocabulary. The SQL builder expression API
(`p.Expr`) and pipeline expression API (`pl.Expr`) are separate native types.

## Joins, windows and sets

Name a relation with `pl.source(relation).named(alias)`. The alias becomes its
column qualifier. `join(source, predicate, kind=p.Join.Left)` supports inner,
left, right and full joins. `pl.this(column)` and `pl.that(column)` identify sides
inside join predicates when an embedded pipeline has no explicit name.

```python
rank = pl.alias("row_rank")
top_two = (
    pl.from_(items)
    .window(
        pl.row_number().as_(rank),
        over=pl.over().by(category).sort_by(amount.desc()).rows(None, 0),
    )
    .filter(rank <= 2)
)
```

Windows support `by`, `sort_by`, `rows(start, end)` and `range(start, end)`; `None`
means unbounded. Aggregate functions include `sum`, `min`, `max`, `average`,
`stddev`, `count`, `count_distinct` and `count_rows`. Window functions include
`row_number`, `rank`, `rank_dense`, `first`, `last`, `lag(offset, value)` and
`lead(offset, value)`.

`append`, `intersect` and `remove` consume another source; use matching explicit
projections on both sides. Rust's compiler rejects unsupported wildcard shapes.
`distinct`, `take(count)` and inclusive `take_range(start, end)` use native stage
semantics. Compiler errors, including reserved alias names, surface as
`ConstructionError`.

## Results and registered models

`all(connection)` returns dynamic `Record` objects. `one(connection)` and
`one_opt(connection)` append native `take(1)`; the former errors when no row exists,
and the latter returns `None`. `inspect(terminal="one")` shows that exact SQL.
Pool and connection `fetch_*` methods also accept pipelines and keep their strict
dynamic-record cardinality rules. Use `pool.stream(query)` or
`connection.stream(query)` for bounded asynchronous record streaming.

An application native module can register a concrete Rust entity tuple:

```rust
registry.entity::<account::Entity>("app.Account")?;
registry.entity::<note::Entity>("app.Note")?;
registry.sources::<(account::Entity, note::Entity)>("app.AccountNotes")?;
```

Python then selects actual Rust models through `SelectedSources`:

```python
accounts = p.entity("app.Account")
notes = p.entity("app.Note")
query = (
    pl.from_(accounts)
    .join(
        pl.source(notes).named("n"),
        pl.col("accounts", "id") == pl.col("n", "account_id"),
        kind=p.Join.Left,
    )
    .select_sources(
        pl.sources("app.AccountNotes"), qualifiers=["accounts", "n"]
    )
)
rows = await query.all(connection)
```

One to six source types are supported. Every returned row is a tuple, and every
position is optional, including the first under right/full joins. For a single
source, `(None,)` means a result row whose source was absent; `None` from
`one_opt` means no result row. Present rows use the real registered Rust model
decoder, so malformed present data raises `DecodeError`. The Rust terminal
rejects prior stages that remove source namespaces, such as explicit projection
or aggregation. Registered source selections expose `all`, `one`, `one_opt` and
`inspect`; model streaming is not exposed.

`p.capabilities()` records the exact pipeline vocabulary, limits and compiled
source registrations. A stock wheel contains no application entity registrations.

## Verification

`tests/test_pipeline.py` executes the installed public API against PostgreSQL.
`cargo test --manifest-path pgorm-python/Cargo.toml --test pipeline` compares 21
Python programs with independent Rust constructions, including exact SQL and
`Values`. `checks/entities.py` builds and installs an independent application wheel
and verifies all six source tuple arities, optional model decoding, aliases,
right/full joins, errors and cancellation, alongside entity and graph regressions.
