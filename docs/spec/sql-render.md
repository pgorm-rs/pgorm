# SQL rendering (pgorm-query backend)

This spec covers the PostgreSQL SQL rendering engine in `pgorm-query/src/backend/`
(`query_builder.rs`, plus the `Oper` helper in `backend/mod.rs`) and the statement
preparation layer in `pgorm-query/src/prepare.rs`. It is a maintenance-scope spec:
rules capture what the renderer emits today, including known limitations, not what
an ideal Postgres renderer would emit.

## The renderer

> [spec:pgorm:def:sql.render]
> The SQL renderer is the unit struct `QueryBuilder` in
> `pgorm-query/src/backend/query_builder.rs`. It is the only backend: pgorm-query
> renders exclusively PostgreSQL-dialect SQL. `QueryBuilder` walks the statement
> AST (`SelectStatement`, `InsertStatement`, `UpdateStatement`, `DeleteStatement`,
> `WithQuery`, and DDL statement types) and emits SQL text into a `SqlWriter`
> sink through `prepare_*` methods. Rendering is infallible by construction: all
> writes `unwrap()`, and unsupported AST shapes abort via `panic!` /
> `unimplemented!` rather than returning errors.

> [spec:pgorm:def:sql.render.writer+1]
> `SqlWriter` (`prepare.rs`) is the output sink trait for rendering:
> `fmt::Write + ToString` plus `push_param(value, query_builder)` and
> `push_param_source_typed(value, query_builder)`, whose default body is
> `push_param` and which only sinks that emit placeholders override (see
> `sql.render.cast-param-type`). Two sinks exist:
>
> `String` implements `SqlWriter` by rendering each parameter inline —
> `push_param` appends `QueryBuilder::value_to_string(&value)` — producing a
> self-contained SQL string with no bind parameters (the `to_string()` build
> path). It takes the default `push_param_source_typed`: an inline literal has
> no wire format to disagree about.
>
> `SqlWriterValues` implements `SqlWriter` by emitting a placeholder and
> collecting the `Value` into an internal `Vec<Value>`. It is constructed with a
> placeholder string and a `numbered` flag; `into_parts()` returns the final
> `(String, Values)` pair (the `build()` path).

## Render conformance

> [spec:pgorm:req:sql.render.oracle]
> Every string `QueryBuilder` renders — through the `to_string()` path with
> literals inlined, or through the `build()` path with `$N` placeholders — MUST
> parse under the PostgreSQL grammar. The contract is enforced inside the test
> suite by an oracle: pgorm-query carries a dev-dependency on `pg_query`, the
> Rust binding to libpg_query (the PostgreSQL server's own parser), pinned to
> the 6.x line. That line carries the PG17 grammar, which for every construct
> this renderer emits is a superset of the PG16 grammar the live test server
> speaks; the 5.x line carries PG16 exactly but does not build against a current
> macOS SDK, whose `string.h` declares the `strchrnul` that libpg_query 16 also
> defines.
>
> The oracle is `pgorm-query/tests/postgres/oracle.rs`. `assert_parses` feeds a
> string to the parser and fails with the parser's message and a caret at the
> token it names; `assert_query_eq` pairs that check with the expected spelling;
> and an `assert_eq!` shim — imported by a test module in place of
> `pretty_assertions::assert_eq` — applies the oracle to any left-hand `String`
> or `&str` that opens with a statement keyword, so the existing render
> assertions are held to the grammar without being rewritten one by one.
> Rendered fragments that are not whole statements are skipped by that keyword
> test. `oracle_sweep.rs` additionally composes a matrix of builder outputs —
> select/join/CTE/window/union/locking shapes, INSERT/UPDATE/DELETE with
> RETURNING and ON CONFLICT, the DDL statements, the `ColumnType` and `BinOper`
> vocabularies, and the `build()` placeholder path — and requires every
> rendering to parse.
>
> The oracle is syntax-only: it has no catalog, so unknown tables and columns,
> misapplied type modifiers (`money(12, 2)`), an empty select list
> (`SELECT  FROM "t"`, which is valid PostgreSQL), and cross-database references
> all pass. The live-Postgres integration suite remains the semantic oracle.
>
> Known exceptions MUST be pinned, not skipped. Each render the grammar rejects
> today has an `oracle_pins_*` test in
> `pgorm-query/tests/postgres/oracle_pins.rs` asserting the rejection and naming,
> in a comment, the plan node that fixes it; the assertion site that produces
> such a render is marked `assert_eq_unparsed!` rather than left on the
> oracle-checked `assert_eq!`. A pin fails as soon as its render becomes valid,
> so a fix cannot land without the pin being retired.

## Placeholders and parameters

> [spec:pgorm:req:sql.render.placeholders]
> `QueryBuilder::placeholder()` MUST return `("$", true)`: parameters are
> rendered as numbered placeholders `$1`, `$2`, … When rendering into a
> `SqlWriterValues`, each `push_param` call MUST increment a counter that starts
> at 0 and write `${counter}` after incrementing, so the first parameter emitted
> is `$1`. Placeholder numbers therefore follow textual emission order exactly,
> and the collected `Values` vector is index-aligned with them (`$N` binds
> `values[N-1]`). Each parameter occurrence gets a fresh number; the renderer
> never deduplicates equal values into a shared placeholder.

> [spec:pgorm:req:sql.render.param-vs-inline]
> `SimpleExpr::Value` MUST be rendered through `SqlWriter::push_param` (a `$N`
> placeholder in the `build()` path), while `SimpleExpr::Constant` MUST always
> be rendered inline via `value_to_string`, regardless of sink. `LIMIT` and
> `OFFSET` amounts on SELECT, and `LIMIT` on UPDATE/DELETE, are stored as
> `Value`s and MUST be parameterized (` LIMIT $N`, ` OFFSET $N`). The values in
> an `Order::Field` ordering (see `sql.render.select-order`) are inlined via
> `value_to_string` even in parameterized mode.

> [spec:pgorm:req:sql.render.cast-param-type]
> A `SimpleExpr::Value` in the left operand of a `BinOper::As` — the operand of
> a cast, since that binary is the shape `Func::cast_as` builds and the shape
> `SimpleExpr::AsEnum` and `ColumnTrait::save_as` are rewritten into — MUST be
> rendered through `push_param_source_typed` rather than `push_param`.
> Postgres infers a placeholder's type from the cast target, but the driver
> writes the value in the format of the type it *is*, so an unpinned cast
> operand binds the wrong bytes: `CAST($1 AS BIT(8))` makes the server expect
> `bit` for what the driver sends as an `int8`, and it answers `22P03`.
>
> `SqlWriterValues` therefore appends `::{type}` to the placeholder, taking the
> name from `Value::source_type_name`: `bool`; `int2` for `TinyInt` and
> `SmallInt`, `int4` for `Int`, `int8` for `BigInt`, `Unsigned` and
> `BigUnsigned`; `float4` / `float8`; `text` for `String` and `Char`; `bytea`;
> `date`, `time`, `timestamp`, and `timestamptz` for all three zoned chrono
> variants; `uuid`; `numeric`; `inet`; `macaddr`; and, for `Array`, the element
> name from `ArrayType::source_type_name` suffixed `[]`. `Json` and `Vector`
> are `None` and stay unpinned — a JSON payload binds as either `json` or
> `jsonb` depending on which the server asked for, and the pgvector type name
> is not guaranteed to resolve in the search path.
>
> A pin names a type the value can be *bound* as, not necessarily the type its
> own `ToSql` impl would name: `TinyInt` pins to `int2` — rather than to the
> one-byte `"char"` an `i8` binds as by default — because the numeric coercion
> of `[spec:pgorm:req:exec.cursor.binding-coerce]` widens it, and because
> `CAST($1::int2 AS text)` yields the digits of the number where
> `CAST($1::"char" AS text)` would yield the character with that code point.
> `Unsigned` pins to `int8` rather than `oid` for the same reason. The one
> shape this costs is a cast from a `TinyInt` to `"char"` itself, for which
> `int2` has no cast: that (never-exercised) target now fails when the
> statement is parsed instead of binding.
>
> Pinning only annotates the placeholder — the outer cast still runs — so
> `CAST($1::text AS tea)` reaches an enum through Postgres' text I/O
> conversion, exactly as the unpinned form did.

## Identifiers and literals

> [spec:pgorm:req:sql.render.ident-quoting]
> The quote pair is `Quote(b'"', b'"')`. Every identifier rendered through
> `Iden::prepare` MUST be wrapped in double quotes with any embedded `"`
> doubled (`Iden::quoted` replaces the right-quote character with itself
> repeated twice: `he"llo` → `"he""llo"`). Quoting is unconditional — there is
> no reserved-word or safe-character check. This applies to column names, table
> names, schema/database qualifiers, aliases, CTE names, window names, and
> index/constraint/foreign-key names. Multi-part references join the quoted
> parts with `.` (e.g. `"schema"."table"."column"`). By contrast,
> `Function::Custom` function names and `Keyword::Custom` keywords are written
> via `Iden::unquoted`, i.e. raw with no quoting.

> [spec:pgorm:req:sql.render.string-escape]
> `QueryBuilder::escape_string` MUST apply exactly these replacements, in
> order: `\` → `\\`, `"` → `\"`, `'` → `\'`, NUL (`\0`) → `\0`, backspace
> (0x08) → `\b`, tab (0x09) → `\t`, 0x1A → `\z`, LF → `\n`, CR → `\r`.
> When rendering a string literal (`write_string_quoted`), the escaped text is
> wrapped in single quotes; if the escaped text contains any backslash the
> literal MUST instead be an E-string, `E'...'`, so Postgres interprets the
> backslash escapes. `unescape_string` is the inverse mapping (a backslash
> followed by `0 b t z n r` maps back to the control character; any other
> escaped character maps to itself).

> [spec:pgorm:def:sql.render.value-literals+1]
> `value_to_string` defines the inline literal syntax per `Value` variant:
>
> Every `None` variant of every `Value` type renders as the bare keyword
> `NULL`. `Bool` renders `TRUE` / `FALSE`. All integer, unsigned, float,
> double, and `Decimal` values render via their plain `Display` output,
> unquoted. `String` and `Char` render via `write_string_quoted` (see
> `sql.render.string-escape`); a `Char` is encoded as the full UTF-8 text of its
> scalar value and then escaped and quoted exactly as the equivalent
> one-character `String`, so `é` renders as `'é'`, `—` as `'—'`, and `'` as
> `E'\''`. The char is never narrowed to a single byte: rendering a char is
> total, and the rendered text always denotes the char that was given. `Json`
> renders its compact serialization as a quoted string. `Bytes` renders as a
> Postgres hex bytea literal `'\xAB01…'` with uppercase two-digit hex per byte.
> Chrono values render single-quoted with fixed formats: date `%Y-%m-%d`, time
> `%H:%M:%S`, naive datetime `%Y-%m-%d %H:%M:%S`, and all timezone-aware
> datetimes `%Y-%m-%d %H:%M:%S %:z` (fractional seconds are truncated). `Uuid`,
> `IpNetwork`, and `MacAddress` render their `Display` form in single quotes.
>
> `Array(_, Some(v))` renders `ARRAY [e1,e2,…]` — the keyword `ARRAY`, a
> space, then square brackets containing each element recursively rendered by
> `value_to_string`, joined by `,` with no spaces. `Vector(Some(v))` renders as
> a quoted pgvector literal `'[f1,f2,…]'`.
>
> `Keyword` expressions render as bare `NULL`, `CURRENT_DATE`, `CURRENT_TIME`,
> or `CURRENT_TIMESTAMP`; `Keyword::Custom` is written unquoted.

## Operators, precedence, and parentheses

> [spec:pgorm:def:sql.render.operators]
> `prepare_bin_oper_common` defines the operator lexicon. Logical/predicate:
> `AND`, `OR`, `LIKE`, `NOT LIKE`, `ILIKE`, `NOT ILIKE`, `IS`, `IS NOT`, `IN`,
> `NOT IN`, `BETWEEN`, `NOT BETWEEN`, `ESCAPE`, `AS`. Comparison: `=`, `<>`,
> `<`, `>`, `<=`, `>=`. Arithmetic: `+`, `-`, `*`, `/`, `%`. Shift: `<<`,
> `>>`. PostgreSQL-specific: `@@` (Matches), `@>` (Contains), `<@`
> (Contained), `||` (Concatenate), `&&` (Overlap), `%` (Similarity), `<%`
> (WordSimilarity), `<<%` (StrictWordSimilarity), `<->`
> (SimilarityDistance), `<<->` (WordSimilarityDistance), `<<<->`
> (StrictWordSimilarityDistance), `->` (GetJsonField), `->>` (CastJsonField),
> `~` (Regex), `~*` (RegexCaseInsensitive), and pgvector's `<->`
> (EuclideanDistance), `<#>` (NegativeInnerProduct), `<=>` (CosineDistance).
> `BinOper::Custom(raw)` emits its raw string verbatim. Note the deliberate
> lexeme collisions: `%` serves both Mod and Similarity, `<->` both
> SimilarityDistance and EuclideanDistance. The only unary operator is
> `UnOper::Not` → `NOT`.

> [spec:pgorm:def:sql.render.precedence]
> Parenthesis elision is driven by
> `inner_expr_well_known_greater_precedence(inner, outer)`, which returns true
> (safe to drop parens around `inner`) when: the inner expression is an atom —
> `Column`, `Tuple`, `Constant`, `FunctionCall`, `Value`, `Keyword`, `Case`, or
> `SubQuery` (the latter four are already self-wrapping); the inner expression
> is an arithmetic (`* / % + -`) or shift (`<< >>`) binary and the outer
> operator is a comparison, BETWEEN, IN, LIKE, or logical operator; or the
> inner expression is a comparison, IN, LIKE, or IS binary and the outer
> operator is logical (`AND`/`OR`/`NOT`). The Postgres-specific extension also
> treats an inner `@>`, `<@`, `%` (similarity), `<%`, `<<%`, or `@@`
> comparison as higher precedence than a logical outer operator. All other
> combinations are considered unknown and keep their parentheses.

> [spec:pgorm:req:sql.render.parens]
> `binary_expr` renders `left op right` and MUST parenthesize each operand by
> default, dropping parentheses only in these cases. Left operand: dropped when
> `sql.render.precedence` says the left is higher-precedence, or when the left
> is a binary expression with the *same* operator and that operator is
> well-known left-associative (`AND`, `OR`, `+`, `-`, `*`, `%`, plus `||` for
> Postgres) — so `a AND b AND c` and `a || b || c` render flat. Right operand:
> dropped when higher-precedence, or under one of three structural hacks for
> ternary constructs encoded as nested binaries: (1) the outer operator is
> BETWEEN/NOT BETWEEN and the right is an `AND` binary (`x BETWEEN a AND b`);
> (2) the outer operator is LIKE/NOT LIKE and the right is an `ESCAPE` binary
> (`x LIKE p ESCAPE e`); (3) the outer operator is `AS` and the right is a
> `SimpleExpr::Custom` (the `CAST(expr AS type)` encoding, where the type name
> is a Custom expression written raw). A unary `NOT` likewise wraps its operand
> in parentheses unless the operand is higher-precedence per
> `sql.render.precedence`.

> [spec:pgorm:sem:sql.render.empty-in+1]
> A binary `IN` or `NOT IN` whose right side is an empty tuple is rewritten to a
> comparison of two string values, rendering as `$N = $M` with those values as
> parameters (or with the literals inline under `to_string`). The rewrite is
> asymmetric, so each side keeps the truth value its predicate has over an empty
> set: empty `IN` becomes `'a' = 'b'` and matches no rows, empty `NOT IN`
> becomes `'a' = 'a'` and matches every row. Both operands are non-NULL literals
> in either form, so neither can evaluate to UNKNOWN.

## Conditions

> [spec:pgorm:req:sql.render.condition-chain+1]
> `prepare_condition(holder, keyword, …)` MUST emit nothing at all when the
> condition holder carries no condition; otherwise it emits ` {keyword} ` (with
> surrounding spaces; keyword is `WHERE`, `HAVING`, or `ON`) followed by the
> condition. There is exactly one rendering path: the `all`/`any` `Condition`
> tree is lowered by its `to_simple_expr()` conversion and written by the
> ordinary expression renderer, so parenthesisation and operator precedence
> follow `sql.render.precedence` alone. A holder carrying an empty `Condition`
> still emits the keyword, followed by that set's constant (`TRUE` for `all`,
> `FALSE` for `any`) per `sql.ast.condition.flattening`.

## SELECT

> [spec:pgorm:req:sql.render.select-order+1]
> `prepare_select_statement` MUST emit clauses in exactly this order:
> `SELECT`; optional distinct (`ALL`, `DISTINCT`, or `DISTINCT ON (col, …)`);
> the comma-separated select expressions; ` FROM ` with comma-separated table
> references (omitted entirely when no from-table); one space-separated join
> expression per join; the WHERE condition; ` GROUP BY ` expressions; the
> HAVING condition; the optional named window declaration
> ` WINDOW "name" AS ( … )`; any union clauses; ` ORDER BY ` expressions;
> ` LIMIT $N`; ` OFFSET $N`; and the row-locking clause. The window
> declaration belongs to the same query level as HAVING, so it MUST precede
> the set operations and the ORDER BY/LIMIT/OFFSET/locking tail that apply to
> the combined result — that is the position PostgreSQL's grammar requires,
> and emitting it later makes a named window unusable with any of them.
>
> Each order expression renders the expression, then ` ASC` / ` DESC`, then
> optionally ` NULLS FIRST` / ` NULLS LAST`. An `Order::Field(values)`
> ordering instead renders a `CASE WHEN expr=v0 THEN 0 WHEN expr=v1 THEN 1 …
> ELSE n END` expression with the values inlined, emulating MySQL
> `FIELD()`-style ordering. Unions render as ` UNION (…)`, ` UNION ALL (…)`,
> ` INTERSECT (…)`, or ` EXCEPT (…)` with the sub-select parenthesized.

> [spec:pgorm:req:sql.render.joins]
> Join types MUST render as `JOIN`, `CROSS JOIN`, `INNER JOIN`, `LEFT JOIN`,
> `RIGHT JOIN`, `FULL OUTER JOIN`, followed by the joined table reference
> (prefixed `LATERAL ` when the join is marked lateral), followed by the join
> constraint rendered as an ` ON …` condition via `sql.render.condition-chain`.
> Current limitation: `JoinOn::Columns` is not implemented — reaching it panics
> via `unimplemented!()` (`query_builder.rs:1018`); only `JoinOn::Condition` is
> renderable, so there is no `USING (…)` output.

> [spec:pgorm:sem:sql.render.locking]
> A lock clause renders as `FOR ` plus `UPDATE`, `NO KEY UPDATE`, `SHARE`, or
> `KEY SHARE`; then ` OF ` with comma-separated quoted table refs when tables
> are named; then optionally ` NOWAIT` or ` SKIP LOCKED`.

> [spec:pgorm:req:sql.render.window+1]
> A window specification is never emitted bare: `prepare_window_spec` wraps it
> in `( ` … ` )` (note the spaces inside the parentheses), and it is the only
> way a specification reaches the sink. Both spelling sites therefore agree —
> an inline windowed projection renders ` OVER ( … )` and a statement-level
> declaration renders ` WINDOW "name" AS ( … )` — and an empty specification
> renders as the legal `(  )`. A projection that references a declared window
> by name instead renders ` OVER "name"`, with any projection alias following
> the window in either form (` OVER … AS "alias"`).
>
> `OVER` MUST only be attached to a projection whose expression renders as a
> function call (`SimpleExpr::FunctionCall`, i.e. the `Func::…` constructors),
> because that is the only production PostgreSQL's grammar allows it after; a
> windowed column reference, arithmetic expression, `CASE` or `CAST` is
> rejected by the grammar no matter how it is spelled. The AST does not yet
> enforce this — see `sql.ast.window-statement`.
>
> Within the parentheses a specification renders `PARTITION BY expr, …`, then
> ` ORDER BY ` order-exprs, then the frame clause: ` RANGE ` or ` ROWS `,
> followed by either `BETWEEN start AND end` when an end bound exists or the
> start bound alone. Frame bounds render `UNBOUNDED PRECEDING`,
> `CURRENT ROW`, `UNBOUNDED FOLLOWING`; bounded offsets render the value as a
> parameter immediately followed by the keyword with **no separating space**
> (`$1PRECEDING`, `$1FOLLOWING`) — a known limitation of the current frame
> renderer, and the one window render PostgreSQL still rejects.

> [spec:pgorm:req:sql.render.subquery]
> A `SimpleExpr::SubQuery` MUST render its optional operator prefix (`EXISTS`,
> `ANY`, `SOME`, `ALL`) directly followed by the parenthesized sub-statement.
> A `SimpleExpr::Tuple` renders `(e1, e2, …)`; `SimpleExpr::Values` renders
> `(v1, v2, …)` with each element parameterized. As a table reference,
> `TableRef::SubQuery` renders `(SELECT …) AS "alias"`, `TableRef::ValuesList`
> renders `(VALUES (…), (…)) AS "alias"`, and `TableRef::FunctionCall` renders
> `func(args) AS "alias"`; all three forms carry mandatory aliases. Contexts
> that require a plain identifier reference (`prepare_table_ref_iden`, DDL
> statements, index/foreign-key targets) panic on value-bearing table refs
> ("Not supported" / "TableRef with values is not support").

## CTEs

> [spec:pgorm:req:sql.render.cte]
> A `WithClause` renders `WITH ` (or `WITH RECURSIVE `) followed by
> comma-separated common table expressions, each as: quoted table name;
> optional ` ("col", …)` column list; ` AS `; optional materialization hint
> (` MATERIALIZED ` or `NOT MATERIALIZED `); then the parenthesized
> sub-statement followed by a trailing space. For recursive queries the
> options render after the CTE list: `SEARCH BREADTH FIRST BY ` /
> `SEARCH DEPTH FIRST BY ` expr ` SET "alias" `, and `CYCLE ` expr
> ` SET "col" USING "col" `. The renderer MUST refuse (by `assert!` panic) a
> with-clause containing zero CTEs, and a recursive with-clause containing
> more than one CTE. The attached statement (`WithQuery`) may be any of
> SELECT/INSERT/UPDATE/DELETE/nested-WITH via `SubQueryStatement`.

## INSERT / UPDATE / DELETE

> [spec:pgorm:req:sql.render.insert]
> `prepare_insert_statement` MUST render: `INSERT` (or `REPLACE` when the
> statement's replace flag is set — kept from the MySQL-era API even though
> PostgreSQL has no such statement), ` INTO ` and the table; then either the
> default-values form or the explicit form. The default-values form (used when
> `default_values` is set with no columns and no source) renders the RETURNING
> hook then `VALUES ` and `(DEFAULT)` repeated `num_rows` times,
> comma-separated. The explicit form renders ` ("col1", "col2", …)` then
> either `VALUES ` with one parenthesized comma-separated expression row per
> value row, or, for insert-from-select, the rendered SELECT statement. ON
> CONFLICT (`sql.render.on-conflict`) and RETURNING (`sql.render.returning`)
> follow.

> [spec:pgorm:req:sql.render.on-conflict]
> When present, the conflict clause MUST render ` ON CONFLICT ` followed by:
> the optional parenthesized target list — each target either a quoted
> conflict column or a conflict expression; the optional target ` WHERE `
> condition; the action — ` DO NOTHING`, or ` DO UPDATE SET ` with
> comma-separated assignments where `OnConflictUpdate::Column` renders
> `"col" = "excluded"."col"` (the `excluded` pseudo-table is double-quoted)
> and `OnConflictUpdate::Expr` renders `"col" = expr`; and the optional action
> ` WHERE ` condition.

> [spec:pgorm:req:sql.render.returning]
> A returning clause on INSERT, UPDATE, or DELETE MUST render as the final
> clause ` RETURNING ` followed by `*` (`ReturningClause::All`), a
> comma-separated list of column refs, or a comma-separated list of
> expressions. The pre-source `prepare_output` hook (SQL Server `OUTPUT`
> heritage) is a no-op in this backend.

> [spec:pgorm:req:sql.render.update-delete]
> `UPDATE ` renders the table, ` SET ` with comma-separated `"col" = expr`
> assignments, then WHERE, ORDER BY, ` LIMIT $N`, and RETURNING. `DELETE `
> renders `FROM ` and the table, then WHERE, ORDER BY, ` LIMIT $N`, and
> RETURNING. Note that ORDER BY and LIMIT are rendered on UPDATE/DELETE when
> populated even though PostgreSQL does not accept them — the builder does not
> guard against producing them.

## Custom expressions

> [spec:pgorm:req:sql.render.custom-expr]
> `SimpleExpr::Custom(s)` MUST be written verbatim, unescaped.
> `SimpleExpr::CustomWithExpr(template, values)` tokenizes the template with
> the SQL tokenizer (`sql.token`) and substitutes placeholder markers: a `$`
> punctuation token followed by another `$` emits a literal `$` (consuming
> both); `$` followed by an unquoted integer `N` substitutes the rendered
> expression `values[N-1]`; a bare `$` followed by anything else substitutes
> the next expression positionally. Because the template is tokenized,
> placeholder-like text inside quoted tokens is never substituted. All other
> tokens pass through verbatim. A `SimpleExpr::AsEnum(type, expr)` at the top
> level is rewritten to a cast and renders as `CAST(expr AS type)`, with the
> type name written raw (unquoted) as a Custom expression.

## Parameter injection

> [spec:pgorm:sem:sql.render.inject]
> `inject_parameters(sql, params, query_builder)` (`prepare.rs`) converts a
> parameterized SQL string back into inline SQL: it tokenizes the input and,
> for each `$` punctuation token immediately followed by an unquoted token
> that parses as `usize` `N`, replaces the pair with
> `value_to_string(params[N-1])`; every other token is reproduced verbatim.
> Because quoted tokens are opaque to the tokenizer, `$N` sequences inside
> string literals or quoted identifiers are not substituted. The same
> parameter may be referenced any number of times, and out-of-range references
> panic on the vector index. String values are re-escaped on injection (e.g.
> `B'C` becomes `E'B\'C'`).

## DDL

> [spec:pgorm:def:sql.render.ddl.types+1]
> `prepare_column_type` defines the Rust-side `ColumnType` → PostgreSQL type
> name mapping (all lowercase): Char(n) → `char(n)`/`char`; String →
> `varchar(n)`/`varchar`; Text → `text`; TinyInteger and SmallInteger →
> `smallint`; Integer/Unsigned → `integer`;
> BigInteger/BigUnsigned → `bigint`; Float → `real`; Double →
> `double precision`; Decimal → `decimal(p, s)`/`decimal`; DateTime →
> `timestamp without time zone`; Timestamp → `timestamp`;
> TimestampWithTimeZone → `timestamp with time zone`; Time → `time`; Date →
> `date`; Interval → `interval [fields][(p)]`; Binary/VarBinary/Blob →
> `bytea`; Bit → `bit(n)`/`bit`; VarBit → `varbit(n)`; Boolean → `bool`;
> Money → `money(p, s)`/`money`; Json → `json`; JsonBinary → `jsonb`; Uuid →
> `uuid`; Array(t) → recursive element type plus `[]`; Vector →
> `vector(n)`/`vector`; Cidr → `cidr`; Inet → `inet`; MacAddr → `macaddr`;
> LTree → `ltree`; Custom/Enum → the identifier's raw string. `Year` is
> unsupported and panics. An auto-increment column instead renders
> `smallserial`, `serial`, or `bigserial` by integer width; auto-increment on
> any other type panics. Table DDL (`CREATE TABLE … ( … )`, `ALTER TABLE`
> add/modify/rename/drop column and add/drop foreign key, `DROP TABLE`,
> `TRUNCATE TABLE`, `ALTER TABLE … RENAME TO`), index DDL
> (`CREATE [UNIQUE ]INDEX … ON … [USING BTREE|GIN|HASH] (cols)` with
> optional ` NULLS NOT DISTINCT`), and foreign-key DDL (`FOREIGN KEY (…)
> REFERENCES … (…) [ON DELETE action] [ON UPDATE action]` with actions
> `RESTRICT`, `CASCADE`, `SET NULL`, `NO ACTION`, `SET DEFAULT`) are rendered
> by the same builder with identifiers quoted per `sql.render.ident-quoting`.

> [spec:pgorm:req:sql.render.ddl.enum-type]
> `CREATE TYPE` renders `CREATE TYPE name AS ENUM (…)` where each enum label
> is emitted through `prepare_value` — i.e. as a `$N` parameter in the
> `build()` path and as a quoted string inline in the `to_string()` path.
> `ALTER TYPE name` supports ` ADD VALUE v [BEFORE w | AFTER w]`,
> ` RENAME TO v`, and ` RENAME VALUE v TO w`, all label operands likewise
> parameterized. `DROP TYPE [IF EXISTS ]name, … [CASCADE|RESTRICT]` renders
> type names via `TypeRef` with quoted, dot-joined parts. Callers executing
> these statements against PostgreSQL MUST use a rendering path that inlines
> the labels, since Postgres does not accept bind parameters in DDL.

> [spec:pgorm:sem:sql.render.ddl.extension]
> `CREATE EXTENSION [IF NOT EXISTS ]name [WITH SCHEMA s] [VERSION v]
> [CASCADE]` and `DROP EXTENSION [IF EXISTS ]name [CASCADE] [RESTRICT]`
> interpolate the extension name, schema, and version strings raw — no
> identifier quoting, no escaping, no parameterization. Callers are
> responsible for the trustworthiness of these strings.
