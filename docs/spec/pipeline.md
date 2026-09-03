# PRQL-shaped Pipeline API

`src/pipeline/` provides a typed, relation-to-relation query builder in the
shape of a [PRQL](https://prql-lang.org) pipeline, compiled to PostgreSQL SQL
through prqlc's intermediate representations. The whole module sits behind the
off-by-default `pipeline` cargo feature. Rules are grouped under
`[spec:pgorm:req:pipeline]`.

## Architecture

> [spec:pgorm:def:pipeline.adapter]
> The pipeline lowers typed Rust construction directly into prqlc's PL AST —
> no PRQL text round-trip — and compiles it with `pl_to_rq` followed by
> `rq_to_sql` targeting `Dialect::Postgres`, producing a single SQL string
> with `$N` placeholders. Clause placement is the compiler's: a filter after
> an aggregation becomes `HAVING`, a filter after a window wraps the pipeline
> so far in a CTE, and stages nest as subqueries exactly where the relational
> semantics require.
>
> The PL AST is not a stable API, so the boundary is confined: every `prqlc`
> import in the workspace lives in the private `src/pipeline/adapter.rs`, the
> dependency is pinned exact (`prqlc = "=0.13.14"`, `default-features =
> false`, `optional = true`), and no prqlc type appears in the public API. A
> compiler bump is absorbed by rewriting the adapter alone. The `pipeline`
> feature MUST stay out of the default set and prqlc MUST NOT appear in any
> other workspace crate's dependencies: only builds that opt in pay the
> compile.

## Surface

> [spec:pgorm:req:pipeline.surface]
> A `Pipeline` is constructed by `from(impl Iden)`, `from_schema(schema,
> table)` or `from_entity::<E>()` — the only entry points, so a sourceless
> pipeline is unrepresentable — and grown one whole transform at a time:
> `filter`, `derive`, `select`, `aggregate_by` (PRQL `group` + `aggregate`,
> taking `(keys, aggregates)`), `sort`, `take(i64)`,
> `take_range(RangeInclusive<i64>)` (1-based, inclusive), `join(JoinSide,
> table, condition)` with an explicit condition only, and `window(WindowDef)`
> with optional partition, ordering and an optional `Frame` (`rows` /
> `range`, integer bounds relative to the current row, `None` = unbounded).
>
> Expressions offer: comparisons (`eq`, `ne`, `gt`, `gte`, `lt`, `lte`),
> boolean `and` / `or` / `Not`, arithmetic through the std `ops` traits
> (`+ - * / %`, `Neg`), `coalesce`, `is_null` / `is_not_null` (rendered `IS
> [NOT] NULL`), `in_array` (rendered `IN (...)`, members may be bound
> placeholders), `cast` over the closed `CastType` enum (rendered
> `CAST(x AS t)`; closed because the type name reaches the SQL text
> verbatim), `case(arms, otherwise)` with a mandatory fallback, literals
> (`lit_int`, `lit_float`, `lit_str`, `lit_bool`, `null`), aggregates (`sum`
> — which PRQL wraps as `COALESCE(SUM(x), 0)` —, `min`, `max`, `average`,
> `stddev`, `count`, `count_rows`, `count_distinct`) and window functions
> (`row_number`, `rank` / `rank_dense` — which take the ranked column, per
> PRQL's own signatures —, `lag`, `lead`, `first`, `last`). `aliased(name)`
> names a projected expression; `out(name)` refers back to a
> pipeline-introduced name; `desc()` / `asc()` mark sort keys.
>
> Because every transform maps relation to relation, any `fn(Pipeline) ->
> Pipeline` is a composable query scope, and scopes may bind their own
> parameters.
>
> Deliberately outside v1 (cut vocabulary, not quality): the set operations
> (`append`, `intersect`, `remove`), `loop`, s-strings and f-strings (raw
> SQL interpolation holes), the `text` / `date` / `math` std modules, range
> membership (`between`), join table aliases (and with them self-joins), and
> `group`-scoped sorts other than through `window`.

## Parameters

> [spec:pgorm:req:pipeline.params]
> Runtime values enter through a `Binder` owned by the pipeline: every
> expression-taking transform passes one to its closure, and `bind(value)`
> pushes the `pgorm_query::Value` and mints its `$N` placeholder in a single
> step, numbering in bind order across the whole pipeline. prqlc carries
> `ExprKind::Param` through lowering untouched — including inside `HAVING`
> and aggregate arguments, with no renumbering — so position `N` in the
> emitted SQL is position `N` in the returned `Values`, and a placeholder
> without its value (or a value without its placeholder) cannot be
> constructed.
>
> The binder and the expressions it returns are branded with a
> higher-ranked, invariant closure lifetime; an expression containing a
> bound placeholder is thereby pinned to the pipeline that minted it, and
> carrying it into another pipeline is a compile error, not a runtime check.
> Param-free expressions (`col`, `out`, literals) are brand-polymorphic and
> reusable anywhere.
>
> `take` and `take_range` accept integers by value, not expressions: PRQL
> refuses a parameterized `LIMIT`, so the signature mirrors the one form
> that compiles.

## Failure model

> [spec:pgorm:req:pipeline.errors]
> Pipeline construction is infallible; `into_sql()` is the fallible
> boundary, returning `PipelineError` and never panicking, per
> `[dec:pgorm:no-panic]`. Two variants: `ReservedAlias(name)` — an alias
> from `aliased` collides with the closed reserved set (the top-level
> bindings of prqlc 0.13's `std` module, its submodule names `math` /
> `text` / `date`, and the PRQL keywords), screened before compilation so
> the collision is named instead of surfacing as an opaque resolution
> failure — and `Compile(diagnostics)`, carrying prqlc's own rendered
> diagnostics for everything its resolver rejects (a `std` name used as a
> value, ill-typed stages). `From<PipelineError> for Error` lifts into
> `Error::Query` so the terminals can fail through the ordinary channel.
>
> Compilation has no catalog: an `out(name)` reference no stage introduced
> resolves as a column of the source relation and compiles; whether the
> column exists is the server's question, answered at execution like any
> other raw SQL. Compile-time name checking covers only what prqlc can see —
> its own std namespace and pipeline-introduced names.

## Qualification

> [spec:pgorm:sem:pipeline.qualify]
> Column references are table-qualified by construction: `col(table,
> column)` takes an `Iden` pair (an entity and its column enum in the
> common case) and mints a two-part identifier, so the unqualified form —
> ambiguous the moment a join appears — is unrepresentable. Schema-qualified
> tables flow through `from_schema(schema, table)` and `from_entity::<E>()`
> (which honours `EntityName::schema_name`): the schema rides the identifier
> path through prqlc's `default_db` namespace and renders as
> `schema.table` — probed against the grammar oracle, with no known
> limitation. Identifier quoting is prqlc's: names needing quotes (spaces,
> case, reserved words such as a table named `order`) render double-quoted.
> `out(name)` is the one bare-identifier form, reserved for names the
> pipeline itself introduced.

## Termination

> [spec:pgorm:sem:pipeline.terminal]
> `into_sql()` yields `(String, Values)`, the raw-SQL currency of
> `exec.crud.selector-entry`: `into_model::<M>()` and `into_tuple::<T>()`
> stage exactly those entry points (`SelectorRaw::from_statement`,
> `SelectorRaw::into_tuple`), and the convenience terminals `all::<M>(db)`,
> `one::<M>(db)` and `one_opt::<M>(db)` go straight to `FromQueryResult`
> models on any `ConnectionTrait`. `one` / `one_opt` append a `take 1`
> stage before compiling — the pipeline is still a pipeline at that point,
> so the limit belongs to it, in contrast to `SelectorRaw::one`, which
> executes its text as written.
