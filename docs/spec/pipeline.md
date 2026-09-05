# PRQL-shaped Pipeline API

`src/pipeline/` provides a typed, relation-to-relation query builder in the
shape of a [PRQL](https://prql-lang.org) pipeline, compiled to PostgreSQL SQL
through prqlc's intermediate representations. The pipeline is a permanent part
of the crate, compiled in every build. Rules are grouped under
`[spec:pgorm:req:pipeline]`.

## Architecture

> [spec:pgorm:def:pipeline.adapter+2]
> The pipeline lowers typed Rust construction directly into prqlc's PL AST —
> no PRQL text round-trip — and compiles it with `pl_to_rq` followed by
> `rq_to_sql` targeting `Dialect::Postgres`, producing a single SQL string
> with `$N` placeholders. Clause placement is the compiler's: a filter after
> an aggregation becomes `HAVING`, a filter after a window wraps the pipeline
> so far in a CTE, and stages nest as subqueries exactly where the relational
> semantics require.
>
> The PL AST is not a stable API, so the boundary is confined: every `prqlc`
> import in the root crate lives in the private `src/pipeline/adapter.rs`,
> the dependency is pinned exact (`prqlc = "=0.13.14"`, `default-features =
> false`), and no prqlc type appears in the public API. A compiler bump is
> absorbed by rewriting the adapter alone. prqlc is a plain dependency of the
> root crate by operator decision — the pipeline is part of the permanent
> story, and every build pays its compile, the same posture as `pg_query` —
> and it MUST NOT appear in any other workspace crate's dependencies, with
> exactly one exception: `pgorm-sql-macro` carries the same exact pin for the
> `prql!` macro (`[spec:pgorm:def:macros.prql]`), which runs the compiler at
> build time over PRQL text. The two pins MUST stay identical, so the typed
> pipeline and the text macro emit through one compiler version and a bump
> rewrites both consumers in one motion.

## Surface

> [spec:pgorm:req:pipeline.surface+1]
> A `Pipeline` is constructed by `from(impl IntoSource)` or
> `from_schema(schema, table)` — the only entry points, so a sourceless
> pipeline is unrepresentable — and grown one whole transform at a time:
> `filter`, `derive`, `select`, `group(keys)` followed by `aggregate(aggs)`,
> `window(columns, over)`, `sort`, `take(i64)`,
> `take_range(RangeInclusive<i64>)` (1-based, inclusive), and
> `join(JoinSide, table, condition)` with an explicit condition only.
>
> Each transform has two forms. The plain one takes its expressions by value
> — the whole query reads as one chained expression, with constants written
> as Rust literals. The `_with` one takes a closure and hands it the `Binder`,
> and exists only where a runtime value has to enter
> (`[spec:pgorm:req:pipeline.params+1]`). The `_with` closures of the
> list-taking transforms return a fixed-size `[Expr; N]`: an array is the one
> list shape whose element type can be written under the higher-ranked brand.
>
> Grouping is two steps that read as PRQL's do: `group(keys)` yields a
> `Grouped`, whose only method back to a `Pipeline` is `aggregate(aggs)` (or
> `aggregate_with`), so a grouping with nothing aggregated is unrepresentable
> rather than merely wrong. The pair lowers to the single PRQL
> `group {keys} (aggregate {aggs})` transform, so it stays one stage and a
> filter that follows still lands in `HAVING`.
>
> `window(columns, over)` names what to compute and what to compute it over.
> The `Over` is built by `by(keys)` (partition), `sort_by(keys)` (ordering)
> or `over()` (neither), chained (`by(..).sort_by(..)`) and narrowed by
> `rows(start, end)` or `range(start, end)` — integer bounds relative to the
> current row, `None` meaning unbounded on that side. With a partition the
> stage compiles to `PARTITION BY` under a `group`; without one the window
> spans the relation.
>
> Every scalar position takes `impl Into<Expr>` and every list position takes
> `impl ExprList`. `Into<Expr>` is implemented for `Expr` itself, for any
> `ColumnTrait` column (qualified by its own entity,
> `[spec:pgorm:sem:pipeline.qualify+1]`), for an `AliasName` token, and for
> the Rust literals `i32`, `i64`, `f64`, `bool` and `&str`. `ExprList` is
> implemented for a single expression, for `[T; N]` and `Vec<T>` of one
> convertible type, and for tuples of up to twelve mixed ones — the mixed
> case being the ordinary one for a projection, and a tuple being how PRQL
> spells a projection too.
>
> The operators live on the `ExprOps` trait, blanket-implemented for `Expr`,
> for `AliasName` and for every `ColumnTrait` column, so a column or a token
> is as much an expression as an `Expr` is: comparisons (`eq`, `ne`, `gt`,
> `gte`, `lt`, `lte`), `and` / `or` (and `Not` on `Expr`), arithmetic as
> methods (`add`, `sub`, `mul`, `div`, `rem`) and as the std `ops` traits on
> `Expr` (`+ - * / %`, `Neg`), `coalesce`, `is_null` / `is_not_null`
> (rendered `IS [NOT] NULL`), `in_array` over any `IntoIterator` (rendered
> `IN (...)`, members may be literals or bound placeholders), `cast` over the
> closed `CastType` enum (rendered `CAST(x AS t)`; closed because the type
> name reaches the SQL text verbatim), `as_(name)`, and `desc()` / `asc()`
> for sort keys. Free functions supply `case(arms, otherwise)` with a
> mandatory fallback, `null()`, the aggregates (`sum` — which PRQL wraps as
> `COALESCE(SUM(x), 0)` —, `min`, `max`, `average`, `stddev`, `count`,
> `count_rows`, `count_distinct`) and the window functions (`row_number`,
> `rank` / `rank_dense` — which take the ranked column, per PRQL's own
> signatures —, `lag`, `lead`, `first`, `last`).
>
> `as_(name)` names a projected expression. The name is an `AliasName` token
> (`[spec:pgorm:def:sql.types+3]`) when anything refers back to it, and a
> bare `&'static str` when nothing does — `as_` takes `impl Into<AliasName>`
> and both spell the same thing. A token declared once by `let rn =
> alias("rn")` is also its own reference: it converts to an unqualified
> expression, so `filter(rn.lte(2))` refers to what `row_number().as_(rn)`
> introduced, and the name exists in the program exactly once. The token
> carries no evidence that it was ever attached: a reference to a name no
> stage introduced compiles, and the server answers for it
> (`[spec:pgorm:req:pipeline.errors+1]`).
>
> `ExprOps`'s comparison names are also `ColumnTrait`'s. Both traits can be
> in scope: the pipeline's methods take `self` by value and `ColumnTrait`'s
> take `&self`, so a column resolves to the pipeline's method where both are
> imported, and the ORM spelling is then `ColumnTrait::gt(&col, v)`. The
> pipeline is in no prelude, so this is per-module and opt-in, and the
> failure mode of getting it wrong is a type error at the ORM call site.
>
> Because every transform maps relation to relation, any `fn(Pipeline) ->
> Pipeline` is a composable query scope, and scopes may bind their own
> parameters.
>
> Deliberately outside v1 (cut vocabulary, not quality): the set operations
> (`append`, `intersect`, `remove`), `loop`, s-strings and f-strings (raw
> SQL interpolation holes), the `text` / `date` / `math` std modules, range
> membership (`between`), join table aliases (and with them self-joins),
> `group`-scoped bodies other than `aggregate` and `window`, and iterator
> adapters as list arguments (an `ExprList` is an array, a `Vec`, a tuple or
> a single expression; a computed sequence is collected into a `Vec` first).

## Parameters

> [spec:pgorm:req:pipeline.params+1]
> Values reach the SQL by exactly two routes, and the spelling says which.
> A Rust literal — `1`, `1.5`, `true`, `"text"` — converts into an `Expr` and
> is inlined into the SQL text, exactly as a literal written in PRQL text
> would be; it is a constant of the query, and prqlc escapes it when
> rendering, so a quote inside a string literal cannot close the literal it
> sits in. A runtime value goes through the `Binder`: the `_with` form of
> every expression-taking transform passes one to its closure, and
> `bind(value)` pushes the `pgorm_query::Value` and mints its `$N`
> placeholder in a single step, numbering in bind order across the whole
> pipeline. prqlc carries `ExprKind::Param` through lowering untouched —
> including inside `HAVING` and aggregate arguments, with no renumbering — so
> position `N` in the emitted SQL is position `N` in the returned `Values`,
> and a placeholder without its value (or a value without its placeholder)
> cannot be constructed.
>
> The binder and the expressions it returns are branded with a
> higher-ranked, invariant closure lifetime; an expression containing a
> bound placeholder is thereby pinned to the pipeline that minted it, and
> carrying it into another pipeline is a compile error, not a runtime check.
> Param-free expressions (columns, tokens, literals, `col`) are
> brand-polymorphic and reusable anywhere. The by-value transforms take
> `Expr<'static>`, a brand a bound expression can never satisfy, so the plain
> form of a transform cannot smuggle a placeholder in either. The same
> reasoning binds `Over`: a window spec holds lowered nodes and therefore
> erases the brand it was built from, so its keys are `ExprList<'static>` and
> a placeholder cannot enter a partition or a window ordering — where it
> would mean nothing anyway.
>
> `take` and `take_range` accept integers by value, not expressions: PRQL
> refuses a parameterized `LIMIT`, so the signature mirrors the one form
> that compiles.

## Failure model

> [spec:pgorm:req:pipeline.errors+1]
> Pipeline construction is infallible; `into_sql()` is the fallible
> boundary, returning `PipelineError` and never panicking, per
> `[dec:pgorm:no-panic]`. Two variants: `ReservedAlias(name)` — a name given
> to `as_` collides with the closed reserved set (the top-level bindings of
> prqlc 0.13's `std` module, its submodule names `math` / `text` / `date`,
> and the PRQL keywords), screened before compilation so the collision is
> named instead of surfacing as an opaque resolution failure — and
> `Compile(diagnostics)`, carrying prqlc's own rendered diagnostics for
> everything its resolver rejects (a `std` name used as a value, ill-typed
> stages). `From<PipelineError> for Error` lifts into `Error::Query` so the
> terminals can fail through the ordinary channel.
>
> Compilation has no catalog, and this is the honest ceiling of the alias
> token: a token whose name no stage introduced still compiles, resolving as
> a column of the source relation, and whether that column exists is the
> server's question, answered at execution like any other raw SQL. Binding a
> token to the projection that declares it is deliberately not modelled in
> the type system — no brand ties a reference to its declaration — so the
> guarantee the token buys is that the name is written once, not that it was
> attached. Compile-time name checking covers only what prqlc can see: its
> own std namespace and pipeline-introduced names.

## Qualification

> [spec:pgorm:sem:pipeline.qualify+1]
> Column references are table-qualified by construction. An entity column
> already carries its entity, so `Into<Expr>` for `ColumnTrait` recovers the
> qualification from `entity_name()` rather than making the caller restate
> it, and a bare `order::Column::Total` mints the two-part identifier. The
> unqualified form — ambiguous the moment a join appears — is never
> constructed from a column. `col(table, column)` remains for the tables an
> entity does not describe and for disambiguation, taking an `Iden` pair.
>
> `IntoSource` is what a pipeline reads from, and there are three: an
> `EntityTrait` entity (which contributes its `table_name` and, when it has
> one, its `EntityName::schema_name`, so an entity source is schema-correct
> without a second spelling), an `AliasName` token, and an `Alias`.
> `from_schema(schema, table)` schema-qualifies a table no entity describes.
> A schema rides the identifier path through prqlc's `default_db` namespace
> and renders as `schema.table` — probed against the grammar oracle, with no
> known limitation. Identifier quoting is prqlc's: names needing quotes
> (spaces, case, reserved words such as a table named `order`) render
> double-quoted. An `AliasName` token used as an expression is the one
> bare-identifier form, reserved for names the pipeline itself introduced.

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
