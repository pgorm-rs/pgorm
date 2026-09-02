# SQL AST (pgorm-query statement and expression tree)

pgorm-query models SQL statements as plain Rust data structures — an abstract
syntax tree built through fluent, mutating builder methods — which are rendered
to PostgreSQL text by the backend `QueryBuilder` only when a build method is
called. This document specifies the AST layer: the statement types under
`pgorm-query/src/query/`, the expression tree in `pgorm-query/src/expr.rs`, and
function calls in `pgorm-query/src/func.rs`. Rules capture what the code does
today, including panicking edges and deliberate failsafes.

## Overview

> [spec:pgorm:req:sql.ast]
> pgorm-query MUST provide a programmatic AST for building SQL statements,
> comprising `SelectStatement`, `InsertStatement`, `UpdateStatement`,
> `DeleteStatement`, and `WithQuery`, plus the `Query` shorthand whose
> associated functions (`Query::select()`, `Query::insert()`, `Query::update()`,
> `Query::delete()`, `Query::with()`, `Query::returning()`) construct fresh
> builders. Builder methods MUST mutate in place and return `&mut Self` so calls
> chain; constructing or mutating a statement MUST NOT touch a database.
>
> Any of the five statement kinds MUST be embeddable as a subquery via
> `into_sub_query_statement`, which wraps it in the `SubQueryStatement` enum
> (`SelectStatement`, `InsertStatement`, `UpdateStatement`, `DeleteStatement`,
> `WithStatement`). `SelectStatement` and `WindowStatement` additionally provide
> `take()`, which moves the accumulated contents out and leaves the builder in
> its default (empty) state.

> [spec:pgorm:req:sql.ast.build]
> Every statement type implements `QueryStatementBuilder` and
> `QueryStatementWriter`. `build` MUST return the pair `(String, Values)` where
> ordinary `SimpleExpr::Value` operands are replaced by numbered PostgreSQL
> placeholders (`$1`, `$2`, ...) — `QueryBuilder::placeholder()` returns
> `("$", true)` — and the corresponding values are collected in order into
> `Values`. `to_string` MUST render the same statement with all values inlined
> as SQL literals instead of placeholders.
>
> `SimpleExpr::Constant` operands are always written inline as literals
> (`prepare_constant`), never parameterised, even under `build`; this is how
> internally generated constants (for example the `ESCAPE` character and the
> empty-condition `TRUE`/`FALSE`) stay out of the parameter list. Identifiers
> are double-quoted in the rendered SQL.

## SELECT statements

> [spec:pgorm:def:sql.ast.select]
> `SelectStatement` is the SELECT AST node. It accumulates: an optional
> `SelectDistinct` (`All`, `Distinct`, `DistinctRow`, `DistinctOn(Vec<ColumnRef>)`),
> a list of `SelectExpr` projections (each an expression with optional alias and
> optional window), `from` table references, `JoinExpr` joins, a WHERE
> `ConditionHolder`, GROUP BY expressions, a HAVING `ConditionHolder`, a list of
> `(UnionType, SelectStatement)` unions, ORDER BY expressions, optional LIMIT
> and OFFSET values (set from `u64` via `limit`/`offset`, cleared via
> `reset_limit`/`reset_offset`), an optional `LockClause`, and at most one named
> WINDOW definition.
>
> A `LockClause` pairs a `LockType` (`Update`, `NoKeyUpdate`, `Share`,
> `KeyShare`) with an optional table list and optional `LockBehavior` (`Nowait`,
> `SkipLocked`); `lock`, `lock_with_tables`, `lock_with_behavior`,
> `lock_with_tables_behavior`, `lock_shared` (FOR SHARE) and `lock_exclusive`
> (FOR UPDATE) each overwrite the whole clause, so the last call wins.
> Structural-control helpers `conditions(bool, then, else)`, `apply_if(Option, f)`
> and `apply(f)` let callers branch while chaining.

> [spec:pgorm:req:sql.ast.select.projection]
> Projections MUST accumulate in call order: `expr`/`exprs` push anything
> convertible to `SelectExpr`, `column`/`columns` push `SimpleExpr::Column`
> projections from any `IntoColumnRef` (bare column, `(table, column)`, or
> `(schema, table, column)` tuples), and `expr_as` attaches an `AS` alias.
> `clear_selects` MUST empty the projection list.
>
> `distinct()` MUST set `SelectDistinct::Distinct`. `distinct_on(cols)` MUST set
> `SelectDistinct::DistinctOn` when the column collection is non-empty, and MUST
> clear the distinct flag entirely (render no DISTINCT at all) when the
> collection is empty.
>
> GROUP BY expressions accumulate via `group_by_columns`, `group_by_col`, and
> `add_group_by`. HAVING accepts conditions through `cond_having` (any
> `IntoCondition`) and `and_having` (a `SimpleExpr` shorthand delegating to
> `cond_having`); both feed the HAVING `ConditionHolder` with the semantics of
> `sql.ast.condition.holder`.

> [spec:pgorm:req:sql.ast.select.from]
> FROM clauses MUST accumulate: calling `from` repeatedly produces multiple
> comma-separated FROM items (the "old-school join" form), and `from_clear`
> MUST remove all of them. The FROM item variants are: plain tables (with
> optional schema and database qualification via tuples), `from_as` (aliased
> table), `from_subquery` (`TableRef::SubQuery` with mandatory alias),
> `from_function` (`TableRef::FunctionCall` with alias), and `from_values`
> (`TableRef::ValuesList` rendering `(VALUES (..), (..)) AS "alias"`).
>
> `from_values` MUST panic when given an empty tuple list (`assert!` on the
> collected rows); there is no non-panicking variant.

> [spec:pgorm:req:sql.ast.select.join]
> Joins MUST accumulate as `JoinExpr { join, table, on, lateral }` entries. The
> generic `join(JoinType, table, condition)` accepts `JoinType::Join`,
> `CrossJoin`, `InnerJoin`, `LeftJoin`, `RightJoin`, or `FullOuterJoin`, with
> named shorthands `cross_join`, `left_join`, `right_join`, `inner_join`, and
> `full_outer_join`. `join_as` MUST alias the joined table; `join_subquery` MUST
> join a `SelectStatement` as an aliased subquery; `join_lateral` MUST do the
> same with the `lateral` flag set, rendering `JOIN LATERAL`.
>
> The ON condition is any `IntoCondition` (a bare `SimpleExpr` or a
> `Condition` tree) and MUST be stored as `JoinOn::Condition` wrapping a
> `ConditionHolder`, so multi-part conditions built with `Condition::all`/`any`
> render as chained `AND`/`OR` in the ON clause.

> [spec:pgorm:sem:sql.ast.select.union]
> `union(UnionType, query)` appends one compound-query arm and `unions(iter)`
> extends with many; arms accumulate in call order and are never merged or
> deduplicated. `UnionType::All` renders `UNION ALL`, `UnionType::Distinct`
> renders plain `UNION`, and `Intersect`/`Except` render the corresponding set
> operators; each appended arm is rendered as a parenthesised SELECT after the
> operator. The AST does not verify that the arms project the same columns —
> that is left to PostgreSQL.

## Ordering

> [spec:pgorm:req:sql.ast.order]
> `SelectStatement`, `UpdateStatement`, `DeleteStatement`, and `WindowStatement`
> share the `OrderedStatement` trait. Order expressions MUST accumulate in call
> order via `order_by` (column + `Order`), `order_by_expr`, `order_by_customs`
> (raw string rendered verbatim as `SimpleExpr::Custom`), `order_by_columns`,
> and the `*_with_nulls` variants which attach a `NullOrdering` (`First`/`Last`)
> rendered as `NULLS FIRST`/`NULLS LAST`. `clear_order_by` MUST remove all
> accumulated order expressions.
>
> `Order` MUST support `Asc`, `Desc`, and `Field(Values)`; the `Field` variant
> renders a `CASE WHEN col=v_i THEN i ... ELSE n END` expression implementing
> explicit custom value ordering.

## Conditions

> [spec:pgorm:def:sql.ast.condition]
> `Condition` (aliased as `Cond`) is a tree node holding a `condition_type`
> (`ConditionType::All` = conjunction, `ConditionType::Any` = disjunction), a
> `negate` flag, and child `ConditionExpression`s, where each child is either a
> nested `Condition` or a leaf `SimpleExpr`. `Condition::all()` and
> `Condition::any()` construct empty sets; `add` pushes a child; `add_option`
> pushes only when `Some`; `not()` toggles the negate flag; `is_empty`/`len`
> inspect the children. The `all![...]` and `any![...]` macros are shorthand
> for building the corresponding set from a list of expressions.
>
> The `IntoCondition` trait converts arguments at API boundaries: a
> `SimpleExpr` becomes `Condition::all().add(expr)` and a `Condition` passes
> through unchanged, which is why `and_where`-style helpers and `cond_where`
> accept both.

> [spec:pgorm:sem:sql.ast.condition.flattening]
> `Condition::add` flattens trivial nesting: when the added child is itself a
> `Condition` with exactly one member and no negation, the inner member is
> unwrapped and pushed directly, skipping the useless junction. Nested
> conditions with two or more members, or with `negate` set, are kept intact
> and render inside parentheses.
>
> When a `Condition` is lowered to a `SimpleExpr` (`to_simple_expr`), members
> are folded left-to-right with `OR` for `Any` and `AND` for `All`. An empty
> `Any` lowers to the constant `FALSE` and an empty `All` to the constant
> `TRUE` (as inline `SimpleExpr::Constant`s), and a set with `negate` wraps the
> folded expression in `NOT (...)`.

> [spec:pgorm:req:sql.ast.condition.holder+1]
> WHERE and HAVING clauses are backed by `ConditionHolder`, whose contents are
> one of `Empty`, `Chain(Vec<LogicalChainOper>)`, or `Condition` (built by the
> `cond_where` style). `ConditionalStatement::and_where` and
> `and_where_option` delegate to `cond_where` and therefore build the
> `Condition` state; the only entry point into the `Chain` state is the
> `#[doc(hidden)]` `and_or_where(LogicalChainOper)`. The two states MUST NOT
> be mixed on the same holder: adding a chain operator to a holder in
> `Condition` state, or a condition to a holder in `Chain` state, panics with
> "Cannot mix `and_where`/`or_where` and `cond_where` in statements".
>
> Repeated `cond_where` calls MUST conjoin: if both the current and the added
> condition are non-negated `All` sets the additions are appended flat into the
> existing set; otherwise the current contents and the addition are combined
> under a fresh `Condition::all()`. Order of calls is preserved in the rendered
> output.

## Expressions

> [spec:pgorm:def:sql.ast.expr]
> `SimpleExpr` is the expression tree node, with variants `Column(ColumnRef)`,
> `Tuple`, `Unary(UnOper, ..)` (the only unary operator is `Not`),
> `FunctionCall`, `Binary(lhs, BinOper, rhs)`, `SubQuery(Option<SubQueryOper>, ..)`,
> `Value` (parameterised), `Values`, `Custom(String)` (verbatim SQL),
> `CustomWithExpr(String, Vec<SimpleExpr>)` (template with `$1`-style splices,
> `$$` escaping a literal `$`), `Keyword`, `AsEnum`, `Case`, and `Constant`
> (inlined literal). `Expr` is the entry-point builder holding a left operand
> plus pending unary/binary operator state; `Expr::col`, `Expr::val`,
> `Expr::expr`, `Expr::tuple`, `Expr::value`, `Expr::cust`,
> `Expr::cust_with_values`, `Expr::cust_with_expr`, and `Expr::cust_with_exprs`
> construct expressions from columns, values, other expressions, and raw SQL.
>
> Subquery expressions carry an optional `SubQueryOper`: `Expr::exists`,
> `Expr::any`, `Expr::some`, and `Expr::all` wrap a `SelectStatement` in
> `EXISTS(...)`, `ANY(...)`, `SOME(...)`, and `ALL(...)` respectively.
> `From` conversions lift `Value`-convertible Rust primitives, `FunctionCall`,
> `ColumnRef`, `Keyword`, `CaseStatement`, and finished `Expr` builders into
> `SimpleExpr`, which is what allows plain Rust values wherever
> `Into<SimpleExpr>` is accepted.

> [spec:pgorm:req:sql.ast.expr.operators]
> `Expr` and `SimpleExpr` MUST provide combinators that produce `Binary`/`Unary`
> nodes: comparisons `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, plus
> `equals`/`not_equals` for column-to-column comparison; arithmetic `add`,
> `sub`, `mul`, `div`, `modulo`; bit shifts `left_shift`, `right_shift`;
> `between`/`not_between`; `is_null`, `is_not_null`, `is`, `is_not`; logical
> `and`, `or`, and `not` (prefix `NOT`); string/pattern operators `like`,
> `not_like`, `ilike`, `not_ilike` — a `LikeExpr` with an escape character MUST
> render an `ESCAPE` clause whose character is an inline constant — and
> `concat` (`||`).
>
> PostgreSQL-specific operators MUST be available: full-text `matches` (`@@`),
> containment `contains` (`@>`) and `contained` (`<@`), JSON access
> `get_json_field` (`->`) and `cast_json_field` (`->>`). The escape hatch
> `binary(op, rhs)` accepts any `BinOper`, whose variants further include
> regex (`~`, `~*`), trigram similarity and distance operators, pgvector
> distance operators, `Overlap`, and `Custom(&'static str)` for arbitrary
> operator text. Casts are expressed with `cast_as` (`CAST(expr AS type)`) and
> `as_enum`; aggregate shorthands `max`, `min`, `sum`, `count`,
> `count_distinct`, and `if_null` wrap the expression in the corresponding
> function call.

> [spec:pgorm:req:sql.ast.expr.in]
> `is_in`/`is_not_in` MUST build an `IN`/`NOT IN` over a `Tuple` of the given
> operands. When the operand collection is empty, rendering MUST fall back to
> the always-false comparison `'a' = 'b'` for both `IN` and `NOT IN` — a
> deliberate failsafe so an empty list can never produce invalid SQL such as
> `IN ()`. (Note the current limitation: an empty `NOT IN`, which is logically
> always-true, also renders the always-false form.)
>
> `in_tuples` MUST build multi-column membership tests
> (`(a, b) IN ((..), (..))`) from `IntoValueTuple` rows, and
> `in_subquery`/`not_in_subquery` MUST build `IN (SELECT ...)` /
> `NOT IN (SELECT ...)` from a `SelectStatement`.

> [spec:pgorm:def:sql.ast.keywords]
> `Keyword` represents bare SQL keywords usable as expressions: `Null`,
> `CurrentDate`, `CurrentTime`, `CurrentTimestamp`, and `Custom(DynIden)`;
> `Expr::current_date()`, `Expr::current_time()`, `Expr::current_timestamp()`,
> and `Expr::custom_keyword(..)` construct them. Identifier helpers: `Alias`
> wraps an arbitrary string as an identifier, `NullAlias` is an empty
> identifier, and `Asterisk` expresses `*` — as a bare projection or
> table-qualified via `(Table, Asterisk)` rendering `"table".*`. `ColumnRef`
> spans `Column`, `TableColumn`, `SchemaTableColumn`, `Asterisk`, and
> `TableAsterisk`; `TableRef` spans plain/schema/database-qualified tables,
> their aliased forms, `SubQuery`, `ValuesList`, and `FunctionCall`.

## INSERT statements

> [spec:pgorm:def:sql.ast.insert]
> `InsertStatement` is the INSERT AST node: a target table (`into_table`), a
> column list (`columns`, which replaces any previous list), a value source, an
> optional `OnConflict`, an optional `ReturningClause`, and an optional
> default-values row count. The value source (`InsertValueSource`) is either
> `Values(Vec<Vec<SimpleExpr>>)` — multi-row VALUES accumulated one row per
> `values`/`values_panic` call — or `Select(..)` set by `select_from`, which
> makes the insert read from a query (`INSERT INTO .. SELECT ..`); setting a
> select source replaces any previously accumulated rows.
>
> `or_default_values()` / `or_default_values_many(n)` record a fallback used
> only when no columns and no values were supplied, rendering
> `VALUES (DEFAULT)` repeated `n` times; when columns and values are present
> the fallback is ignored.

> [spec:pgorm:req:sql.ast.insert.arity]
> `values(row)` MUST verify that the row length equals the declared column
> count and return `Err(Error::ColValNumMismatch { col_len, val_len })` on
> mismatch, appending the row only on success. `select_from` MUST apply the
> same check between the column count and the select's projection count.
> `values_panic` and `values_from_panic` are the unwrapping variants and MUST
> panic on the same mismatch.
>
> An empty row passes the check only when zero columns are declared, and is
> then silently discarded (no row appended) — so feeding zero rows leaves the
> statement without a values source. This is the AST half of pgorm's failsafe
> behavior for empty `insert_many` operations.

## ON CONFLICT

> [spec:pgorm:req:sql.ast.on-conflict]
> `OnConflict` (attached with `InsertStatement::on_conflict`) MUST hold: target
> specifications — `OnConflictTarget::ConflictColumn` entries via
> `OnConflict::column`/`columns` or `ConflictExpr` entries via `expr`/`exprs`
> (expressions append to any existing targets); an optional target WHERE
> (`target_and_where`, `target_and_where_option`, `target_cond_where`) rendered
> between the target list and the action; an action; and an optional action
> WHERE (`action_and_where`, `action_and_where_option`, `action_cond_where`)
> rendered after `DO UPDATE SET ...`.
>
> The action is either `DoNothing` — set by `do_nothing()` or
> `do_nothing_on(pk_cols)`, both rendering `DO NOTHING` on PostgreSQL (the
> column list is a polyfill inherited from upstream and is not rendered) — or
> `Update`, a list of `OnConflictUpdate` entries. `update_column`/`update_columns`
> MUST add `Column` entries (rendering `"col" = "excluded"."col"`), and
> `value`/`values` MUST add `Expr` entries (rendering `"col" = <expr>`).
> Update entries MUST append to an existing `Update` action and MUST replace a
> previously set `DoNothing` action.

## RETURNING

> [spec:pgorm:def:sql.ast.returning]
> `ReturningClause` expresses PostgreSQL's `RETURNING` and has three forms:
> `All` (`RETURNING *`), `Columns(Vec<ColumnRef>)`, and
> `Exprs(Vec<SimpleExpr>)`. The `Returning` helper (obtained from
> `Query::returning()`) constructs them via `all()`, `column(..)`,
> `columns(..)`, `expr(..)`, and `exprs(..)`. Insert, update, and delete
> statements accept a clause through `returning(..)`, with shorthands
> `returning_col(..)` and `returning_all()`; `SelectStatement` has no RETURNING
> support. Each call replaces any previously set clause.

## UPDATE and DELETE statements

> [spec:pgorm:req:sql.ast.update]
> `UpdateStatement` MUST accumulate SET assignments in call order as
> `(column, expression)` pairs: `values(pairs)` pushes many, `value(col, expr)`
> pushes one, and any `Into<SimpleExpr>` is accepted on the right-hand side
> (values, keywords, `Expr::cust` fragments, subqueries). Duplicate columns are
> not deduplicated — each call appends. The statement also carries the target
> `table`, a WHERE `ConditionHolder` (chain and condition styles per
> `sql.ast.condition.holder`), ORDER BY expressions, an optional LIMIT, and an
> optional `ReturningClause`. `get_values` MUST expose the accumulated
> assignment pairs for inspection.

> [spec:pgorm:def:sql.ast.delete]
> `DeleteStatement` is the DELETE AST node: a target table set by
> `from_table`, a WHERE `ConditionHolder` shared with the condition rules, ORDER
> BY expressions, an optional LIMIT, and an optional `ReturningClause`. Like
> the other write statements it can be prefixed with a WITH clause via
> `with(..)`, producing a `WithQuery`.

## WITH clauses and CTEs

> [spec:pgorm:def:sql.ast.with]
> `CommonTableExpression` defines one named query in a WITH clause: a mandatory
> `table_name`, optional column list (`column`/`columns`), a mandatory `query`
> (any `QueryStatementBuilder`, stored as a `SubQueryStatement` — the AST does
> not restrict UPDATE/DELETE CTEs; validity is left to PostgreSQL), and an
> optional `materialized` flag rendering `MATERIALIZED` / `NOT MATERIALIZED`.
> `CommonTableExpression::from_select` derives a CTE from a `SelectStatement`,
> naming it `cte_<table>` after the first FROM table and deriving column names
> from aliases or plain column projections; `try_set_cols_from_select` performs
> only the column derivation and reports `false` (leaving columns untouched)
> when any projection is an expression or wildcard it cannot name.
>
> `WithClause` aggregates a `recursive` flag, an optional `Search`, an optional
> `Cycle`, and the CTE list (`cte`); `WithClause::query(stmt)` (or
> `stmt.with(clause)` on select/insert/update/delete) produces a `WithQuery`
> pairing the clause with the statement it prefixes. `WithQuery` also exposes
> the same setters directly (`recursive`, `search`, `cycle`, `cte`, `query`).

> [spec:pgorm:req:sql.ast.with.recursive]
> Rendering a `WithQuery` MUST panic (assert) if its clause contains no CTE,
> and MUST panic if `recursive` is set and the clause contains more than one
> CTE — a recursive WITH must consist of exactly one CTE containing a union
> query. When `recursive` is set the clause renders `WITH RECURSIVE` and the
> optional `SEARCH` and `CYCLE` clauses are emitted; they are ignored for
> non-recursive queries.
>
> `Search` pairs a `SearchOrder` (`BREADTH`/`DEPTH`) with a `SelectExpr` whose
> alias names the generated order column; `Search::new_from_order_and_expr` and
> `Search::expr` MUST panic (`unwrap` on the alias) when the given expression
> has no alias. `Cycle` requires the node-identifying expression (`expr`), the
> cycle-mark column name (`set`), and the path column name (`using`), rendering
> `CYCLE <expr> SET <set> USING <using>`.

## Window statements

> [spec:pgorm:def:sql.ast.window-statement]
> `WindowStatement` describes an OVER window: PARTITION BY expressions
> (`partition_by`, `partition_by_custom`, and the `OverStatement` trait's
> `partition_by_columns`/`partition_by_customs`), ORDER BY expressions (shared
> `OrderedStatement` trait), and an optional `FrameClause` — a `FrameType`
> (`Range` or `Rows`) with a start `Frame` and optional end `Frame`
> (`UnboundedPreceding`, `Preceding(n)`, `CurrentRow`, `Following(n)`,
> `UnboundedFollowing`), set via `frame_start` (single bound) or
> `frame_between` (`BETWEEN .. AND ..`).
>
> A select projection references a window in one of two ways
> (`WindowSelectType`): `Query` embeds the window inline (`expr_window`,
> `expr_window_as` render `OVER ( ... )`), while `Name` references a named
> window (`expr_window_name`, `expr_window_name_as` render `OVER "w"`) that is
> declared at statement level with `SelectStatement::window(name, window)`,
> rendering a `WINDOW "w" AS ...` clause. The statement holds at most one named
> window; a second `window()` call replaces the first.

## CASE expressions

> [spec:pgorm:def:sql.ast.case]
> `CaseStatement` builds a searched CASE expression: each `case(cond, then)`
> call appends a `WHEN <condition> THEN <result>` arm — the condition is any
> `IntoCondition`, so `Condition` trees render with their `AND`/`OR`/`NOT`
> structure inside the WHEN — and `finally(expr)` sets the optional `ELSE`
> result. `Expr::case(cond, then)` is the shorthand constructor for the first
> arm. A `CaseStatement` converts into `SimpleExpr::Case`, so a whole CASE
> expression can be projected (with `expr_as`), compared, or used anywhere an
> expression is accepted.

## Function calls

> [spec:pgorm:def:sql.ast.func]
> `FunctionCall` pairs a `Function` selector with argument expressions and
> per-argument modifiers (`FuncArgMod { distinct }`); `arg` appends one
> argument, `args` replaces the argument list. The `Function` enum covers the
> built-ins with typed constructors on the `Func` helper: aggregates `max`,
> `min`, `sum`, `avg`, `count`, `count_distinct` (the DISTINCT argument
> modifier), `bit_and`, `bit_or`; scalar helpers `abs`, `char_length`,
> `if_null`, `coalesce`, `lower`, `upper`, `round`, `round_with_precision`,
> `random`, `starts_with`, `gen_random_uuid`, `cast_as`; the PostgreSQL
> full-text family `to_tsquery`, `to_tsvector`, `phraseto_tsquery`,
> `plainto_tsquery`, `websearch_to_tsquery` (each with an optional `regconfig`
> OID prepended as first argument), `ts_rank`, `ts_rank_cd`; and array/subquery
> comparators `any`, `some`, `all`.
>
> `Func::cust(iden)` calls an arbitrary function by identifier
> (`Function::Custom`). A `FunctionCall` converts into
> `SimpleExpr::FunctionCall`, and can serve as a FROM item through
> `SelectStatement::from_function`.
