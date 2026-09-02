# Query building and batched loading

This chapter covers `src/query/`: the fluent builders that turn entities and
ActiveModels into PostgreSQL statements (`select.rs`, `insert.rs`, `update.rs`,
`delete.rs`, `join.rs`, `combine.rs`, `helper.rs`, `traits.rs`, `util.rs`) and
the data-loader API (`loader.rs`). Rules are grouped under
`[spec:pgorm:req:query.build]` and `[spec:pgorm:req:query.loader]`.

## Query building

> [spec:pgorm:req:query.build]
> The query layer MUST provide fluent, owned-`self` builders for SELECT
> (`Select<E>`, `SelectTwo<E, F>`, `SelectTwoMany<E, F>`), INSERT (`Insert<A>`,
> `TryInsert<A>`), UPDATE (`Update`, `UpdateOne<A>`, `UpdateMany<E>`) and DELETE
> (`Delete`, `DeleteOne<A>`, `DeleteMany<E>`). Each builder wraps exactly one
> pgorm-query statement (`SelectStatement`, `InsertStatement`,
> `UpdateStatement`, `DeleteStatement`) and exposes it through `QueryTrait`;
> the shared query-modification surface is provided by the blanket traits
> `QuerySelect`, `QueryOrder` and `QueryFilter` over a `query()` accessor
> returning the underlying statement.

Construction of a plain selector happens in `Select::new` (`select.rs`), which
is what `EntityTrait::find()` produces.

> [spec:pgorm:sem:query.build.select-defaults]
> `Select<E>::new` initialises the statement with exactly two clauses: a select
> list containing every variant of `E::Column` in iteration order, each mapped
> through `col.select_as(col.into_expr())` (so enum-typed columns are cast to
> text on selection), and a FROM clause of `E::default().table_ref()`.
>
> No default WHERE, ORDER BY, GROUP BY, LIMIT or OFFSET is applied; a freshly
> constructed `Select<E>` renders as `SELECT <all columns> FROM <table>`.

> [spec:pgorm:def:query.build.query-trait]
> `QueryTrait` is the common build surface: `query()` (mutable access),
> `as_query()` (shared access), `into_query()` (ownership) and
> `build()`, which renders the statement to a `(String, Values)` pair via
> `pgorm_query::QueryBuilder` — there is no backend parameter; only PostgreSQL
> syntax is produced. `QueryTrait::apply_if(Option<T>, f)` applies `f` to the
> builder only when the option is `Some`, enabling conditional query
> construction without breaking the fluent chain.
>
> `IntoSimpleExpr` (in `select.rs`) is the conversion bound used by column
> positions: it is implemented for every `ColumnTrait` type (producing a
> column reference), for `Expr` and for `SimpleExpr` (identity).

> [spec:pgorm:sem:query.build.filter]
> `QueryFilter::filter` converts its argument through `IntoCondition` and adds
> it with `cond_where`; repeated `filter` calls accumulate as AND-ed
> conditions. Condition trees (`Condition::any()` / `Condition::all()`,
> including `add_option` for runtime-optional predicates) and raw
> pgorm-query expressions are accepted through the same entry point, and
> `ColumnTrait` operators (`eq`, `is_in`, `contains`, ...) are the usual
> operand source.
>
> `QueryFilter::belongs_to(model)` adds one equality filter per primary-key
> column of the model's entity (`col.eq(model.get(col))`);
> `belongs_to_tbl_alias` does the same but qualifies the columns with a given
> table alias string.

> [spec:pgorm:sem:query.build.modifiers+1]
> `QuerySelect` mutates the select statement in place: `select_only()` clears
> the entire select list; `column` appends a column through
> `col.select_as(col.into_expr())` (same enum-cast rule as the default list);
> `columns` iterates `column`; `column_as` / `expr_as` / `tbl_col_as` append
> an expression with an explicit alias; `expr` / `exprs` append raw select
> expressions. `offset` and `limit` take `Into<Option<u64>>`: `Some(n)` sets
> the clause (last call wins), `None` removes it. `group_by` adds a GROUP BY
> expression, `having` accumulates AND-ed HAVING conditions, `distinct` /
> `distinct_on` add DISTINCT / DISTINCT ON, and `lock`, `lock_shared`,
> `lock_exclusive` and `lock_with_behavior` add row-locking clauses.
> `SelectColumns` (in `traits.rs`) re-exposes `column`/`column_as` as
> `select_column`/`select_column_as` for partial-model queries.
>
> `select_only()` therefore leaves the statement in a state that renders as
> `SELECT  FROM "tbl"` until a column or expression is re-added, and rendering
> keeps emitting exactly that: `to_string` / `build` have no `Result` channel,
> so they are not where the mistake is caught. The guard is at the execution
> boundary instead. Every ORM path that would send a SELECT whose projection
> list is empty MUST return
> `DbErr::Query(RuntimeErr::Internal("select list is empty; add at least one
> column or expression"))` before any statement reaches the server: the paths
> are `Selector::one` / `one_opt` / `all` / `stream` (and everything routed
> through them, including `Select::all`, `SelectTwo`/`SelectTwoMany`,
> `into_tuple`, `into_values`, `into_model` and `into_partial_model`),
> `Paginator::fetch_page` (so also `fetch`, `fetch_and_next` and
> `into_stream`), `Paginator::num_items` — which checks the *inner* query it is
> about to wrap in `SELECT COUNT(*) FROM (…)`, since the wrapper's own
> projection is never empty — and so `num_pages`, `num_items_and_pages` and
> `PaginatorTrait::count`, and `Cursor::all`. `SelectorRaw` is exempt: its
> statement is a caller-supplied string, not a projection list. A statement
> with a non-empty projection is unaffected.
>
> `QueryOrder` appends ORDER BY expressions in call order (`order_by` with an
> explicit `Order`, `order_by_asc`, `order_by_desc`, and
> `order_by_with_nulls` for `NULLS FIRST`/`LAST`); calls accumulate and are
> never deduplicated.

Joins are derived from `RelationDef` (`helper.rs` bottom half plus
`join.rs`).

> [spec:pgorm:sem:query.build.join]
> `QuerySelect::join(join_type, rel)` joins `rel.to_tbl`;
> `join_rev` joins `rel.from_tbl`; `join_as` / `join_as_rev` first re-alias
> the joined table with a caller-supplied identifier. The ON condition is
> computed by `join_condition(rel)`: each side's identifier is the table alias
> if the `TableRef` carries one, otherwise the bare table identifier; the
> `from_col`/`to_col` `Identity` values are zipped into pairwise
> `from.col = to.col` equalities under `Condition::all()` or
> `Condition::any()` according to `rel.condition_type`; and any
> `rel.on_condition` closure is evaluated with the two identifiers and AND-ed
> in.
>
> The `Related`-driven helpers on `Select<E>` — `left_join`, `right_join`,
> `inner_join` — call `join_join(type, E::to(), E::via())`: when a junction
> relation `via` exists it is joined first, then the target relation.
> `reverse_join(R)` performs an INNER JOIN using `R::to()` in the reverse
> direction.

> [spec:pgorm:sem:query.build.combine]
> `SelectTwo`/`SelectTwoMany` use a fixed column-aliasing scheme
> (`combine.rs`): `select_also(F)` / `select_with(F)` first rewrite every
> select expression of `E` with the `A_` prefix via `apply_alias` (an existing
> alias becomes `A_<alias>`; an unaliased entry must be a plain column or an
> `AsEnum`-wrapped column, whose name becomes `A_<column>`; any other
> expression, including asterisks, panics), then append every `F::Column` as
> `<select_as expr> AS B_<column>`.
>
> `SelectTwoMany::new` additionally appends `ORDER BY <E primary key> ASC`
> for each primary-key column, so that consecutive rows for the same left
> model are adjacent; `SelectTwo` adds no ordering. `find_also_related(R)` is
> exactly `left_join(R).select_also(R)` and `find_with_related(R)` is
> `left_join(R).select_with(R)`.
>
> `find_also_linked` / `find_with_linked` walk a `Linked` chain instead: the
> i-th relation is LEFT JOINed with its target aliased `r{i}` (the join
> source being `r{i-1}`, or the base table for i = 0), custom `on_condition`
> closures included; `E`'s columns get the `A_` prefix and the final target's
> columns are selected from the last alias as `B_<column>`. Unlike
> `find_with_related`, `find_with_linked` does not append the primary-key
> ORDER BY (it constructs the selector with `new_without_prepare`).

INSERT building lives in `insert.rs`; the ActiveModel column rules below are
what makes it total over partially-set models.

> [spec:pgorm:sem:query.build.insert]
> `Insert::<A>::new` targets `A::Entity`'s table and applies
> `or_default_values()`, so a builder to which no model was ever added still
> renders a valid default-values INSERT rather than invalid SQL. `Insert::one`
> and `Insert::many` (and `add`/`add_many`) accept anything implementing
> `IntoActiveModel<A>`, converting Models to ActiveModels first.
>
> `add` iterates every `A::Entity` column in order: `Set` and `Unchanged`
> values are included (each value passed through `col.save_as(...)`, applying
> any save-time cast); `NotSet` columns are omitted from the column and value
> lists entirely. When the entity's primary key is not auto-increment, the
> model's primary-key value tuple is captured on the builder (used later to
> report `last_insert_id`); for auto-increment keys it is left `None`.
> `on_conflict` attaches a pgorm-query `OnConflict` clause verbatim.

> [spec:pgorm:req:query.build.insert.uniform-columns]
> All models added to a single `Insert` MUST have the same set of present
> (`Set` or `Unchanged`) columns. The first model added records a per-column
> presence bitmap; any subsequent model whose presence differs for any column
> causes `add` to panic with `"columns mismatch"`. Rows with heterogeneous
> column sets are not merged into a column union.

> [spec:pgorm:sem:query.build.insert.empty-failsafe]
> `TryInsert<A>` wraps an `Insert<A>` and is the failsafe form:
> `Insert::do_nothing()` and its alias `on_empty_do_nothing()` convert without
> altering the statement, while `Insert::on_conflict_do_nothing()` first
> attaches `ON CONFLICT (<primary key columns>) DO NOTHING` and then converts.
> Every `TryInsert` execution path (`exec`, `exec_without_returning`,
> `exec_with_returning`) first checks the recorded column bitmap: if no
> columns were ever added (e.g. `insert_many` over an empty iterator), it
> returns `TryInsertResult::Empty` without sending any SQL, leaving the
> database untouched. A `DbErr::RecordNotInserted` from the underlying insert
> is mapped to `TryInsertResult::Conflicted`; success wraps the result in
> `TryInsertResult::Inserted`.

> [spec:pgorm:sem:query.build.update+2]
> `Update::one(model)` builds an `UpdateOne<A>` in two passes over the
> ActiveModel and returns `Result<UpdateOne<A>, DbErr>`. Filters: every
> primary-key column contributes a `WHERE pk = value` equality from a `Set`
> or `Unchanged` value; a `NotSet` primary key aborts the build with
> `Err(DbErr::PrimaryKeyNotSet)` rather than panicking, so an `UpdateOne`
> can never exist without a filter on every primary-key column. Values: only
> `Set`, non-primary-key columns are written into the SET clause (through
> `col.save_as`); `Unchanged` and `NotSet` columns are omitted, so only
> changed values are updated and primary keys are never SET by `UpdateOne`.
> `EntityTrait::update` forwards both the success and the error.
>
> `Update::many(entity)` builds a bare `UPDATE <table>` with no implicit
> filter; `set(model)` applies the same `Set`-only rule but does not exclude
> primary-key columns, and `col_expr(col, expr)` sets a raw expression. Both
> forms implement `QueryFilter` for WHERE clauses.

> [spec:pgorm:sem:query.build.delete+1]
> `Delete::one(model)` converts through `IntoActiveModel`, targets the
> entity's table and returns `Result<DeleteOne<A>, DbErr>`: every primary-key
> column contributes a `WHERE pk = value` equality from a `Set` or
> `Unchanged` value; a `NotSet` primary key aborts the build with
> `Err(DbErr::PrimaryKeyNotSet)` rather than panicking, so a `DeleteOne` can
> never exist without a filter on every primary-key column and this path
> cannot render an unfiltered `DELETE`. Non-key attribute values do not
> participate in the filter. `EntityTrait::delete` forwards both the success
> and the error. `Delete::many(entity)` builds a bare `DELETE FROM <table>`;
> constraining it is the caller's job via `QueryFilter`.

> [spec:pgorm:def:query.build.debug-query]
> `DebugQuery<'a, Q, T>` (`util.rs`) is a plain holder of a `&Q` query and a
> value, paired with the `debug_query_stmt!` and `debug_query!` macros that
> expand to constructing a `DebugQuery` and calling `.build()` on it.
> Limitation: every `debug_query_build!` invocation that would generate the
> per-value `build` impls is commented out in the current source, so
> `DebugQuery` has no methods and the two macros have no working `build`
> target; the type is vestigial and raw SQL is obtained via
> `QueryTrait::build()` instead.

## Batched loading

> [spec:pgorm:req:query.loader]
> The loader layer MUST provide `LoaderTrait`, implemented for `Vec<M>` and
> `&[M]` (the `Vec` impl delegating to the slice impl), with three batched
> eager-loading operations: `load_one` for has-one relations
> (`Vec<Option<R::Model>>`), `load_many` for has-many relations
> (`Vec<Vec<R::Model>>`) and `load_many_to_many` for junction-mediated
> relations (`Vec<Vec<R::Model>>`). Each accepts either a bare entity or a
> pre-filtered `Select<R>` through `EntityOrSelect` (a bare entity becomes
> `E::find()`), and each returns results positionally aligned with the input
> slice.
>
> Relation shape is validated before querying: `load_one` errors if the
> relation has a `via` junction or is `HasMany`; `load_many` errors if it has
> a `via` junction or is `HasOne`; `load_many_to_many` errors if there is no
> `via` junction, if the target relation is not `HasOne`, or if the passed
> junction entity's table ref differs from the relation's junction (compared
> by `Debug` formatting). An empty input slice short-circuits to an empty
> result without querying.

> [spec:pgorm:sem:query.loader.batching+1]
> Keys are collected in input order: for each input model, `extract_key`
> builds a `ValueTuple` from the relation's `from_col` `Identity` (unary,
> binary, ternary or many), resolving each column name back to the entity's
> `Column` enum via `FromStr`. A name that does not map is a caller-authored
> relation naming a column its model does not have, so `extract_key` MUST
> return `Err(DbErr::Query)` naming the unresolved column and the model's
> table rather than panicking, and the load aborts with that error. The
> batch filter built by `prepare_condition` is a single IN predicate against
> the relation's `to_col` on `to_tbl`: a unary key becomes
> `col IN (v1, v2, ...)` over the flattened values; composite keys become a
> tuple expression `(a, b, ...) IN ((..), (..))` via `in_tuples`;
> `prepare_condition` is likewise fallible, propagating the qualification
> error of [spec:pgorm:req:query.loader.table-ref-limitation].
>
> Keys are not deduplicated: duplicate key values across input models are
> repeated verbatim in the IN list (the dedup is an acknowledged TODO in
> `prepare_condition`; current behaviour sends the duplicates). The condition
> is AND-ed onto the caller-supplied `Select` via `QueryFilter::filter`, so
> user filters and the key predicate compose.

> [spec:pgorm:sem:query.loader.regroup+1]
> Results are regrouped to input order by hashing on the extracted `to_col`
> key of each returned row. `load_one` builds a `HashMap<ValueTuple, Model>`
> — if several returned rows share a key, the last row wins — and yields, per
> input key, `Some(model.clone())` or `None`; inputs sharing a key each
> receive a clone of the same model. `load_many` seeds the map with an empty
> `Vec` per input key, pushes each returned row onto its key's bucket in
> result order, and yields a clone of the bucket per input key — so inputs
> sharing a key receive duplicated vectors, and unmatched inputs receive an
> empty `Vec`.
>
> A returned row whose key is absent from the seeded map means the relation's
> two sides matched in SQL but not as Rust values — differing integer widths,
> `char(n)` blank padding, a case-insensitive collation. `load_many` MUST
> report that as `Err(DbErr::Query)` rather than panicking, and the message
> MUST carry the unmatched key and a sample input key in `Debug` form (so both
> value types are named) together with the `from_col` and `to_col` column
> lists, making the asymmetry diagnosable from the error alone.

> [spec:pgorm:sem:query.loader.many-to-many]
> `load_many_to_many` issues two queries. First the junction entity is loaded
> with `V::find()` filtered on the via-relation's `to_col` against the input
> primary keys, building a key map from each input key to the list of target
> foreign keys in junction-row order. Second, the target selector is filtered
> on the target relation's `to_col` against all collected foreign keys (their
> order is the flattening of a `HashMap`'s values, hence unspecified) and the
> returned models are indexed by key, last row winning on duplicates. The
> result maps each input key to its foreign-key list resolved against that
> index via `filter_map`, so foreign keys whose target row was not returned
> (e.g. filtered out by the caller's `Select`) are silently dropped, and
> shared targets are cloned per referencing input.

> [spec:pgorm:req:query.loader.table-ref-limitation+1]
> Loader key predicates can only qualify columns for `TableRef::Table` and
> `TableRef::SchemaTable` relation targets: `table_column` matches exactly
> those two variants and MUST return `Err(DbErr::Query)` for every other
> variant (aliased, database-qualified, subquery, values-list or
> function-call table refs), naming the key column it could not qualify and
> the offending table reference. `prepare_condition` propagates that error,
> so the load aborts with an `Err` and not a panic. Entities whose relations
> resolve to such table refs still cannot be loaded through `LoaderTrait`;
> the limitation is unchanged, only its reporting.
