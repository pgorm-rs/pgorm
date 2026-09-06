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

> [spec:pgorm:req:pipeline.surface+3]
> A `Pipeline` is constructed by `from(impl IntoSource)` or
> `from_schema(schema, table)` — the only entry points, so a sourceless
> pipeline is unrepresentable — and grown one whole transform at a time:
> `filter`, `derive`, `select`, `group(keys)` followed by `aggregate(aggs)`,
> `window(columns, over)`, `sort`, `take(i64)`,
> `take_range(RangeInclusive<i64>)` (1-based, inclusive),
> `join(JoinSide, relation, condition)` with an explicit condition only, the
> set operations `append` / `intersect` / `remove` over another relation,
> and `distinct` ([spec:pgorm:req:pipeline.compose]). Every relation
> position — the source, the join operand, a set-operation operand — takes
> `impl IntoSource`, and a whole `Pipeline` is an `IntoSource`, so pipelines
> compose with each other by the same spellings that name tables; any of
> them may be read under a name of its own, which is how a relation meets
> itself ([spec:pgorm:sem:pipeline.self-join]).
>
> Each transform has two forms. The plain one takes its expressions by value
> — the whole query reads as one chained expression, with constants written
> as Rust literals. The `_with` one takes a closure and hands it the `Binder`,
> and exists only where a runtime value has to enter
> (`[spec:pgorm:req:pipeline.params+3]`). The `_with` closures of the
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
> `[spec:pgorm:sem:pipeline.qualify+2]`), for an `AliasName` token, and for
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
> (`[spec:pgorm:def:sql.types+5]`) when anything refers back to it, and a
> bare `&'static str` when nothing does — `as_` takes `impl Into<AliasName>`
> and both spell the same thing. A token declared once by `let rn =
> alias("rn")` is also its own reference: it converts to an unqualified
> expression, so `filter(rn.lte(2))` refers to what `row_number().as_(rn)`
> introduced, and the name exists in the program exactly once. The token
> carries no evidence that it was ever attached: a reference to a name no
> stage introduced compiles, and the server answers for it
> (`[spec:pgorm:req:pipeline.errors+2]`).
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
> parameters; a finished pipeline composes with other pipelines whole, as a
> relation ([spec:pgorm:req:pipeline.compose]).
>
> Deliberately cut vocabulary (not quality): `loop`, s-strings and f-strings
> (raw SQL interpolation holes), the `text` / `date` / `math` std modules,
> range membership (`between`), `group`-scoped bodies other than `aggregate`
> and `window`, and iterator adapters as list arguments (an `ExprList` is an
> array, a `Vec`, a tuple or a single expression; a computed sequence is
> collected into a `Vec` first).

## Parameters

> [spec:pgorm:req:pipeline.params+3]
> Values reach the SQL by exactly two routes, and the spelling says which.
> A Rust literal — `1`, `1.5`, `true`, `"text"` — converts into an `Expr` and
> is inlined into the SQL text, exactly as a literal written in PRQL text
> would be; it is a constant of the query, and prqlc escapes it when
> rendering, so a quote inside a string literal cannot close the literal it
> sits in. A runtime value goes through the `Binder`: the `_with` form of
> every expression-taking transform passes one to its closure, and
> `bind(value)` pushes the `pgorm_query::Value` and mints its `$N`
> placeholder in a single step, numbering in bind order across the whole
> pipeline, so a placeholder without its value cannot be constructed. The
> converse can arise without being written: prqlc carries `ExprKind::Param`
> through lowering verbatim — including inside `HAVING` and aggregate
> arguments — but its optimizer MAY prune an expression nothing reads (a
> derived column no later stage keeps), and the placeholder vanishes with it
> while the bound value remains. `into_sql` therefore takes a census of the
> emitted SQL: `pg_query::scan`, the PostgreSQL lexer pgorm already links,
> whose `PARAM` tokens cannot be confused with a `$N` inside a string
> literal. Values whose placeholders were optimized away are discarded and
> the surviving placeholders renumber contiguously `$1..$K`, the SQL text
> rewritten and the `Values` compacted in one pass; a repeated placeholder
> keeps its value exactly once. The invariant the terminals rely on —
> position `N` in the emitted SQL is position `N` in the returned `Values` —
> is thus enforced at the boundary rather than assumed of the optimizer. The
> lexer census is complete here where it has to be argued for in `prql!`
> ([spec:pgorm:sem:macros.prql.census]): the pipeline has no s-strings
> (deliberately cut, `[spec:pgorm:req:pipeline.surface+3]`), so every
> placeholder in the emitted text is a
> `Param` node the binder minted, and the census can only ever find a subset
> of the minted numbers — never a foreign `$N` with no value behind it. A
> census that cannot be trusted (unscannable text, an out-of-range number;
> neither reachable from the builder's own output) passes the statement
> through unchanged for the server to judge, keeping `into_sql` panic-free
> (`[spec:pgorm:req:pipeline.errors+2]`).
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
> The one sanctioned crossing between pipelines is embedding a whole
> pipeline ([spec:pgorm:req:pipeline.compose]), which is sound where the
> expression crossing is not because the values move with their
> placeholders: the embedded pipeline's values append to the consumer's and
> its placeholders renumber by the same offset, so the alignment invariant —
> position `N` in the SQL is position `N` in the `Values` — is preserved
> across the merge.
>
> `take` and `take_range` accept integers by value, not expressions: PRQL
> refuses a parameterized `LIMIT`, so the signature mirrors the one form
> that compiles.

## Composition

> [spec:pgorm:req:pipeline.compose]
> A whole `Pipeline` is a relation: `IntoSource` is implemented for
> `Pipeline` by value, so another pipeline can read it (`from`), join it
> (`join`), or combine with it (`append` / `intersect` / `remove`). This is
> PRQL's own composition mechanism, `let` bindings: each embedded pipeline
> becomes a `let table_N = (...)` statement ahead of `main`, and prqlc
> lowers each referenced binding to a CTE — or inlines it where SQL allows,
> as it may for an `append` operand. `sort` and `take` inside an embedded
> pipeline stay inside its binding (rendered in the CTE), and PRQL's sticky
> `sort` carries outward to the reading pipeline, as it does in PRQL text.
>
> Embedding consumes the pipeline by value, and the consumption is the
> re-branding: the embedded pipeline's values append to the consumer's, and
> a rebase walk over its PL nodes shifts every `$N` placeholder up by the
> count the consumer had already bound — prqlc passes `Param` nodes through
> verbatim, so renumbering at PL-construction time is this module's job —
> and shifts its own binding references past the bindings the consumer
> already holds. Values and placeholders therefore cross together and stay
> aligned; an unattached `Expr` still cannot cross
> (`[spec:pgorm:req:pipeline.params+3]`). Nesting is unbounded: an embedded
> pipeline's own embeddings ride along, renumbered the same way.
>
> The binding names are minted by the module — `table_N`, by position — and
> are internal: no API takes or returns one, and callers refer to an
> embedded pipeline's columns by their own names instead. prqlc mints its
> wrapping CTEs in the same `table_N` namespace and steps around taken
> names, so the two sequences coexist; the namespace is reserved, and a real
> table sharing a minted name is shadowed by the binding. A name the
> embedded pipeline introduced with `as_` is referred to unqualified
> (the alias token); a name known to exactly one side of a join resolves to
> that side even unqualified; a name both sides export is an ambiguity prqlc
> refuses at `into_sql`, naming the candidates. In the join condition —
> where neither an embedded relation nor a mid-pipeline consumer has a name
> to qualify by — `this(column)` and `that(column)` qualify by role, PRQL's
> own `this` / `that`, and are scoped to that condition. Alias tokens
> declared in two composed pipelines never collide: each lives in its own
> binding's scope, and the same name may be introduced in both.
>
> The set operations correspond by column position and emit the `ALL` forms:
> `append` is `UNION ALL`, `intersect` is `INTERSECT ALL`, `remove` is
> `EXCEPT ALL` (each row of the operand cancels one matching row, not all).
> `distinct` is PRQL's `group this (take 1)`, rendered `SELECT DISTINCT`,
> and folded by prqlc into `UNION DISTINCT` when it directly follows
> `append`. When both projections are visible to prqlc a column-count
> mismatch is refused at `into_sql`; relations with wildcard projections
> are the server's to check. After `intersect` or `remove` the combined
> relation is renamed, so later stages refer to its columns by bare name —
> an entity-qualified reference no longer resolves — while after `append`
> the left side's naming survives.

## Self-joins

> [spec:pgorm:sem:pipeline.self-join]
> A relation meets itself by being read twice under two names.
> `IntoSource::named(name)` is a provided method on the trait every relation
> position already takes, so `join(side, employee::Entity.named(manager),
> condition)` needs no second spelling of `join`, no aliased table type and
> no new expression form: the name is an `AliasName` token, and
> `col(manager, ID)` — the disambiguating reference that already existed —
> is how the far side's columns are written. It lowers to PRQL's own operand
> alias (`join m=employee`) and renders as SQL's, `employee AS manager`,
> with no CTE: the employee-manager query is one join stage, selecting
> `Column::Name` beside `col(manager, NAME)`.
>
> Naming a relation replaces the name it had, exactly as SQL's `AS` does:
> after `employee::Entity.named(manager)` the reference `employee.name` no
> longer resolves, and prqlc refuses it by name
> ([spec:pgorm:req:pipeline.errors+2]). The ordinary shape therefore names
> only the second occurrence and leaves the reading pipeline
> entity-qualified. The name then reaches every stage after the join —
> `filter`, `sort`, `select` — which is what distinguishes it from `this` /
> `that`, scoped to the condition alone
> ([spec:pgorm:req:pipeline.compose]); and names chain, so a third and
> fourth occurrence of one table are the same motion.
>
> A whole `Pipeline` is a relation, so an embedded pipeline takes a name the
> same way: the binding still lowers to a CTE and the name attaches to the
> reference (`table_0 AS manager`). Renaming the far side's columns with
> `as_` inside the embedded pipeline, and reading them unqualified
> afterwards, remains a working spelling and is the one composition already
> gave; it costs a rename per column crossed, where a name costs one per
> relation, and it changes the embedded projection to do it. A name is
> screened against the reserved set like any `as_` name, and shares the
> `table_N` namespace the module mints its bindings in — reserved for that
> reason ([spec:pgorm:req:pipeline.compose]).

## Failure model

> [spec:pgorm:req:pipeline.errors+2]
> Pipeline construction is infallible; `into_sql()` is the fallible
> boundary, returning `PipelineError` and never panicking, per
> `[dec:pgorm:no-panic]`. Three variants: `ReservedAlias(name)` — a name
> given to `as_` collides with the closed reserved set (the top-level
> bindings of prqlc 0.13's `std` module, its submodule names `math` /
> `text` / `date`, and the PRQL keywords), screened before compilation so
> the collision is named instead of surfacing as an opaque resolution
> failure; `Compile(diagnostics)`, carrying prqlc's own rendered
> diagnostics for everything its resolver rejects (a `std` name used as a
> value, ill-typed stages); and `ReshapedSources(stage)` — the
> `select_sources` terminal (`[spec:pgorm:sem:pipeline.select-sources]`)
> was asked to project entity models out of a pipeline whose sources'
> column namespaces a `select`, `group().aggregate()`, `intersect` or
> `remove` stage had already replaced, refused before prqlc compiles and
> naming the stage. `From<PipelineError> for Error` lifts into
> `Error::Query` so the terminals can fail through the ordinary channel.
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

> [spec:pgorm:sem:pipeline.qualify+2]
> Column references are table-qualified by construction. An entity column
> already carries its entity, so `Into<Expr>` for `ColumnTrait` recovers the
> qualification from `entity_name()` rather than making the caller restate
> it, and a bare `order::Column::Total` mints the two-part identifier. The
> unqualified form — ambiguous the moment a join appears — is never
> constructed from a column. `col(table, column)` remains for the tables an
> entity does not describe and for disambiguation, taking an `Iden` pair.
>
> `IntoSource` is any relation a pipeline can read — the source, the join
> operand, a set-operation operand — and there are four: an `EntityTrait`
> entity (which contributes its `table_name` and, when it has one, its
> `EntityName::schema_name`, so an entity source is schema-correct without a
> second spelling), an `AliasName` token, an `Alias`, and a whole `Pipeline`
> ([spec:pgorm:req:pipeline.compose]). `into_source` yields the opaque
> `Source` carrier, whose contents only the pipeline module can construct,
> so the set of relation shapes is closed.
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

> [spec:pgorm:sem:pipeline.select-sources]
> `select_sources(sources)` is the model-decode terminal: where
> `into_model::<M>` asks the caller for a row type whose projection the
> caller must have arranged, `select_sources` takes the relations
> themselves and arranges the projection. It is the pipeline's complement
> to the source graph — row-shaped reads that need what the graph excludes
> (right and full joins, aggregates beside models in a later `derive`,
> arbitrary composed relations) are written here; reads the graph's slot
> typing can carry belong there (`[spec:pgorm:def:query.graph]`).
>
> The argument is a single source or a tuple of up to six — each an
> `IntoSource` some stage of the pipeline read (the `from` source or a
> join operand), restated: an entity for a relation read bare, the same
> `named(..)` spelling for a relation read under a name
> (`[spec:pgorm:sem:pipeline.self-join]`), so two occurrences of one table
> are told apart exactly as the join told them apart. Before compilation
> the terminal appends one final projection stage through the same writer
> as the graph's (`[spec:pgorm:sem:query.graph.writer+1]`): for the i-th
> listed source (zero-based), every column of its entity in iteration
> order, projected `col.select_as(..)` and aliased `s{i}_{col}`,
> qualified by the source's name — the `named` token, or the entity's own
> qualification (`[spec:pgorm:sem:pipeline.qualify+2]`). An explicitly
> aliased projection is what dissolves prqlc's `_expr_N` renaming: two
> sources sharing a column name land under different prefixes by
> construction, so the compiler never has to invent names the decode
> cannot predict.
>
> Every listed source decodes through the absence witness
> (`[spec:pgorm:req:exec.decode.absent]`) as `Option<Model>` — every
> position, the first included. The pipeline's joins carry no missability
> in their types, and under a right or full join the *left* side is the
> absent one, so the terminal claims nothing a join could falsify: the
> row type is `(Option<E1::Model>, …, Option<En::Model>)`, and a source
> listed alone still decodes as `Option<E1::Model>`. Callers who can
> prove a side present unwrap it; the graph is the surface whose types
> state it.
>
> `select_sources` yields a `SelectedSources`, a terminal-only value on
> the pattern of `Grouped`: its only methods are the terminals —
> `into_sql()`, `all(db)`, `one(db)`, `one_opt(db)`, with the `take 1`
> semantics of `[spec:pgorm:sem:pipeline.terminal]` — so no transform can
> follow the selection and reshaping *after* it is unrepresentable.
> Reshaping *before* it is refused at `into_sql`: a pipeline that already
> passed through `select`, `group(..).aggregate(..)`, `intersect` or
> `remove` no longer carries the sources' own column namespaces (a
> projection replaced them, an aggregation collapsed them, a set-op
> rename dissolved the entity qualification,
> `[spec:pgorm:req:pipeline.compose]`), and the terminal MUST fail with a
> typed `PipelineError` variant naming the offending stage — before prqlc
> compiles, so the caller reads "select_sources after <stage>" rather
> than an opaque unresolved-name diagnostic. `filter`, `derive`, `sort`,
> `take` / `take_range`, `join`, `window`, `distinct` and `append` (whose
> left-side naming survives) leave every source addressable and compose
> freely ahead of the terminal; construction itself stays infallible per
> `[spec:pgorm:req:pipeline.errors+2]`.
>
> The catalog-less ceiling stands: a listed source the pipeline never
> read compiles up to prqlc, which refuses the unresolvable columns as
> `Compile(diagnostics)` — the terminal checks stage shape, not
> membership. And as everywhere the witness reads, a matched row whose
> every projected column is NULL is indistinguishable from an unmatched
> one only when the projection omits the primary key, which this
> projection never does.
