# Query building and batched loading

This chapter covers `src/query/`: the fluent builders that turn entities and
ActiveModels into PostgreSQL statements (`select.rs`, `insert.rs`, `update.rs`,
`delete.rs`, `join.rs`, `combine.rs`, `helper.rs`, `traits.rs`, `util.rs`) and
the data-loader API (`loader.rs`). Rules are grouped under
`[spec:pgorm:req:query.build]` and `[spec:pgorm:req:query.loader+1]`.

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

> [spec:pgorm:sem:query.build.filter+1]
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
> table alias. That alias is taken as `impl IntoIden`, not as a `&str`: its
> in-tree caller is `ModelTrait::find_linked`, which has a `LinkedAlias`
> (`[spec:pgorm:req:entity.relation.linked+2]`) in hand and would otherwise
> have to render it back to a string for the callee to parse into an
> identifier again. A string literal still passes, through
> `IntoIden for &str`.

> [spec:pgorm:sem:query.build.modifiers+6]
> `QuerySelect` mutates the select statement in place: `column` appends a
> column through `col.select_as(col.into_expr())` (same enum-cast rule as the
> default list); `columns` iterates it; `column_as` / `expr_as` /
> `tbl_col_as` append an expression with an explicit alias; `expr` / `exprs`
> append raw select expressions. `offset` and `limit` take
> `Into<Option<u64>>`: `Some(n)` sets the clause (last call wins), `None`
> removes it. `group_by` adds a GROUP BY expression, `having` accumulates
> AND-ed HAVING conditions, `distinct` / `distinct_on` add DISTINCT /
> DISTINCT ON, and `lock`, `lock_shared`, `lock_exclusive` and
> `lock_with_behavior` add row-locking clauses. The composition clauses join
> them on the same terms — `with_cte` / `with_recursive_cte`
> (`query.build.with`), `join_lateral` / `join_lateral_on_true`
> (`query.build.lateral`), `window` and `window_expr_as`
> (`query.build.window`), and `union` (`query.build.union`) — each a default
> method mutating the statement and returning `Self`, except the projecting
> `window_expr_as`, which returns `Projected`.
>
> There MUST be one name per projection operation. `expr_as_` (a trailing-
> underscore duplicate of `expr_as`, kept "for legacy reasons" and called by
> nothing) is deleted, and so is the `SelectColumns` trait, which re-exposed
> `column`/`column_as` as `select_column`/`select_column_as` through a blanket
> impl over `QuerySelect` with an identical `Projected` fixpoint. It added a
> second vocabulary and no capability; `PartialModelTrait::select_cols` is
> generic over `QuerySelect` directly, and the typestate proof that a
> field-less `DerivePartialModel` cannot compile is unchanged because the
> fixpoint bound it relies on is `QuerySelect`'s own.
>
> Clearing the select list is a typestate transition, not an in-place
> mutation, and `select_only` is therefore NOT a `QuerySelect` method.
> `Select<E>::select_only()` returns `SelectCustom<E>` and
> `SelectTwo<E, F>::select_only()` returns `SelectTwoCustom<E, F>`;
> `SelectProjected<E>` and `SelectTwoProjected<E, F>` carry the same method
> to start a projection over. `SelectTwoMany<E, F>` and `Cursor<S, K>` —
> whose select lists their own machinery owns — cannot clear one at all.
>
> `select(items)` is that clear and the projection that follows it in one
> call, under the name the pipeline projects with
> (`[spec:pgorm:req:pipeline.surface+1]`), so the verb means the same thing
> on both surfaces. It is inherent on the same six states, for the reason
> `select_only` is: the destination is per-builder — `Select<E>`,
> `SelectCustom<E>` and `SelectProjected<E>` land on `SelectProjected<E>`,
> the two-model trio on `SelectTwoProjected<E, F>` — and a `QuerySelect`
> method returning `Projected` would hand `Select<E>`, `SelectTwoMany<E, F>`
> and `Cursor<S, K>` back their own `E::Model`-typed selves over a projection
> that is no longer that shape. `SelectTwoMany<E, F>` and `Cursor<S, K>`
> accordingly do not have it. `select_only`, `column` and `columns` are
> unchanged and stay: `select` is the one-call spelling of the pair, not a
> replacement, and appending to a projection — including the aliased
> `column_as` that chains after `select` — remains the pair's work.
>
> The argument is a `SelectList`, whose shapes are the pipeline's: a single
> item needs no wrapper, a homogeneous list is an array or a `Vec`, and a
> mixed list is a tuple of up to twelve items, because two entities' column
> enums have no common array element type. An item is a `SelectItem` — a
> `ColumnTrait` column, projected through `col.select_as(col.into_expr())` so
> the enum cast is the one `column` applies; an `Expr` or `SimpleExpr`; a
> `SelectExpr`, which carries its own alias; or an `AliasName`, a bare
> reference to a name an earlier clause bound. A list computed at run time is
> an iterator, which no tuple arity covers, so that case stays `select_only`
> plus `columns`.
>
> One name per projection operation covers the name `select` itself: the
> loader's `EntityOrSelect` conversion (`query.loader+1`), which spelled
> itself `select` and would be shadowed on `Select<E>` by the inherent
> method, is `into_select`.
>
> Every projection method returns `QuerySelect::Projected` rather than
> `Self`, under the fixpoint bound
> `Projected: QuerySelect<Projected = Self::Projected>`. `Select<E>`,
> `SelectTwo<E, F>`, `SelectTwoMany<E, F>`, `Cursor<S, K>`,
> `SelectProjected<E>` and `SelectTwoProjected<E, F>` project onto
> themselves; `SelectCustom<E>` projects onto `SelectProjected<E>` and
> `SelectTwoCustom<E, F>` onto `SelectTwoProjected<E, F>`. This associated
> type is what lets `PartialModelTrait::select_cols<S: QuerySelect>` return
> `S::Projected` (`entity.traits.from-query-result`).
>
> The two `Custom` states have no execution path at all: no `all` / `one` /
> `one_opt` / `stream`, no `into_model` / `into_tuple` / `into_values` /
> `into_partial_model`, no `paginate` / `count` / `cursor_by`. The two
> `Projected` states carry the terminals that name a decode target —
> `into_model`, `into_tuple`, `into_values`, `into_partial_model` (which
> re-clears the list, so the partial model owns the whole projection), and
> `cursor_by` plus `cursor_by_other` on the two-model form — but not the
> `E::Model`-typed ones, and not `select_also` / `select_with` /
> `find_also_related` / `find_with_related` / `find_also_linked` /
> `find_with_linked`: a caller's projection is neither `E::Model`'s shape nor
> carries the `A_`/`B_` aliases those need (`query.build.combine`).
> `SelectProjected::cursor_by` and `SelectTwoProjected::cursor_by` /
> `cursor_by_other` accordingly yield `Cursor<SelectUndecoded, K>`, which is
> not a `SelectorTrait` and so has no `all` until `Cursor::into_model` or
> `into_partial_model` names one.
>
> Both `Custom` states keep the whole of `QueryTrait`, `QueryFilter` and
> `QueryOrder`: a cleared select list still renders as `SELECT  FROM "tbl"`,
> and `to_string` / `build` have no `Result` channel, so rendering is not
> where the mistake is caught. The typestate keeps the ORM's own builders out
> of the empty-projection state, but two seams remain — an empty
> `columns([])` / `exprs([])` / `select([])` list, and a `SelectStatement` handed
> straight to `Selector::with_columns` / `Selector::into_tuple` /
> `Selector::from_select` (`exec.crud.selector-entry`) — so the
> execution-boundary guard stays. Every ORM path that would send a SELECT
> whose projection list is empty MUST return
> `Error::Query(RuntimeError::Internal("select list is empty; add at least one
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

> [spec:pgorm:sem:query.build.alias]
> A name the ORM's own call sites introduce MUST be written as the `AliasName`
> token (`[spec:pgorm:def:sql.types+5]`), not as a string repeated per site.
> Every aliasing and referencing position on the builders takes it through the
> existing conversions and needs no new one: the alias argument of `column_as`
> / `expr_as` / `tbl_col_as`, the expression positions of `having`, `group_by`,
> `order_by*` and `filter`, the qualifier of a `(table, column)` pair, the
> table alias of `join_as` / `join_as_rev` / `from_alias`, a lateral join's
> alias, a window name, and a CTE's name and columns. `alias` and `AliasName`
> are accordingly members of the prelude (`[spec:pgorm:def:entity.prelude+3]`).
>
> Reaching the `Identity` positions — `column_as`'s alias, `cursor_by`, a
> cursor's secondary ordering — takes one more impl, because `IntoIdentity`
> keys on the entity layer's `IdenStr` (`[spec:pgorm:def:entity.traits+1]`)
> rather than on `Iden`. `AliasName` implements it, so a single token spelling
> reaches every position in the crate and there is no position where a caller
> is pushed back to a string.
>
> `Alias` is NOT deprecated by this and MUST remain: a name computed at run
> time cannot be a `&'static str` token, and the ORM builds such names itself
> — the `A_`/`B_` column prefixes of the two-model selectors
> (`query.build.combine`), the loader's join-back alias, an entity's schema
> qualifier. The token is the paved road for the static case, not a
> replacement for the dynamic one.
>
> The ceiling is the same one the token carries at the query layer, and it is
> worth restating where a caller meets it: the token is evidence of nothing.
> `Cake::find().filter(alias("nope").into_column_ref()...)` — a token no
> projection ever declared — compiles, and the server rejects the unknown
> column exactly as it would a mistyped string. What the token removes is the
> second, unchecked spelling of a name that WAS declared; it does not make the
> declaration itself checkable.

Joins are derived from `RelationDef` (`helper.rs` bottom half plus
`join.rs`).

> [spec:pgorm:sem:query.build.join+3]
> `QuerySelect::join(join_type, rel)` joins `rel.to_tbl`;
> `join_rev` joins `rel.from_tbl`; `join_as` / `join_as_rev` first re-alias
> the joined table with a caller-supplied identifier. The ON condition is
> computed by `join_condition(rel)`: each side's identifier is
> `FromItem::qualifier()` — the bound alias if there is one, otherwise the bare
> table identifier; each `(from, to)` pair of `rel.columns` becomes one
> `from.col = to.col` equality under `Condition::all()` or
> `Condition::any()` according to `rel.condition_type`; and any
> `rel.on_condition` closure is evaluated with the two identifiers and AND-ed
> in. Because the columns are held as pairs
> (`[spec:pgorm:def:entity.relation.def+4]`), the join MUST constrain every
> column the relation declares: there are no two lists to reconcile and so no
> way to emit an under-constrained join.
>
> The `Related`-driven helpers on `Select<E>` — `left_join`, `right_join`,
> `inner_join` — call `join_join(type, E::to(), E::via())`: when a junction
> relation `via` exists it is joined first, then the target relation.
> `reverse_join(R)` performs an INNER JOIN using `R::to()` in the reverse
> direction. The relation-named forms of `join` / `join_rev` are
> `[spec:pgorm:sem:query.build.join.rel]`.

> [spec:pgorm:sem:query.build.join.rel]
> `QuerySelect` MUST also carry the join type in the method name and take the
> relation rather than its `RelationDef`: `left_join_rel`, `inner_join_rel` and
> `right_join_rel` are `join(JoinType::{Left,Inner,Right}Join, rel.def())`, and
> `left_join_rel_rev`, `inner_join_rel_rev`, `right_join_rel_rev` the same over
> `join_rev`. They are provided methods over any `R: RelationTrait`, so they
> reach every builder `join` does and emit the identical SQL — including
> whatever `on_condition` and `condition_type` the relation's own def carries.
>
> This is sugar, not a replacement: `join` / `join_rev` remain, and are still
> the way to join a `RelationDef` that was modified in flight (`.rev()`,
> `.on_condition(..)`) rather than taken whole from a relation.

> [spec:pgorm:sem:query.build.combine+2]
> `SelectTwo`/`SelectTwoMany` use a fixed column-aliasing scheme
> (`combine.rs`): `select_also(F)` / `select_with(F)` first rewrite every
> select expression of `E` with the `A_` prefix via `apply_alias` (an existing
> alias becomes `A_<alias>`; an unaliased plain column, or an
> `AsEnum`-wrapped one, becomes `A_<column>`), then append every `F::Column`
> as `<select_as expr> AS B_<column>`.
>
> An unaliased entry with no column name to take — an asterisk, or an
> expression that is neither a column nor an `AsEnum`-wrapped column — has no
> correct `A_` name, so `apply_alias` leaves it exactly as written and MUST
> NOT panic. Such an entry belongs to neither model and neither model's
> decode looks for it; the models' own columns are aliased either way.
>
> These combinators live on `Select<E>` only. A select whose projection was
> cleared by `select_only` does not carry them at all
> (`query.build.modifiers`), which is what keeps a wholly caller-authored
> select list — where every entry could be unaliasable — out of the scheme.
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
>
> Those `r{i}` names are generated but observable — a caller ordering by a
> column of the joined target has to name the table — so they are not a
> string the builders format and callers retype. They are `LinkedAlias`, and
> the last of them is derived by `Linked::last_hop_alias`
> (`[spec:pgorm:req:entity.relation.linked+2]`), which is the same call these
> two builders make. A chain that gains a hop therefore moves the join and
> every reference to it together.

The composition clauses — WITH, LATERAL, WINDOW and the set operators — are all
in-place mutations of the same `SelectStatement`, so none of them changes what a
builder is or what its rows decode into.

> [spec:pgorm:def:query.build.with]
> A WITH clause attaches to a SELECT by being *carried on it*:
> `SelectStatement` holds `with: Option<Box<AnyWithClause>>` and the statement
> renders its own prefix (`sql.render.select-order`). It MUST NOT be modelled as
> a wrapper around the select, because a wrapper erases the statement — the
> ORM's whole spine is a `SelectStatement`, and a value that has stopped being
> one can no longer take a filter, an ordering, a `LIMIT`, a projection or any
> typed terminal.
>
> Three setters write that one slot. `SelectStatement::with(clause)` takes
> `self` and returns `Self`, accepting either clause form through
> `Into<AnyWithClause>`; `with_cte(WithClause)` and
> `with_recursive_cte(RecursiveWithClause)` are the `&mut self` builder-style
> pair, named apart so the call site says which form it is building. All three
> overwrite: the last call wins and a select carries at most one clause
> (`query.build.with.single`).
>
> `QuerySelect` re-exposes the pair as owned-`self` default methods
> `with_cte` / `with_recursive_cte` returning `Self`, so every ORM builder over a
> `SelectStatement` — `Select<E>`, the projected and two-model states, `Cursor` —
> gains CTE support without a new type and without a new decode path. `Selector`,
> `SelectorRaw`, `Paginator` and `Cursor` inherit it unchanged, because the
> clause is already inside the statement they were always given.

> [spec:pgorm:sem:query.build.with.attach]
> The carried clause renders as a prefix of the statement at whatever level the
> statement occupies, so a select carrying one nests exactly like any other: as a
> FROM subquery, a union arm, a CTE body and a LATERAL body, all of which
> PostgreSQL parses. Nothing else about the statement changes — the builder keeps
> its type, the projection keeps its shape, and `filter`, `order_by`, `limit`,
> `join`, `join_lateral` and the projection combinators all still apply after the
> clause is attached.
>
> Because the clause rides *inside* the statement rather than around it, the
> execution terminals keep their semantics: `Selector::one` sets `LIMIT 1` on the
> carrying select and not on any CTE body (`exec.crud.select`), the
> empty-projection guard still inspects the carrying select's projection
> (`query.build.modifiers`), and `stream`, the paginator and the cursor work on a
> CTE query with no code of their own.
>
> A recursive CTE takes its column types from the anchor arm, where an
> unannotated placeholder resolves to `text`; annotating it is the caller's
> obligation under `sql.render.placeholder-typing`.

> [spec:pgorm:req:query.build.with.single]
> A WITH clause MUST have exactly one place to live on a SELECT. A carried clause
> and a clause wrapped around the same select would both render, producing
> `WITH … WITH … SELECT …`, which PostgreSQL does not parse — so the wrapping
> form is removed from the type system rather than guarded at runtime
> (`[dec:pgorm:invalid-states-unrepresentable]`).
>
> `WithQuery` therefore prefixes data-modifying statements only: its bound is the
> `WithBody` trait, implemented for `InsertStatement`, `UpdateStatement` and
> `DeleteStatement` and NOT for `SelectStatement` (nor for `WithQuery` itself,
> which would stack two prefixes on one statement). `WithQuery::new`,
> `WithClause::query` and `RecursiveWithClause::query` all take `T: WithBody`, so
> handing any of them a select is a compile error and the double-WITH render is
> unconstructible. `SelectStatement::with` accordingly returns `Self` rather than
> a `WithQuery`; the DML statements' `with` methods still return one, since they
> carry no clause of their own.

> [spec:pgorm:sem:query.build.lateral]
> `QuerySelect::join_lateral(join_type, sub, alias, on)` is the ORM name for
> `SelectStatement::join_lateral`: it appends the join in place and returns
> `Self`, so the builder's type — and the decode target with it — is unchanged.
> `join_lateral_on_true(join_type, sub, alias)` is the top-N-per-group spelling,
> where the correlation lives in the subquery's own WHERE and the join has
> nothing left to constrain; its ON condition is the inlined constant `TRUE`
> (`SimpleExpr::Constant`), never a bound parameter, because a bare `$n` in that
> position has no type to resolve from (`sql.render.placeholder-typing`).

> [spec:pgorm:sem:query.build.window]
> `QuerySelect::window(name, spec)` declares a named window in place and returns
> `Self`. `window_expr_as(func, window, alias)` projects
> `<func>(…) OVER <window> AS <alias>` and, being a projection, returns
> `QuerySelect::Projected` — it steps the `select_only` typestate forward exactly
> as `column` and `expr_as` do (`query.build.modifiers`). The window declaration
> is rendered at its own query level, ahead of the set operations and the
> ORDER BY/LIMIT tail (`sql.render.select-order`).

> [spec:pgorm:sem:query.build.union]
> `QuerySelect::union(union_type, other)` appends a set-operation arm, where
> `other` is of the *same builder type* as the receiver. That is the whole of the
> static guarantee available at this layer and it is exactly the right one: both
> arms are the same type, therefore the same projection in the same order,
> therefore the same row shape — and the combined result still decodes as
> whatever the first arm decoded as, which is also the arm PostgreSQL takes the
> result column names from.
>
> The method carries `where Self: QueryTrait<QueryStatement = SelectStatement>`
> so it can take the other arm's statement by value; a builder that is not one
> simply does not have it. `UnionType::All` / `Distinct` / `Intersect` / `Except`
> select the operator (`sql.ast.select.union`), and the arms accumulate rather
> than merge.

INSERT building lives in `insert.rs`; the ActiveModel column rules below are
what makes it total over partially-set models.

> [spec:pgorm:sem:query.build.insert+3]
> `Insert::<A>::new` targets `A::Entity`'s table and applies
> `or_default_values()`, so a builder to which no model was ever added still
> renders a valid default-values INSERT rather than invalid SQL. `Insert::one`
> and `Insert::many` (and `add`/`add_many`) accept anything implementing
> `IntoActiveModel<A>`, converting Models to ActiveModels first.
>
> `add` iterates every `A::Entity` column in order: `Set` and `Unchanged`
> values are included (each value passed through `col.save_as(...)`, applying
> any save-time cast); `NotSet` columns are omitted from the column and value
> lists entirely. A model that leaves every column `NotSet` therefore
> contributes no column list and no values row: instead of an arity-zero row
> it raises the statement's default-values row count, so `n` such models
> render `VALUES (DEFAULT)` repeated `n` times, one row of database defaults
> each. `on_conflict` attaches a pgorm-query `OnConflict` clause verbatim.
>
> `Insert` MUST NOT cache the added model's primary-key value tuple. It carried
> one — populated for non-auto-increment keys, last row winning, and read back
> by `exec_returning_pk` in place of the `RETURNING` row — but the key a caller
> asked to write is not the key of the row the database wrote, and an
> `ON CONFLICT DO UPDATE` landing on some other row made the difference
> observable as a primary key that names no row. The key is now resolved from
> `RETURNING` alone (`[spec:pgorm:sem:exec.crud.insert+4]`), so the builder has
> nothing to remember.

> [spec:pgorm:req:query.build.insert.uniform-columns+3]
> All models added to a single `Insert` MUST have the same set of present
> (`Set` or `Unchanged`) columns; rows with heterogeneous column sets are never
> merged into a column union. The first model added records a per-column
> presence bitmap and every later model is compared against it — including
> against the empty state, so a batch that opens with models setting nothing
> mismatches the first model that sets something.
>
> `add` returns `Self` so that calls chain, and therefore cannot report the
> disagreement where it finds it: a mismatching model is recorded as a third
> builder state naming the columns present in the earlier models and absent in
> it and vice versa, and contributes neither its columns nor its values to the
> statement. That state is terminal — models added after it are no longer
> compared — so a mismatch never panics and never renders a ragged VALUES list.
>
> `Insert::ensure_uniform_columns`, mirrored on `TryInsert`, reports the
> recorded state as `Err(Error::Query(RuntimeError::Internal(..)))` whose message
> names the offending columns on each side. Every execution path of both types
> (`exec`, `exec_returning_pk`, `exec_returning_model`) asks it first and
> fails with that error before any SQL is sent, so a mismatched batch leaves
> the database untouched.

> [spec:pgorm:sem:query.build.insert.empty-failsafe+3]
> `TryInsert<A>` wraps an `Insert<A>` and is the failsafe form:
> `Insert::on_empty_do_nothing()` converts without
> altering the statement, while `Insert::on_conflict_do_nothing()` first
> attaches `ON CONFLICT (<primary key columns>) DO NOTHING` and then converts.
>
> There MUST be exactly one conversion method. `do_nothing()` was a
> byte-identical twin of `on_empty_do_nothing()` and is deleted: the surviving
> name says *when* it does nothing, and dropping the shorter one removes a
> genuine collision, since `OnConflict::do_nothing()` appears in the same
> chains and means the opposite — a statement that runs and lets the server
> skip the row, rather than a statement never sent.
>
> Emptiness is a state the builder records, not a predicate re-derived at each
> execution. An `Insert` holds either the per-column presence bitmap of the
> first model added — which always marks at least one column present — or the
> empty state, reached both by adding no model at all (`Insert::many` over an
> empty iterator) and by adding only models that leave every column `NotSet`.
> All three `TryInsert` execution paths (`exec`, `exec_returning_pk`,
> `exec_returning_model`) read that one state, so an all-`NotSet` model reports
> `TryInsertResult::Empty` on every path exactly as an empty batch does,
> without sending any SQL and leaving the database untouched. A
> `Error::RecordNotInserted` from the underlying insert is mapped to
> `TryInsertResult::Conflicted`; success wraps the result in
> `TryInsertResult::Inserted`.

> [spec:pgorm:sem:query.build.update+3]
> `Update::one(model)` builds an `UpdateOne<A>` in two passes over the
> ActiveModel and returns `Result<UpdateOne<A>, Error>`. Filters: every
> primary-key column contributes a `WHERE pk = value` equality from a `Set`
> or `Unchanged` value; a `NotSet` primary key aborts the build with
> `Err(Error::PrimaryKeyNotSet)` rather than panicking, so an `UpdateOne`
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

> [spec:pgorm:sem:query.build.delete+2]
> `Delete::one(model)` converts through `IntoActiveModel`, targets the
> entity's table and returns `Result<DeleteOne<A>, Error>`: every primary-key
> column contributes a `WHERE pk = value` equality from a `Set` or
> `Unchanged` value; a `NotSet` primary key aborts the build with
> `Err(Error::PrimaryKeyNotSet)` rather than panicking, so a `DeleteOne` can
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

> [spec:pgorm:req:query.loader+1]
> The loader layer MUST provide `LoaderTrait`, implemented for `Vec<M>` and
> `&[M]` (the `Vec` impl delegating to the slice impl), with three batched
> eager-loading operations: `load_one` for has-one relations
> (`Vec<Option<R::Model>>`), `load_many` for has-many relations
> (`Vec<Vec<R::Model>>`) and `load_many_via` for junction-mediated
> relations (`Vec<Vec<R::Model>>`). Each accepts either a bare entity or a
> pre-filtered `Select<R>` through `EntityOrSelect` (a bare entity becomes
> `E::find()`), and each returns results positionally aligned with the input
> slice.
>
> No loader operation takes the junction entity: it is the one
> `Related::via()` already names, so `load_many_via` MUST NOT ask the caller
> for it and there is no mismatched-junction error to raise.
>
> Relation shape is validated before querying: `load_one` errors if the
> relation has a `via` junction or is `HasMany`; `load_many` errors if it has
> a `via` junction or is `HasOne`; `load_many_via` errors if there is no
> `via` junction or if the target relation is not `HasOne`. An empty input
> slice short-circuits to an empty result without querying.

> [spec:pgorm:sem:query.loader.batching+3]
> Keys are collected in input order: for each input model, `extract_key`
> builds a `ValueTuple` from the from side of the relation's `columns`,
> projected as an `Identity` (unary,
> binary, ternary or many), resolving each column name back to the entity's
> `Column` enum via `FromStr`. A name that does not map is a caller-authored
> relation naming a column its model does not have, so `extract_key` MUST
> return `Err(Error::Query)` naming the unresolved column and the model's
> table rather than panicking, and the load aborts with that error. The
> batch filter built by `prepare_condition` is a single IN predicate against
> the to side of the relation's `columns` on `to_tbl`: a unary key becomes
> `col IN (v1, v2, ...)` over the flattened values; composite keys become a
> tuple expression `(a, b, ...) IN ((..), (..))` via `in_tuples`;
> `prepare_condition` is likewise fallible, propagating the qualification
> error of [spec:pgorm:req:query.loader.table-ref-limitation+3].
>
> Keys are not deduplicated: duplicate key values across input models are
> repeated verbatim in the IN list (the dedup is an acknowledged TODO in
> `prepare_condition`; current behaviour sends the duplicates). The condition
> is AND-ed onto the caller-supplied `Select` via `QueryFilter::filter`, so
> user filters and the key predicate compose.

> [spec:pgorm:sem:query.loader.regroup+3]
> Results are regrouped to input order by hashing on the to-side key extracted
> from each returned row. `load_one` builds a `HashMap<ValueTuple, Model>`
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
> report that as `Err(Error::Query)` rather than panicking, and the message
> MUST carry the unmatched key and a sample input key in `Debug` form (so both
> value types are named) together with both sides' column lists, making the
> asymmetry diagnosable from the error alone.

> [spec:pgorm:sem:query.loader.many-to-many+2]
> `load_many_via` issues one query. The caller's target selector is inner
> joined backwards through the target relation to the junction and through the
> via relation to the input entity's own table, which is joined under an
> internal alias so that a self-referencing many-to-many does not name one
> table twice and so that the key predicate — the via relation's from side
> against the input keys — qualifies against that alias. The input entity's
> columns are appended to the projection as the `B_` side
> (`[spec:pgorm:sem:query.build.combine+2]`), so each returned row carries its
> target model and the input model it belongs to.
>
> The junction's own columns are never decoded: reading a junction row would
> mean decoding a column whose Rust type the loader cannot name, whereas both
> entities' models decode through the path every other read takes. The price
> is that the input entity's row is transmitted once per matching target row.
>
> Rows are regrouped by the key extracted from the returned input model, into
> buckets seeded empty per input key; the result clones the bucket per input
> key, so inputs sharing a key each receive the same list and unmatched inputs
> receive an empty `Vec`. A target the caller's `Select` filtered away is
> absent from the join and so is dropped from the list, and a shared target is
> cloned into every referencing input. A returned key absent from the seeded
> buckets is reported as `Err(Error::Query)` on the same terms as
> `[spec:pgorm:sem:query.loader.regroup+3]`.
>
> Because the targets are read by one query rather than reassembled from a
> key map, an `order_by` on the caller's `Select` orders every bucket. Without
> one the order within a bucket is the join's, hence unspecified.

> [spec:pgorm:req:query.loader.table-ref-limitation+3]
> Loader key predicates can only qualify columns for an unaliased
> `FromItem::Table` relation target: `table_column` matches exactly that
> shape, over either `TableName` form, and MUST return `Err(Error::Query)`
> for every other from item (an aliased table, a subquery, a values list or
> a function call), naming the key column it could not qualify and the
> offending from item. `prepare_condition` propagates that error, so the load
> aborts with an `Err` and not a panic. Entities whose relations resolve to
> such from items still cannot be loaded through `LoaderTrait`; the
> limitation is unchanged, only its reporting.
