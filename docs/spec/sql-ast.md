# SQL AST (pgorm-query statement and expression tree)

pgorm-query models SQL statements as plain Rust data structures — an abstract
syntax tree built through fluent, mutating builder methods — which are rendered
to PostgreSQL text by the backend `QueryBuilder` only when a build method is
called. This document specifies the AST layer: the statement types under
`pgorm-query/src/query/`, the expression tree in `pgorm-query/src/expr.rs`, and
function calls in `pgorm-query/src/func.rs`. Rules capture what the code does
today, including panicking edges and deliberate failsafes.

## Overview

> [spec:pgorm:req:sql.ast+1]
> pgorm-query MUST provide a programmatic AST for building SQL statements,
> comprising `SelectStatement`, `InsertStatement`, `UpdateStatement` and
> `DeleteStatement`, plus the `Query` shorthand whose
> associated functions (`Query::select()`, `Query::insert()`, `Query::update()`,
> `Query::delete()`, `Query::with()`, `Query::returning()`) construct fresh
> builders. Builder methods MUST mutate in place and return `&mut Self` so calls
> chain; constructing or mutating a statement MUST NOT touch a database.
>
> Any of the four statement kinds MUST be embeddable as a subquery via
> `into_sub_query_statement`, which wraps it in the `SubQueryStatement` enum
> (`SelectStatement`, `InsertStatement`, `UpdateStatement`, `DeleteStatement`).
> There is no fifth variant for a WITH-prefixed statement: a clause is carried
> by the statement it prefixes (`query.build.with`), so the prefix nests
> wherever the statement does.
>
> Where a builder offers `take()`, the method MUST move the accumulated contents
> out and leave the source in its default (empty) state — it may copy only the
> identity field a constructor demanded, never content a caller added. A type
> whose contents cannot be moved out without leaving an invalid value therefore
> has no `take()` at all rather than a `take()` that copies: `SelectStatement`,
> `WindowStatement`, `TableIndex`, `ColumnDef`, `TableCreateStatement` and
> `TableDropStatement` have one; `IndexCreateStatement`,
> `ForeignKeyCreateStatement`, `TableForeignKey` and `TableAlterStatement` do
> not, and a caller who wants a second copy of one writes `.to_owned()`.

> [spec:pgorm:req:sql.surface+4]
> The crate's exports are an explicit list, not a set of module globs.
> `pgorm-query/src/lib.rs` MUST name every exported item in `pub use` statements
> grouped by what the items are for — names, expressions, values, query
> statements, schema statements, rendering — and the modules those items are
> defined in MUST be private, so `use pgorm_query::*` and that list are the same
> set and nothing becomes API by being declared `pub` inside a module a caller
> can reach. Exactly three modules stay public, each because a path through it
> is the spelling callers write: `error` (`Error`, `Result`, `TemplateError`),
> `extension` (the `CREATE EXTENSION` / `CREATE TYPE` surface, deliberately not
> flattened into the root) and `value` (whose `value::with_array::NotU8` pgorm's
> derives name in generated code). `tests_cfg` is `#[doc(hidden)]` behind its
> own feature and is not surface.
>
> What the crate reads but a caller cannot use is `pub(crate)`: the renderer's
> `Mode` and `Oper` helpers, the clause shapes a statement holds internally
> (`JoinExpr`, `JoinKind`, `JoinOn`, `LockClause`, `ConditionHolder`), and
> `prepare`'s re-export of `std::fmt::Write`, which put a `std` trait in
> pgorm-query's namespace and was the only path by which `pgorm_query::Write`
> could be written. An item that narrowing would merely turn into dead code is
> instead exported `#[doc(hidden)]`, so the rendered documentation is the
> curated list while the item stays reachable: the statement-dispatch wrappers
> `QueryStatement`, `TableStatement`, `IndexStatement`, `ForeignKeyStatement`
> and `SchemaStatement`, which no builder produces and no renderer takes; the
> `SelectDistinct` flag, whose `All` variant no builder sets; and the
> `Token`/`Tokenizer` lexer behind `inject_parameters`, whose classification
> accessors only the conformance suite calls. Each of those MUST be named
> individually in `lib.rs` rather than left to a glob, so the residue is a list
> a later pass can work through rather than a category.
>
> The list grows only by a named item that a rule specifies, and each addition
> is recorded here with that rule: `CaseOperand` and `SimpleCaseStatement`,
> the simple form of `CASE` (`sql.ast.case`); `Subscript`, what an array
> subscript holds between its brackets (`sql.ast.expr.subscript`);
> `GroupingElement`, `GroupingSets` and `Grouping`, the GROUP BY items beyond
> a plain expression and the `GROUPING()` that reads them
> (`sql.ast.select.grouping`); `FrameStart`, its three side markers
> `FramePreceding`, `FrameCurrentRow` and `FrameFollowing`, and
> `FrameExclusion`, the frame builder and its `EXCLUDE` clause, which replace
> the bound enum `Frame` (`sql.ast.window-statement`).

> [spec:pgorm:req:sql.ast.build+3]
> Every statement type implements the single `QueryStatementBuilder`, whose
> `Display` supertrait carries the value-inlined rendering. There is exactly
> one method per rendering, and no rendering method takes a builder argument:
> `QueryBuilder` is a stateless unit struct, so a passed one carried no
> information, and the `*_any` family that existed only to take it by reference
> — `build_any`, `build_collect_any`, `build_collect_any_into` — is gone along
> with the `QueryStatementWriter` half of the split.
>
> `build()` MUST return the pair `(String, Values)` where
> ordinary `SimpleExpr::Value` operands are replaced by numbered PostgreSQL
> placeholders (`$1`, `$2`, ...) per `[spec:pgorm:req:sql.render.placeholders+1]`
> — and the corresponding values are collected in order into
> `Values`. `to_string()`, reached through `Display`, MUST render the same
> statement with all values inlined as SQL literals instead of placeholders.
> `build_collect_into(sink)` is the single required method both are written in
> terms of, and `build_collect(sink)` returns the sink's accumulated text.
>
> `SimpleExpr::Constant` operands are always written inline as literals
> (`prepare_constant`), never parameterised, even under `build`; this is how
> internally generated constants (for example the `ESCAPE` character and the
> empty-condition `TRUE`/`FALSE`) stay out of the parameter list. Identifiers
> are double-quoted in the rendered SQL.
>
> The two renderings are not interchangeable and the difference MUST be
> documented where a reader meets it: each query statement's `Display` impl
> carries the note that it inlines rather than binds, and points at `build`.
> The inlined form escapes what it writes, so the warning is not about
> injection; it is that the server re-parses the literal and the type pinning
> a bound value carries (`[spec:pgorm:req:sql.render.cast-param-type+3]`) is
> lost.

## Scope

> [spec:pgorm:req:sql.scope+4]
> pgorm-query models the PostgreSQL a data-access layer writes, not the whole
> of PostgreSQL, and the boundary MUST be written down rather than discovered.
> A construct outside the builder is still reachable — `Expr::raw` and
> `SqlTemplate` in expression position, `FromItem`'s raw relation in relation
> position, and `ConnectionTrait::execute` / `batch_execute` for a whole
> statement — so what follows records a decision about the *typed* surface,
> never about what a caller can send. Each entry MUST carry a verdict and the
> reason for it; "deferred" MUST additionally say what closing it would take,
> because a deferral whose cost is unrecorded is indistinguishable from an
> oversight. An entry graduates by being implemented and deleted from this
> list, and a construct absent from both this list and the builder is a defect
> in this rule, not a silent no.
>
> Three tests decide which verdict an entry gets. *Does the ORM need it to do
> its job?* — the builder exists to render the statements an entity layer
> issues, so a construct no ORM path emits is out of scope however ordinary it
> is in SQL. *Does the typed form add anything over the raw one?* — a
> construct with no options, no identifiers to quote and no values to bind is
> a string, and wrapping a string in a builder buys nothing. *Is it a
> statement or a vocabulary?* — a whole statement kind costs a new AST node, a
> new renderer and a new `SubQueryStatement` arm; a clause on an existing
> statement costs a field.
>
> **Deferred** — worth building, not built:
>
> - **`MERGE`.** A fifth statement kind: its own AST node, renderer, and
>   `WHEN MATCHED` / `WHEN NOT MATCHED [BY SOURCE]` action list, plus a source
>   relation that is already `FromItem`. `ON CONFLICT` covers the upsert that
>   an ORM actually emits, which is why this ranks below its size.
> - **Range and multirange types.** Absent end to end: no `Value` variant, no
>   `ColumnType` variant, no `CREATE TYPE ... AS RANGE`. A range is a value
>   with a discriminated subtype and two bound inclusivities, so it needs a
>   `Value` variant that round-trips through tokio-postgres before any of the
>   rest is useful; the operators it would be read with (`@>`, `<@`, `&&`)
>   already exist. This is the largest deferred entry and the one that would
>   most change `sql.value`.
> - **`ON CONFLICT ON CONSTRAINT <name>`.** The arbiter today is index
>   inference — a column or expression list with an optional `WHERE`
>   (`sql.ast.on-conflict`) — and a named constraint takes neither. Closing it
>   means splitting the arbiter into inference-or-constraint and giving the
>   constraint form its own small typestate, so `and_column` and `and_where`
>   stay unreachable from it; a variant inside `ConflictElement` would make
>   both invalid states constructible.
> - **`COLLATE`.** Postfix, and its right operand is a collation *name*, not an
>   expression, so it is a dedicated `SimpleExpr` variant holding a `Name` —
>   the shape `AsEnum` uses — rather than a `BinOper`, which would admit
>   `a COLLATE b` for arbitrary `b`. Wanted in three positions (expression,
>   `ORDER BY`, column definition), and `pgorm-codegen` already refuses to read
>   a column carrying one, so closing this is two changes in two crates.
> - **Deferrability on constraints other than foreign keys.** `UNIQUE`,
>   `PRIMARY KEY` and `CHECK` take the same clause, and reach the renderer
>   through `ColumnSpec` and the index statements rather than through
>   `TableForeignKey`. The foreign-key case is the one with a use an ORM meets
>   — mutually referencing rows — which is why it is built and these are not.
> - **`CREATE TYPE ... AS (composite)`.** Mechanical: a list of
>   `(Name, ColumnType)` pairs and a render arm reusing the column-type
>   renderer, beside the `AS ENUM` form that already exists. It waits on a
>   consumer: nothing in the ORM decodes a composite value, so the DDL would
>   create a type no entity could name.
> - **Sequences.** `CREATE`/`ALTER`/`DROP SEQUENCE`, and the `START WITH` /
>   `INCREMENT BY` / `CACHE` tail that `sql.ddl.column-def` currently routes
>   through `raw_suffix`. One typed sequence builder closes both, and that
>   rule already names this as where those options should land.
>
> **Out of scope** — not the builder's job:
>
> - **Partitioning DDL** (`PARTITION BY`, `PARTITION OF`, `ATTACH`/`DETACH`).
>   A physical-layout decision made once per table by whoever owns the schema,
>   with a large option surface and no values to bind. Migrations write it as
>   raw SQL.
> - **`EXCLUDE` constraints.** An operator-class-per-column constraint whose
>   typed form would be most of an index builder again, for a constraint an
>   entity layer never generates.
> - **Views and materialized views.** A view is a stored query, so `CREATE
>   VIEW` is one keyword wrapped around a `SelectStatement` a caller already
>   has; the refresh and storage options on matviews are schema-ownership
>   decisions like partitioning. What pgorm needs is to *read* views, which it
>   does — an entity maps to a view as readily as to a table.
> - **Table and column storage options** (`WITH (fillfactor = …)`,
>   `TABLESPACE`, `SET STORAGE`). Physical tuning with no effect on any
>   statement the ORM issues, and no identifier or value that needs quoting.
> - **`GRANT` / `REVOKE`, and roles.** Privileges are an operational concern,
>   granted by whoever administers the database rather than by the process
>   reading from it; a library that can widen its own privileges is a hazard
>   and not a feature.
> - **Triggers and `CREATE FUNCTION`.** Both carry a body in another language
>   (`plpgsql`, `sql`, C), which no expression AST can model — the body is an
>   opaque string however it is delivered, so a builder around it is quoting
>   with extra steps.
> - **Row-level security policies.** Same shape as privileges, and a policy
>   that the application can rewrite is not a security boundary.
> - **`COPY`.** Protocol-level: `COPY ... FROM STDIN` is a distinct message
>   flow, not a statement a renderer emits, so it belongs to the driver
>   surface rather than to the query builder.
> - **Procedural `DO` blocks.** A string of another language, as triggers are.
> - **`CREATE DATABASE` / `CREATE SCHEMA` / `CREATE EXTENSION`'s neighbours.**
>   Cluster-level administration, run once by an operator and outside any
>   transaction. `CREATE EXTENSION` itself is the exception that proves the
>   rule and is built (`sql.ddl.extension`), because an extension is what makes
>   a *column type* available and so is reachable from an entity.

## SELECT statements

> [spec:pgorm:def:sql.ast.select+3]
> `SelectStatement` is the SELECT AST node. It accumulates: an optional carried
> WITH clause (`Option<Box<AnyWithClause>>`, boxed so a clause-less statement
> pays one pointer — `query.build.with` records why the clause lives on the
> statement rather than in a wrapper), an optional
> `SelectDistinct` (`All`, `Distinct`, `DistinctOn(Vec<ColumnRef>)` — the
> MySQL-era `DistinctRow`, which no builder set and no renderer spelled, is
> gone and MUST NOT return),
> a list of `SelectExpr` projections (each an expression with optional alias and
> optional window), `from` table references, `JoinExpr` joins, a WHERE
> `ConditionHolder`, a GROUP BY list of `GroupingElement`s
> (`sql.ast.select.grouping`), a HAVING `ConditionHolder`, a list of
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

> [spec:pgorm:req:sql.ast.select.projection+2]
> Projections MUST accumulate in call order: `expr`/`exprs` push anything
> convertible to `SelectExpr`, `column`/`columns` push `SimpleExpr::Column`
> projections from any `IntoColumnRef` (bare column, `(table, column)`, or
> `(schema, table, column)` tuples), and `expr_as` attaches an `AS` alias.
> `clear_selects` MUST empty the projection list, and `selects()` MUST expose
> the accumulated list for inspection — the read side callers need to tell an
> empty projection from a populated one without rendering the statement.
>
> `distinct()` MUST set `SelectDistinct::Distinct`. `distinct_on(cols)` MUST set
> `SelectDistinct::DistinctOn` when the column collection is non-empty, and MUST
> clear the distinct flag entirely (render no DISTINCT at all) when the
> collection is empty.
>
> GROUP BY items accumulate in call order via `group_by_columns`,
> `group_by_col` and `add_group_by`, each adding plain expressions, and
> `group_by_element`, which adds a grouping element of `sql.ast.select.grouping`
> to the same list. HAVING accepts conditions through `cond_having` (any
> `IntoCondition`) and `and_having` (a `SimpleExpr` shorthand delegating to
> `cond_having`); both feed the HAVING `ConditionHolder` with the semantics of
> `sql.ast.condition.holder`.

> [spec:pgorm:req:sql.ast.select.from+2]
> FROM clauses MUST accumulate: calling `from` repeatedly produces multiple
> comma-separated FROM items (the "old-school join" form), and `from_clear`
> MUST remove all of them. The FROM item variants are: plain tables (with
> optional schema qualification via a 2-tuple), `from_as` (aliased
> table), `from_subquery` (`FromItem::SubQuery` with mandatory alias),
> `from_function` (`FromItem::FunctionCall` with alias), and `from_values`
> (`FromItem::ValuesList` rendering `(VALUES (..), (..)) AS "alias"`).
>
> `FromItem::Template` (`[spec:pgorm:def:sql.types.table-ref+4]`) gets no
> shorthand of its own. Its constructor returns `Result`, and a shorthand
> would have to either return `Result` — breaking the `&mut Self` chain every
> other builder method keeps — or swallow the failure; instead it is built
> first and handed to the generic `from`, which already takes any
> `IntoFromItem`. A builder method is not owed to every variant; it is owed
> where it saves the caller a type name and nothing else.
>
> `from_values` MUST panic when given an empty tuple list (`assert!` on the
> collected rows); there is no non-panicking variant.

> [spec:pgorm:req:sql.ast.select.join+1]
> Joins MUST accumulate as `JoinExpr { join, table, lateral }` entries, where
> `join` is a `JoinKind` that carries the constraint with the join it belongs
> to: `Qualified(JoinType, JoinOn)` for a join with an `ON` clause, and `Cross`
> for the one join PostgreSQL takes without one. `JoinType` therefore names
> only the constrained joins — `Join`, `InnerJoin`, `LeftJoin`, `RightJoin`,
> `FullOuterJoin` — and the generic `join(JoinType, table, condition)` cannot
> spell a cross join, nor `cross_join(table)` a condition
> (`[dec:pgorm:invalid-states-unrepresentable]`). Named shorthands are
> `cross_join`, `left_join`, `right_join`, `inner_join`, and
> `full_outer_join`. `join_as` MUST alias the joined table; `join_subquery` MUST
> join a `SelectStatement` as an aliased subquery; `join_lateral` MUST do the
> same with the `lateral` flag set, rendering `JOIN LATERAL`.
>
> The ON condition is any `IntoCondition` (a bare `SimpleExpr` or a
> `Condition` tree) and MUST be stored as `JoinOn::Condition` wrapping a
> `ConditionHolder`, so multi-part conditions built with `Condition::all`/`any`
> render as chained `AND`/`OR` in the ON clause.

> [spec:pgorm:def:sql.ast.select.grouping]
> A SELECT's GROUP BY list is a list of `GroupingElement`s — PostgreSQL's
> `grouping_element` — and each element stands for a list of grouping sets;
> the query groups by every combination of its items' sets, one set from
> each. The element's shape is private, so it is built only through its
> constructors:
>
> - `GroupingElement::set(exprs)` — one set of the given expressions, and
>   `GroupingElement::empty()`, the set of none, which groups the whole input
>   into one grand-total row (one row even over an empty input);
> - `GroupingElement::rollup(exprs)` — every leading prefix of the list,
>   longest first, down to the empty set: `ROLLUP (a, b)` is `(a, b)`, `(a)`
>   and `()`;
> - `GroupingElement::cube(exprs)` — every subset of the list, the empty one
>   included;
> - `GroupingElement::sets(first)` — `GROUPING SETS`, exactly the sets its
>   elements stand for, in turn and duplicates kept. It takes its first
>   element and returns a `GroupingSets` whose `add` appends further ones and
>   which converts into a `GroupingElement`, so the empty `GROUPING SETS ()`
>   the grammar rejects has no value to build. Its elements are elements, so
>   a `ROLLUP` or `CUBE` nests inside it and contributes all its own sets;
>   `ROLLUP` and `CUBE` take expressions, because the grammar nests nothing
>   inside them.
>
> An item of a `ROLLUP` or `CUBE` that is an `Expr::tuple` is one unit of the
> list, kept or dropped whole — `ROLLUP ((a, b), c)` has three prefixes, not
> four. A `ROLLUP` or `CUBE` over no expressions stands for the one empty set.
>
> `SelectStatement::group_by_element(element)` appends an element to the same
> list the plain methods of `sql.ast.select.projection` fill: those add each
> expression as a set of one, so a statement built only from them groups and
> renders exactly as a flat expression list did, and elements and plain
> expressions interleave in call order. The method lives beside the element
> type rather than on `SelectStatement`'s own file, which is at its function
> cap, and pgorm's `QuerySelect::group_by_element` passes through to it.
>
> `Func::grouping(first)` builds `GROUPING(…)`, the function that reads which
> of its arguments the current row's set leaves out: a bitmask whose last
> argument is bit 0, a bit being set when that argument is not grouped. It
> returns a `Grouping`, whose `arg` adds further arguments and which converts
> into `SimpleExpr::Grouping`. It is deliberately not a `FunctionCall`:
> PostgreSQL's grammar spells `GROUPING` as its own expression, with at least
> one argument and with no `FILTER`, `WITHIN GROUP`, `DISTINCT` or `OVER`, so
> as a `FunctionCall` it would admit all four and the windowed projections of
> `sql.ast.window-statement` would accept it. It is how a query tells the
> NULL a subtotal row puts in an omitted column from a NULL in the data; that
> each argument is one of the query's grouping expressions — so that it has
> nothing to read under `GROUP BY ()` — is PostgreSQL's check (`42803`), not
> the builder's.

> [spec:pgorm:sem:sql.ast.select.union+1]
> `union(UnionType, query)` appends one compound-query arm and `unions(iter)`
> extends with many; arms accumulate in call order and are never merged or
> deduplicated. `UnionType` names all six of PostgreSQL's set operations —
> each of the three operators in its duplicate-eliminating and its
> duplicate-keeping form: `Distinct` renders `UNION` and `All` renders
> `UNION ALL`, `Intersect`/`IntersectAll` render `INTERSECT`/`INTERSECT ALL`,
> and `Except`/`ExceptAll` render `EXCEPT`/`EXCEPT ALL`. Each appended arm is
> rendered as a parenthesised SELECT after the operator. The AST does not
> verify that the arms project the same columns — that is left to PostgreSQL.
>
> The two UNION spellings are the inherited ones and MUST be documented on the
> enum itself, because `All` beside `IntersectAll` reads as a modifier and is
> not one: the four original names say which row set the operation yields,
> where the two new ones name the operator. Renaming them to `Union`/`UnionAll`
> is the fix that clause anticipates, not a defect it records.

## Ordering

> [spec:pgorm:req:sql.ast.order+3]
> `SelectStatement` and `WindowStatement` — the two statements PostgreSQL
> admits an ORDER BY on — share the `OrderedStatement` trait; the write
> statements do not implement it, per `sql.ast.update` and `sql.ast.delete`.
> Order expressions MUST accumulate in call order via `order_by` (column +
> `Order`), `order_by_expr`, `order_by_columns`, and the `*_with_nulls` variants
> which attach a `NullOrdering` (`First`/`Last`) rendered as `NULLS
> FIRST`/`NULLS LAST`. `clear_order_by` MUST remove all accumulated order
> expressions. There is no raw-string ordering verb: a verbatim SQL fragment
> reaches ORDER BY position only as an `Expr::raw` through `order_by_expr`.
>
> `Order` MUST support `Asc`, `Desc`, and `Field(Values)`; the `Field` variant
> renders a `CASE WHEN col=v_i THEN i ... ELSE n END` expression implementing
> explicit value ordering.

## Conditions

> [spec:pgorm:def:sql.ast.condition+1]
> `Condition` is a tree node holding a `condition_type`
> (`ConditionType::All` = conjunction, `ConditionType::Any` = disjunction), a
> `negate` flag, and child `ConditionExpression`s, where each child is either a
> nested `Condition` or a leaf `SimpleExpr`. `Condition::all()` and
> `Condition::any()` construct empty sets; `add` pushes a child; `add_option`
> pushes only when `Some`; `not()` toggles the negate flag; `is_empty`/`len`
> inspect the children. Those constructors are the only spelling — the type
> carries no shorthand alias and no shorthand macro.
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

> [spec:pgorm:req:sql.ast.condition.holder+2]
> WHERE and HAVING clauses are backed by `ConditionHolder`, whose contents are
> an `Option<Condition>`: absent until a condition is added, `Some` thereafter.
> There is exactly one way in — `add_condition`, reached through `cond_where`
> and `cond_having` — so the holder has no second, incompatible representation
> to mix with, and the API surface offers no operation that can fail on it.
> `ConditionalStatement::and_where`, `and_where_option`, and `and_having` are
> shorthands that lift a `SimpleExpr` through `IntoCondition` into
> `Condition::all().add(expr)` and delegate; a chain style built from
> per-link `AND`/`OR` operators is deliberately absent, so the two styles
> cannot disagree about how links combine.
>
> Repeated `cond_where` calls MUST conjoin: if both the current and the added
> condition are non-negated `All` sets the additions are appended flat into the
> existing set; otherwise the current contents and the addition are combined
> under a fresh `Condition::all()`. Order of calls is preserved in the rendered
> output.

## Expressions

> [spec:pgorm:def:sql.ast.expr+5]
> `SimpleExpr` is the expression tree node, with variants `Column(ColumnRef)`,
> `Tuple`, `Unary(UnOper, ..)` (the only unary operator is `Not`),
> `FunctionCall`, `Binary(lhs, BinOper, rhs)`, `SubQuery(Option<SubQueryOper>, ..)`,
> `Value` (parameterised), `Values`, `Raw(&'static str)` (verbatim SQL),
> `Template(SqlTemplate)`, `Keyword`, `AsEnum`, `Case` and `SimpleCase` (the
> searched and simple forms of `sql.ast.case`), `Subscript` (an array
> subscript or slice, `sql.ast.expr.subscript`), `Grouping` (the
> `GROUPING()` of `sql.ast.select.grouping`), `Constant` (inlined literal),
> and `LikePattern` (a `LIKE` pattern with its optional `ESCAPE`). `SqlTemplate` holds a template with `$1`-style splices
> (`$$` escaping a literal `$`) already resolved against the expressions it
> substitutes; its segments are private and its only constructor is
> `SqlTemplate::new`, which returns `Result`, so the AST cannot hold a template
> whose placeholders and values disagree — see `sql.render.custom-expr`.
>
> `Expr` is the entry-point builder holding a left operand plus pending
> unary/binary operator state; `Expr::col`, `Expr::val`, `Expr::expr`,
> `Expr::tuple`, `Expr::value` and `Expr::raw` construct expressions from
> columns, values, other expressions, and raw SQL — `Expr::raw` bound to
> `&'static str`, so only program text can be written verbatim — and
> `Expr::template`, `Expr::template_with_expr` and `Expr::template_with_exprs`
> do the same for templates, each returning `Result` because each pairs a
> template with substitutions.
>
> Subquery expressions carry an optional `SubQueryOper`: `Expr::exists`,
> `Expr::any`, `Expr::some`, and `Expr::all` wrap a `SelectStatement` in
> `EXISTS(...)`, `ANY(...)`, `SOME(...)`, and `ALL(...)` respectively.
> `From` conversions lift `Value`-convertible Rust primitives, `FunctionCall`,
> `ColumnRef`, `Keyword`, `CaseStatement`, `SimpleCaseStatement`, and finished
> `Expr` builders into
> `SimpleExpr`, which is what allows plain Rust values wherever
> `Into<SimpleExpr>` is accepted.

> [spec:pgorm:req:sql.ast.expr.operators+3]
> `Expr` and `SimpleExpr` MUST provide combinators that produce `Binary`/`Unary`
> nodes: comparisons `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, plus
> `equals`/`not_equals` for column-to-column comparison; arithmetic `add`,
> `sub`, `mul`, `div`, `modulo`; bit shifts `left_shift`, `right_shift`;
> `between`/`not_between` and the bound-sorting
> `between_symmetric`/`not_between_symmetric`; `is_null`, `is_not_null`, `is`,
> `is_not`, and the null-safe `is_distinct_from`/`is_not_distinct_from`;
> logical
> `and`, `or`, and `not` (prefix `NOT`); string/pattern operators `like`,
> `not_like`, `ilike`, `not_ilike` — a `LikeExpr` with an escape character MUST
> render an `ESCAPE` clause whose character is an inline constant — and
> `concat` (`||`).
>
> The null-safe pair is not a verbose spelling of `eq`/`ne`: under three-valued
> logic `a <> b` is NULL whenever either side is, so a nullable column passes
> neither the comparison nor its negation, and `IS DISTINCT FROM` is the only
> form that answers true or false for every pair. `between_symmetric` likewise
> exists for the failure it removes rather than for brevity — `BETWEEN` with
> its bounds written in the wrong order matches nothing and reports no error.
>
> PostgreSQL-specific operators MUST be available: full-text `matches` (`@@`)
> and containment `contains` (`@>`) / `contained` (`<@`), and temporal
> `at_time_zone`, whose right operand is an ordinary expression — a bound zone
> name or a column of them — and whose direction follows the left operand's
> type, as the server's operator does. Containment and
> `concat` are type-general rather than string-specific — PostgreSQL defines
> one `@>`, one `<@` and one `||` across arrays, ranges, `tsquery` and `jsonb`
> alike — so these three combinators MUST also serve as the JSON containment
> and merge tests, and `sql.ast.expr.json` MUST NOT name duplicates of them.
> The escape hatch `binary(op, rhs)` accepts any `BinOper`, whose variants
> further include regex (`~`, `~*`), trigram similarity and distance operators,
> pgvector distance operators, `Overlap`, and `Raw(&'static str)` for
> arbitrary operator text. Casts are expressed with `cast_as`
> (`CAST(expr AS type)`) and `as_enum`; aggregate shorthands `max`, `min`,
> `sum`, `count`, `count_distinct`, and `if_null` wrap the expression in the
> corresponding function call. JSON field, path and key-existence access is
> `sql.ast.expr.json`.

> [spec:pgorm:req:sql.ast.expr.in+1]
> `is_in`/`is_not_in` MUST build an `IN`/`NOT IN` over a `Tuple` of the given
> operands. When the operand collection is empty, rendering MUST fall back to a
> constant comparison rather than emit invalid SQL such as `IN ()`, and the two
> fall-backs MUST carry the vacuous truth value of the predicate they stand in
> for: an empty `IN` renders the always-false `'a' = 'b'`, because membership in
> the empty set holds for nothing; an empty `NOT IN` renders the always-true
> `'a' = 'a'`, because non-membership in the empty set holds for everything,
> including a NULL operand.
>
> `in_tuples` MUST build multi-column membership tests
> (`(a, b) IN ((..), (..))`) from `IntoValueTuple` rows, and
> `in_subquery`/`not_in_subquery` MUST build `IN (SELECT ...)` /
> `NOT IN (SELECT ...)` from a `SelectStatement`.

> [spec:pgorm:req:sql.ast.expr.eq-any]
> `eq_any`/`ne_all` MUST build the same membership tests over a *single array
> parameter*: `eq_any` the comparison `= ANY(<array>)` and `ne_all` its
> complement `<> ALL(<array>)`, with the operand collection gathered into one
> `Value::Array` by `Value::array` rather than spread across a `Tuple`. They are
> the paved road beside `is_in`/`is_not_in`, which stay: `IN` spends one
> placeholder per element, so each list length is distinct SQL text that a
> per-connection prepared-statement cache must hold separately, and a long
> enough list reaches PostgreSQL's 65535-parameter limit. Prefer `is_in` for a
> short literal list written into the query, and `eq_any` when the length varies
> at runtime.
>
> The operand type MUST be bound `Into<Value> + ValueType`, not `Into<Value>`
> alone: the array's element tag is read from the Rust type, so an empty list is
> still a typed array (see `sql.value.array`).
>
> The negation MUST be spelled `<> ALL`, not `NOT (… = ANY …)`. The two are
> equivalent under three-valued logic — a NULL element makes both NULL, as it
> makes `NOT IN` NULL — so the choice is which shape reaches the planner, and
> PostgreSQL's own parser settles it: it reads `x NOT IN (…)` as an `A_Expr` of
> kind `AEXPR_IN` whose operator name is `<>`, the same operator-level negation
> `x <> ALL(…)` parses to (`AEXPR_OP_ALL`, name `<>`), where `NOT (x = ANY(…))`
> parses to a `BoolExpr` wrapped around a second node.
>
> An empty collection MUST NOT be special-cased. Unlike `IN`, an array
> comparison over an empty array is valid SQL carrying the right vacuous truth
> on its own: `= ANY` over an empty array is false for every operand and `<>
> ALL` true for every operand, NULL operands included — the same asymmetry
> `sql.ast.expr.in`'s two constant fall-backs exist to reproduce. The statement
> text is therefore identical at every cardinality, including zero.
>
> `ANY`/`SOME`/`ALL` are quantifiers over the right operand of a comparison, not
> functions: `Func::any`/`some`/`all` render SQL only in that position, and are
> the escape hatch for the operators these two do not name.

> [spec:pgorm:req:sql.ast.expr.json]
> `Expr` MUST provide the JSON *operator* vocabulary as typed combinators:
> field access `get_json_field` (`->`) and `cast_json_field` (`->>`), path
> access `get_json_path` (`#>`) and `cast_json_path` (`#>>`), and key existence
> `has_json_key` (`?`), `has_any_json_keys` (`?|`) and `has_all_json_keys`
> (`?&`). They live in a child module of `expr` rather than in `Expr`'s main
> block, as the membership family does.
>
> The boundary is operators, not functions. PostgreSQL's `jsonb_*` calls —
> `jsonb_set`, `jsonb_build_object`, `jsonb_array_elements`, `jsonb_typeof` and
> the rest — are ordinary function applications a caller already spells with
> `Func::named`, and naming each one here would be a second, worse function-call
> syntax. The operators pgorm does not name (`-` and `#-` key/path deletion,
> the jsonpath operators `@?` and `@@`) stay reachable through `binary` with
> `BinOper::Raw`; only `@@` has a variant, shared with full-text `matches`.
>
> Containment (`@>`, `<@`) and concatenation (`||`) MUST NOT be duplicated
> here: PostgreSQL defines one operator each across every type that has them,
> so `contains`, `contained` and `concat` from `sql.ast.expr.operators` are
> already the JSON forms, and a JSON-specific alias would be a second name for
> the identical AST node.
>
> `get_json_path`, `cast_json_path`, `has_any_json_keys` and
> `has_all_json_keys` MUST gather their operand collection into a single
> `text[]` `Value::Array` via `Value::array`, exactly as `sql.ast.expr.eq-any`
> does: the operators take `text[]` and nothing else, so path depth and key
> count vary without varying the statement text. The element bound MUST be
> `Into<String>`, and `has_json_key`'s key bound likewise, rather than the
> `Into<SimpleExpr>` the two field accessors take. The asymmetry is the
> operators' own: `->` selects an object key by text *or* an array element by
> integer, so its right operand is any expression, while `jsonb ? integer` is
> not an operator PostgreSQL defines, so a numeric key MUST fail to compile
> instead of reaching the server.
>
> Empty collections MUST NOT be special-cased; each operator already carries
> the right vacuous truth. `?|` over an empty key list is false for every
> operand (no key of none can be present) and `?&` is true for every operand —
> the same asymmetry `eq_any`/`ne_all` carry, and the reason neither needs
> `sql.ast.expr.in`'s constant fall-backs. An empty `#>` path selects the
> document itself.
>
> The family's operand typing is PostgreSQL's, not pgorm's, and pgorm does not
> paper over it: `#>` and `#>>` apply to `json` and `jsonb` alike, while the
> `?` family is `jsonb`-only, so a `json` operand needs an explicit `cast_as`.
> `->>` and `#>>` return `text`, which ends a chain — no JSON operator applies
> to their result. Because these combinators return `SimpleExpr` and
> `SimpleExpr` carries no JSON methods, drilling in more than one step means
> re-entering the builder with `Expr::expr`; `#>` exists so that the common
> multi-step path needs one node instead of a nest of them.

> [spec:pgorm:req:sql.ast.expr.subscript]
> `SimpleExpr::Subscript(base, subscript)` MUST express PostgreSQL's array
> subscript, where `Subscript` is either `Index(expr)` — one element, `a[i]` —
> or `Slice(lower, upper)` with each bound an `Option<SimpleExpr>` — `a[l:u]`,
> `a[l:]`, `a[:u]` and `a[:]`. The node is reached from `Expr` through
> `index(i)`, `slice(l, u)`, `slice_from(l)`, `slice_to(u)`, and
> `subscript(Subscript)` for the one form the shorthands do not name, the
> slice with neither bound. They live in a child module of `expr`, as the
> JSON family does, and unlike every other combinator there they return an
> `Expr` rather than a `SimpleExpr`, because a subscripted value is an operand
> rather than a predicate: it goes on to be compared, or subscripted again,
> in the same chain.
>
> A chain of subscripts is one multi-dimensional access, and the AST keeps it
> as nested `Subscript` nodes that render as one: `.index(1).index(2)` is
> `a[1][2]`, the element at row 1, column 2. That is not the same query as
> subscripting the first access's result, `(a[1])[2]`: PostgreSQL types a
> single-index access as the array's element type, so the parenthesised form
> is refused (`42804`, cannot subscript type `integer`) where the chain
> answers the element, and `sql.render.subscript` therefore never
> parenthesises a subscripted base.
>
> The node carries PostgreSQL's semantics and does not adjust them, so what a
> caller from a zero-based language expects is wrong in four places, each
> held by the live suite: arrays count from 1; an index outside the bounds
> answers NULL rather than raising; a slice outside the bounds is cut to them,
> and is the empty array rather than NULL when nothing overlaps; and once any
> subscript in a chain is a slice, every one is, a plain index `i` beside a
> slice reading as `1:i`.
>
> Index and bounds are any `Into<SimpleExpr>`, so a column or an expression
> can index as readily as a literal; the server coerces each to `int4`, and a
> bound value is a placeholder it types as `int4`.

> [spec:pgorm:def:sql.ast.keywords+5]
> `Keyword` represents bare SQL keywords usable as expressions, and the variant
> set is closed: `Null`, `CurrentDate`, `CurrentTime`, and `CurrentTimestamp`,
> constructed by `Expr::current_date()`, `Expr::current_time()` and
> `Expr::current_timestamp()`. There is no caller-supplied keyword — an
> arbitrary word reaches keyword position only as an `Expr::raw`, which says
> raw SQL where a `Keyword` would have said identifier. Name helpers:
> `Name::runtime` wraps a runtime string as an identifier and `Asterisk`
> expresses `*` — as a bare projection or
> table-qualified via `(Table, Asterisk)` rendering `"table".*`. `ColumnRef`
> spans `Column`, `TableColumn`, `SchemaTableColumn`, `Asterisk`, and
> `TableAsterisk`; `TableName` spans plain and schema-qualified tables, and
> `FromItem` spans a `TableName` with an optional alias, `SubQuery`,
> `ValuesList`, and `FunctionCall`.

## INSERT statements

> [spec:pgorm:def:sql.ast.insert+3]
> `InsertStatement` is the INSERT AST node: a target table (`into_table`,
> taking the `NamedTable` of `[spec:pgorm:def:sql.types.table-ref+4]` — a name
> with an optional alias, which is the whole of what PostgreSQL's insert target
> admits, so a subquery, values list or function call cannot be inserted into,
> and an alias renders as `INSERT INTO "t" AS "a"`), a
> column list (`columns`, which replaces any previous list), a value source, an
> optional `Overriding`, an optional `OnConflict`, an optional
> `ReturningClause`, an optional WITH clause
> attached by `with(..)` (`query.build.with`), and an optional
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
>
> `overriding(Overriding)` sets the statement-scoped exemption an identity
> column's generation clause is otherwise absolute about: `SystemValue`
> accepts a supplied value for a `GENERATED ALWAYS AS IDENTITY` column that
> would otherwise raise `428C9`, and `UserValue` discards one supplied to a
> `BY DEFAULT` column. The two are the whole of PostgreSQL's
> `OVERRIDING { SYSTEM | USER } VALUE`, so the slot is a closed pair rather
> than a flag, and the last call wins. It belongs to the *statement* and not to
> the column definition, which is why the exemption can exist at all without
> weakening what `identity()` declares
> (`[spec:pgorm:req:sql.ddl.column-def+5]`): the declaration still refuses every
> insert that does not say this word.

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

> [spec:pgorm:req:sql.ast.on-conflict+1]
> `OnConflict` (attached with `InsertStatement::on_conflict`, which accepts
> anything converting into one) MUST be one of exactly two shapes:
> `AnyDoNothing`, carrying nothing, for the arbiter-less clause PostgreSQL
> admits only for `DO NOTHING`; or `Targeted`, pairing a `ConflictTarget` with
> a `ConflictAction`. There is no third shape, so a clause without an action
> and a `DO UPDATE` without the inference specification PostgreSQL demands are
> both unrepresentable per [dec:pgorm:invalid-states-unrepresentable].
>
> `ConflictTarget` MUST hold at least one `ConflictElement` — `Column` or
> `Expr` — plus an optional filter standing for a partial index's predicate.
> `OnConflict::column`/`expr` take the first element and `and_column`,
> `and_columns`, `and_expr`, `and_exprs` add further ones, so the empty target
> list the PostgreSQL grammar rejects has no value to build; the filter lives
> on the target because `ON CONFLICT WHERE ..` with no target is rejected too.
> `and_where`, `and_where_option` and `cond_where` MUST fold into that filter
> through the same merge `sql.ast.condition.holder` specifies.
>
> `ConflictAction` MUST be either `DoNothing`, carrying nothing, or `Update`
> holding a non-empty `ConflictAssignments` and its own optional filter — the
> only filter in the clause, because PostgreSQL accepts `WHERE` after
> `DO UPDATE SET ..` and nowhere else. `ConflictTarget::do_nothing` yields the
> first; `update_column` and `value` take the first assignment and yield a
> `ConflictUpdate`, whose `update_column`/`update_columns` add `Column`
> assignments (rendering `"col" = "excluded"."col"`), whose `value`/`values`
> add `Expr` assignments (rendering `"col" = <expr>`), and whose
> `and_where`/`and_where_option`/`cond_where` fold into the update's filter.
> A `ConflictUpdate` converts into an `OnConflict`, which is how a builder
> chain reaches the statement.

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

> [spec:pgorm:req:sql.ast.update+5]
> `UpdateStatement` MUST accumulate SET assignments in call order as
> `(column, expression)` pairs: `values(pairs)` pushes many, `value(col, expr)`
> pushes one, and any `Into<SimpleExpr>` is accepted on the right-hand side
> (values, keywords, `Expr::raw` fragments, subqueries). Duplicate columns are
> not deduplicated — each call appends. The statement also carries the target
> `table` — the `NamedTable` of `[spec:pgorm:def:sql.types.table-ref+4]`, so
> the target is a name with an optional alias and nothing else, rendering
> `UPDATE "t" AS "a" SET ..` when one is bound — a FROM relation list, a WHERE
> `ConditionHolder` (per `sql.ast.condition.holder`), an optional
> `ReturningClause`, and an optional WITH clause attached by `with(..)`
> (`query.build.with`). `get_values` MUST expose the accumulated assignment
> pairs for inspection.
>
> The FROM relation list MUST accumulate through `from(..)`, exactly as
> `sql.ast.select.from` describes for a SELECT, and MUST take the same
> currency: any `IntoFromItem`, so a plain table, an aliased table, a
> subquery, a function call, a values list or a `FromItem::Template` fragment
> all stand where a relation stands. It is a relation *list*, not a join tree:
> the statement holds no `JoinExpr`, so a caller writes the join condition as
> an ordinary WHERE predicate, which is how PostgreSQL's own `UPDATE .. FROM`
> reads and what its documentation recommends. That boundary is a deliberate
> first cut rather than an oversight — `JoinExpr`, `JoinKind` and `JoinOn` are
> `pub(crate)` per `sql.surface`, so joins in the write statements' relation
> lists would mean publishing the join shape and a whole
> `inner_join`/`left_join`/`right_join`/`full_outer_join`/`cross_join` family
> on two more statement types to buy syntax the WHERE already expresses. A
> caller who needs an outer join's null-extension drives the statement from a
> subquery FROM item that performs it.
>
> The statement MUST NOT carry ORDER BY expressions or a LIMIT: PostgreSQL
> admits neither on an UPDATE. `UpdateStatement` therefore does not implement
> `OrderedStatement` and has no `limit` method, so neither clause can be built
> to be rendered; an update over an ordered, limited set of rows is expressed
> by the caller as a subquery filter (`WHERE id IN (SELECT .. ORDER BY ..
> LIMIT ..)`). A FROM relation does not substitute for that: it widens what
> the statement can read, not how many rows it touches.

> [spec:pgorm:def:sql.ast.delete+4]
> `DeleteStatement` is the DELETE AST node: a target table set by
> `from_table` — the `NamedTable` of `[spec:pgorm:def:sql.types.table-ref+4]`,
> a name with an optional alias, rendering `DELETE FROM "t" AS "a"` when one is
> bound — a USING relation list, a WHERE `ConditionHolder` shared with the
> condition rules, and an optional `ReturningClause`. Like the other three
> statements it carries an optional WITH clause, attached by `with(..)`
> (`query.build.with`).
>
> USING is DELETE's spelling of UPDATE's FROM and MUST behave identically:
> `using(..)` accumulates a relation list of any `IntoFromItem`, carries no
> join tree, and leaves the join condition to WHERE, with the boundary
> `sql.ast.update` states. The two clauses differ in keyword only because
> PostgreSQL's grammar does — `FROM` is already spoken for on a DELETE by the
> target table — so a single `FromItem` list serves both and one rendering
> rule covers the pair.
>
> As with `sql.ast.update`, the statement MUST NOT carry ORDER BY expressions
> or a LIMIT — PostgreSQL admits neither on a DELETE — so it implements no
> `OrderedStatement` and offers no `limit`, and an ordered or limited delete is
> written as a subquery filter over a SELECT.

## WITH clauses and CTEs

> [spec:pgorm:def:sql.ast.with+3]
> `CommonTableExpression` defines one named query in a WITH clause and MUST be
> complete the moment it exists: `CommonTableExpression::new(table_name, query)`
> takes both mandatory parts, the query being any `QueryStatementBuilder` stored
> as a `SubQueryStatement` — the AST does not restrict UPDATE/DELETE CTEs;
> validity is left to PostgreSQL. Only the genuinely optional parts remain
> builder methods: the column list (`column`/`columns`) and the `materialized`
> flag rendering `MATERIALIZED` / `NOT MATERIALIZED`.
> `CommonTableExpression::from_select` derives a CTE from a `SelectStatement`,
> naming it `cte_<table>` after the first FROM table and deriving column names
> from aliases or plain column projections; it returns `Option<Self>`, yielding
> `None` when the select has no FROM table to take a name from rather than
> producing a nameless CTE. `try_set_cols_from_select` performs only the column
> derivation and reports `false` (leaving columns untouched) when any projection
> is an expression or wildcard it cannot name.
>
> The two shapes a WITH clause can take are distinct types rather than a
> `recursive` flag. `WithClause` is the non-recursive form and holds a non-empty
> CTE collection: `WithClause::new(cte)` takes the first, `cte` appends further
> ones, and `ctes` iterates them in order. `RecursiveWithClause` is the
> recursive form, described by `sql.ast.with.recursive`. `AnyWithClause` is the
> closed sum of the two, and `Into<AnyWithClause>` is what `stmt.with(clause)`
> accepts on each of select, insert, update and delete — the only way a clause
> reaches a statement.
>
> A clause has exactly one home, the statement that carries it. There is no
> wrapper type holding a clause plus the statement it prefixes, and therefore no
> `WithBody` bound to keep a select out of one: the invalid state that bound
> guarded is unconstructible because the second home no longer exists
> (`query.build.with.single`).

> [spec:pgorm:req:sql.ast.with.recursive+1]
> The recursive WITH form MUST be a distinct type, `RecursiveWithClause`, whose
> single `CommonTableExpression` is taken by `RecursiveWithClause::new` — a
> recursive WITH consists of exactly one CTE containing a union query, and the
> multi-CTE recursive clause is therefore not constructible. It renders
> `WITH RECURSIVE` and carries the optional `SEARCH` (`search`) and `CYCLE`
> (`cycle`) clauses, which only this form accepts, so no setting can be
> silently ignored. Neither this type nor `WithClause` can be built empty, so
> rendering asserts nothing and MUST NOT panic on any clause a caller can
> construct.
>
> `Search::new(order, expr, alias)` pairs a `SearchOrder` (`BREADTH`/`DEPTH`)
> with the expression tracking the path and the name of the generated order
> column; the name is a constructor argument rather than an optional alias on a
> `SelectExpr`, so the missing-alias failure cannot arise. `Cycle::new(expr,
> set, using)` likewise requires the node-identifying expression, the cycle-mark
> column name, and the path column name at construction, rendering
> `CYCLE <expr> SET <set> USING <using>`.

## Window statements

> [spec:pgorm:def:sql.ast.window-statement+5]
> `WindowStatement` describes an OVER window: PARTITION BY expressions
> (`partition_by`, and the `OverStatement` trait's
> `partition_by_columns`), ORDER BY expressions (shared
> `OrderedStatement` trait), and an optional `FrameClause`, set by
> `frame(f)` from anything `Into<FrameClause>`; a second call replaces the
> first.
>
> `FrameType` is all three of PostgreSQL's frame modes — `Range`, `Rows`,
> `Groups` — because the offset means something different under each and none
> of the three is expressible through the others: `Rows` counts rows, `Range`
> counts a distance in the ordering column's own values, and `Groups` counts
> whole peer groups, so `GROUPS 1 PRECEDING` reaches back past every row tied
> with the one before.
>
> A frame is begun from its mode, and its start is typed by the side of the
> current row it lies on, because PostgreSQL's grammar refuses every frame
> whose end comes before its start and none of those MUST construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). `FrameType`'s four methods
> name the start — `unbounded_preceding()`, `preceding(offset)`,
> `current_row()`, `following(offset)`, and no `UNBOUNDED FOLLOWING` — and
> return a `FrameStart<S>`, whose marker `S` is `FramePreceding`,
> `FrameCurrentRow` or `FrameFollowing`. Its `and_*` methods give the frame an
> end, `BETWEEN <start> AND <end>`, and each side offers only the ends that may
> follow it: after a preceding start `and_preceding(offset)`,
> `and_current_row()`, `and_following(offset)` and `and_unbounded_following()`;
> after `CURRENT ROW` all but `and_preceding`; after a following start only
> `and_following` and `and_unbounded_following`. A preceding or current-row
> start also stands alone as a whole frame, converting into a `FrameClause`,
> and a following start does not, because PostgreSQL reads a lone start as
> running to the current row, which lies behind it. `ROWS UNBOUNDED
> FOLLOWING`, `… AND UNBOUNDED PRECEDING`, `BETWEEN CURRENT ROW AND 1
> PRECEDING`, `BETWEEN 1 FOLLOWING AND CURRENT ROW` and `ROWS 1 FOLLOWING`
> therefore have no construction. The public `Frame` enum, which could spell
> all five, is gone; the bound is the crate's own `FrameBound`.
>
> An offset is any `Into<SimpleExpr>`, not a count, because under `Range` it is
> a value of the type PostgreSQL pairs with the ordering column's — an
> `interval` over a timestamp, a `numeric` over a numeric — which a count
> cannot spell. The builder does not know the ordering column's type, so the
> pairing is the server's to check, and these refusals are the server's: an
> offset whose type the column's does not pair with (`0A000` under `Range`,
> and `42804` under `Rows` and `Groups`, whose offsets are `bigint` counts); a
> `Range` offset in a window without exactly one ORDER BY column (`42P20`); a
> `Groups` frame in a window with no ORDER BY (`42P20`); an offset that reads a
> column (`42P10`); and a negative one (`22013`). A bound offset is a `$N` the
> server types from the same pairing — `interval`, `numeric` or `bigint` — so
> a text value bound where it expects an `interval` fails at bind, and the
> offset is spelled with a cast (`sql.render.placeholder-typing`).
>
> `FrameClause::exclude(e)` adds the `EXCLUDE` clause, `FrameExclusion` being
> `CurrentRow`, `Group`, `Ties` or `NoOthers`; a second call replaces the
> first, and a lone start has the same method. The clause is grammatical only
> inside a frame, and it is a method of the frame rather than of the window,
> so a window without a frame has nothing to call it on. Each exclusion is
> relative to the current row's peers — the rows the window's ORDER BY ties
> with it — so `Group` and `Ties` differ from `CurrentRow` only where the
> ordering ties, and `NoOthers` removes nothing: it is the default, spelled.
>
> A select projection references a window in one of two ways
> (`WindowSelectType`): `Query` embeds the window inline (`expr_window`,
> `expr_window_as` render `OVER ( ... )`), while `Name` references a named
> window (`expr_window_name`, `expr_window_name_as` render `OVER "w"`) that is
> declared at statement level with `SelectStatement::window(name, window)`,
> rendering a `WINDOW "w" AS ( ... )` clause. The statement holds at most one
> named window; a second `window()` call replaces the first.
>
> PostgreSQL accepts `OVER` only after a function call, so all four
> `expr_window*` constructors MUST take the windowed expression as a
> `FunctionCall` rather than anything convertible to `SimpleExpr` (per
> `[dec:pgorm:invalid-states-unrepresentable]`): a windowed column reference,
> arithmetic expression, `CASE` or `CAST` does not typecheck, so the AST with
> no valid rendering has no constructor. `SelectExpr`'s expression and window
> are therefore read-only after construction — `expr()` and `window()` read
> them, `SelectExpr::new`/`new_as` build the windowless forms, and the four
> constructors are the only source of a windowed one — so the pairing cannot
> be taken apart by mutating a projection in place. The alias stays writable:
> renaming a projection cannot invalidate the pairing.

## CASE expressions

> [spec:pgorm:def:sql.ast.case+2]
> PostgreSQL's two CASE forms are two types, because their arms have two
> shapes and a CASE mixing them has no spelling.
>
> `CaseStatement` builds the searched form: each `case(cond, then)` call
> appends a `WHEN <condition> THEN <result>` arm — the condition is any
> `IntoCondition`, so `Condition` trees render with their `AND`/`OR`/`NOT`
> structure inside the WHEN — and `finally(expr)` sets the optional `ELSE`
> result. `Expr::case(cond, then)` is its only constructor and takes the first
> arm, so a searched CASE holds at least one arm from the start: there is no
> armless `new()` and no `Default`. A `CaseStatement` converts into
> `SimpleExpr::Case`.
>
> `SimpleCaseStatement` builds the simple form, `CASE <operand> WHEN <value>
> THEN <result> … END`, whose arms hold *values* the one operand is compared
> with. `Expr::case_of(operand)` takes the operand and returns a
> `CaseOperand`, whose only method, `when(value, then)`, adds the first arm
> and yields the `SimpleCaseStatement`; that type's own `when` appends further
> arms in call order and `finally(expr)` sets the `ELSE`. Neither type offers
> the other's arm method, so a condition arm cannot join an operand CASE nor a
> value arm a searched one. The operand is a constructor argument rather than
> an optional field beside the arms, and the first arm is a step rather than a
> default: PostgreSQL's grammar requires at least one `WHEN`, so `CaseOperand`
> does not convert into an expression, and a simple CASE with no arm has no
> value to build. A `SimpleCaseStatement` converts into `SimpleExpr::SimpleCase`.
>
> The simple form is not an abbreviation of a searched CASE over `IS NULL`
> tests. Each arm is the comparison `operand = value`, which is unknown when
> either side is NULL, so a NULL operand matches no arm — `WHEN NULL`
> included — and yields the `ELSE`, or NULL without one; the searched form's
> `WHEN x IS NULL` is the spelling that catches it.
>
> Either form can be projected (with `expr_as`), compared, nested in the
> other's results, or used anywhere an expression is accepted. Neither can be
> built without a `WHEN` — the searched form's constructor takes the first arm
> and the simple form's operand step converts into nothing — so `(CASE END)`
> and `(CASE ELSE … END)`, which the grammar rejects, have no construction
> (`[dec:pgorm:invalid-states-unrepresentable]`).

## Casts

> [spec:pgorm:req:sql.ast.cast-shape]
> A cast has exactly ONE node shape: `SimpleExpr::AsEnum(TypeName, operand)`.
> Every spelling that produces one — `as_enum`, `cast_as`, `cast_as_type`, the
> entity layer's enum casts, and the `cast_as_raw` escape hatch — builds
> that node, and there is no `Function::Cast`. Whether the type renders as a
> quoted identifier or as the caller's own verbatim text is carried *inside*
> the `TypeName` (`[spec:pgorm:def:sql.types.type-name+7]`), never by choosing
> a different node.
>
> What this forbids is the second, `FunctionCall`-shaped cast whose type rode
> as a `SimpleExpr::Raw` operand, and it forbids it for two reasons.
> A consumer reading a cast back off an expression must recognise one shape
> rather than enumerate them: `source_read_cast` recognising only the
> structured shape is why a `#[pgorm(select_as = "…")]` column's read cast
> was silently dropped, so the graph and pipeline decoded the column's stored
> type instead of its cast one — a defect no test of either shape alone could
> have found. And raw text in expression position is an injection site by
> construction; confining verbatim type text to one field of `TypeName`,
> reachable only through `cast_as_raw`, makes the places that can emit an
> unescaped type name a finite list that inspection can enumerate.

## Function calls

> [spec:pgorm:def:sql.ast.func+5]
> `FunctionCall` pairs a `Function` selector with argument expressions and
> per-argument modifiers (`FuncArgMod { distinct }`); `arg` appends one
> argument, `args` replaces the argument list. The `Function` enum covers the
> built-ins with typed constructors on the `Func` helper: aggregates `max`,
> `min`, `sum`, `avg`, `count`, `count_distinct` (the DISTINCT argument
> modifier), `bit_and`, `bit_or`; the ordered-set aggregates
> `percentile_cont` and `percentile_disc`; scalar helpers `abs`,
> `char_length`, `if_null`, `coalesce`, `lower`, `upper`, `round`,
> `round_with_precision`, `random`, `starts_with`, `gen_random_uuid`; the
> PostgreSQL full-text family `to_tsquery`, `to_tsvector`,
> `phraseto_tsquery`, `plainto_tsquery`, `websearch_to_tsquery` (each with an
> optional `regconfig` OID prepended as first argument), `ts_rank`,
> `ts_rank_cd`; and array/subquery comparators `any`, `some`, `all`.
>
> Beyond its arguments a call carries the two clauses PostgreSQL admits after
> them, each optional and each empty by default:
>
> - A FILTER condition, set by `filter(cond)` taking any `IntoCondition`.
>   Each call REPLACES the previous one rather than accumulating — a
>   conjunction is a single `Condition`, so accumulation would give one
>   meaning two spellings and make `filter` the only builder method on the
>   type whose second call means something other than its first.
> - A WITHIN GROUP ordering, a `Vec<OrderExpr>` accumulated by
>   `within_group(col, order)` and `within_group_expr(expr, order)`, mirroring
>   `order_by` / `order_by_expr` of `sql.ast.order`. It accumulates because
>   the hypothetical-set aggregates rank against several columns at once.
>   `OrderExpr`'s fields are not public, so these constructors are the only
>   way to build the payload and there is no list-taking form to offer.
>
> The pair is stored as one `Option<Box<..>>` rather than as two inline
> fields, and that is a size decision rather than a stylistic one.
> `SimpleExpr::FunctionCall` holds a `FunctionCall` inline, and `SelectExpr`,
> `Search` and `Cycle` hold a `SimpleExpr` inline in turn, so every byte here
> is paid by the whole expression tree — including by the scalar calls
> (`LOWER`, `COALESCE`, `ROUND`) that can never carry either clause. Two
> inline fields grew `AnyWithClause` past clippy's `large_enum_variant`
> threshold; one boxed field that is `None` until a clause is set costs eight
> bytes and one allocation only on the calls that actually aggregate.
>
> `get_func`, `get_args`, `get_mods`, `get_filter` and `get_within_group`
> expose the whole of a call for inspection: a consumer outside this crate
> reads a `FunctionCall` only through these, so a field added without one
> would be invisible to it — and since the modifiers are stored boxed and
> merged, the accessors are what make that storage an implementation detail
> rather than a shape a caller has to know. `get_within_group` MUST answer
> the empty slice, not an `Option`, when no ordering was supplied: absent and
> empty are the same statement about the call.
>
> The AST does not police which functions the two clauses are meaningful on.
> `FILTER` is valid on any aggregate and `WITHIN GROUP` only on an ordered-set
> or hypothetical-set one, but that is a property of the function the server
> resolves — including `Function::Named` ones this enum has never heard of —
> so the check belongs to PostgreSQL, which makes it, and not to a builder
> that would have to guess.
>
> Both clauses stop at this crate's own builders. `pgorm::pipeline`'s
> aggregate verbs do NOT expose them, and that is a boundary rather than an
> omission: the pipeline's SQL is written by prqlc from its RQ, never by this
> renderer, and prqlc has no representation to carry either clause — every
> function it constructs in `sql/gen_expr.rs` is built with `filter: None`
> and `within_group: vec![]`, with no PL or RQ construct that reaches them.
> The only route would be the written-SQL mechanism
> `[spec:pgorm:sem:pipeline.count-argument]` uses, which would mean
> respelling every aggregate verb as a written call and re-deriving its
> interaction with the OVER attachment those rewrites already perform. That
> is its own piece of work, not a pass-through, and until it is done a
> pipeline caller writes conditional aggregation the way PRQL does: filtering
> before the group, or aggregating over a `case`.
>
> A cast is not among them. `CAST` is written by `SimpleExpr::AsEnum`
> (`[spec:pgorm:req:sql.ast.cast-shape]`), so there is no `Function::Cast`
> and no `Func` constructor that produces one — a consumer matching on a
> `FunctionCall` never has to consider a cast. Nor is `GROUPING`: the
> grammar spells it as an expression of its own, so `Func::grouping` returns
> the `Grouping` of `sql.ast.select.grouping` rather than a `FunctionCall`.
>
> `Func::named(name)` calls an arbitrary function by identifier
> (`Function::Named`). A `FunctionCall` converts into
> `SimpleExpr::FunctionCall`, and can serve as a FROM item through
> `SelectStatement::from_function`.
