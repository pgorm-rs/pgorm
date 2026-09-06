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

> [spec:pgorm:def:sql.render.writer+2]
> `SqlWriter` (`prepare.rs`) is the output sink trait for rendering:
> `fmt::Write + ToString` plus `push_param(value)` and
> `push_param_source_typed(value)`, whose default body is
> `push_param` and which only sinks that emit placeholders override (see
> `sql.render.cast-param-type`). Neither takes a `QueryBuilder` argument: the
> builder is a stateless unit struct, so a sink that needs it names it
> directly. Two sinks exist:
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

> [spec:pgorm:req:sql.render.param-vs-inline+1]
> `SimpleExpr::Value` MUST be rendered through `SqlWriter::push_param` (a `$N`
> placeholder in the `build()` path), while `SimpleExpr::Constant` MUST always
> be rendered inline via `value_to_string`, regardless of sink. `LIMIT` and
> `OFFSET` amounts on SELECT — the only statement that carries either — are
> stored as `Value`s and MUST be parameterized (` LIMIT $N`, ` OFFSET $N`). The
> values in an `Order::Field` ordering (see `sql.render.select-order`) are
> inlined via `value_to_string` even in parameterized mode.

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
> of `[spec:pgorm:req:exec.cursor.binding-coerce+2]` widens it, and because
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

> [spec:pgorm:sem:sql.render.placeholder-typing]
> Every `SimpleExpr::Value` renders as a `$n` placeholder
> (`sql.render.param-vs-inline`), and PostgreSQL types a placeholder from the
> context it appears in. A placeholder in a position that supplies *no* context
> resolves to `text`, and the value the driver then binds is neither the type the
> server inferred nor convertible to it. The two positions that supply no context
> are a bare projection — `SELECT $1`, whether standalone, in a LATERAL marker
> subquery, or as the anchor arm of a recursive CTE, whose column types the whole
> recursion is resolved from — and any operand whose siblings are themselves
> untyped.
>
> The failures are the server's, at parse or bind time, not the renderer's: a
> recursive CTE whose anchor projects a bare `$n` settles that column as `text`
> and the recursive arm's `"col" + $n` fails `42883`
> (`operator does not exist: text + …`), while a bare `SELECT $1` bound with an
> integer fails `22021` (`invalid byte sequence for encoding "UTF8"`) because the
> driver wrote `int4` bytes where the server asked for `text`.
>
> pgorm-query annotates a placeholder in exactly one place — the operand of a
> cast, per `sql.render.cast-param-type` — and MUST NOT guess a type anywhere
> else: the renderer has no catalog and no expression typing, so any other
> annotation would be a guess that silently changes what the statement means.
> Supplying the context is therefore the caller's obligation, and it has two
> spellings: annotate the value with `Expr::val(v).cast_as("<type>")`, which
> renders `CAST($n::<pin> AS <type>)` and gives the position a type; or, where the
> value is a fixed literal rather than caller data, use `SimpleExpr::Constant`,
> which is inlined and so is never a placeholder at all
> (`join_lateral_on_true`'s `TRUE` is this case, per `query.build.lateral`).

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

> [spec:pgorm:def:sql.render.value-literals+2]
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
> `value_to_string`, joined by `,` with no spaces. An *empty* array MUST carry a
> cast naming its element type — `ARRAY []::int4[]`, the name taken from
> `ArrayType::source_type_name` — because there is no element for PostgreSQL to
> infer a type from and it rejects the bare `ARRAY []` with "cannot determine
> type of empty array". The grammar accepts that bare form, so no render oracle
> can catch it (`sql.render.oracle` is syntax-only) and the live-server suite is
> the only witness. The `Json` and `Vector` element tags have no single type
> name to pin (see `sql.render.cast-param-type`), so their empty arrays keep the
> untypeable spelling. `Vector(Some(v))` renders as a quoted pgvector literal
> `'[f1,f2,…]'`.
>
> `Keyword` expressions render as bare `NULL`, `CURRENT_DATE`, `CURRENT_TIME`,
> or `CURRENT_TIMESTAMP`; `Keyword::Custom` is written unquoted.

## Operators, precedence, and parentheses

> [spec:pgorm:def:sql.render.operators+3]
> `prepare_bin_oper` defines the operator lexicon. Logical/predicate:
> `AND`, `OR`, `LIKE`, `NOT LIKE`, `ILIKE`, `NOT ILIKE`, `IS`, `IS NOT`, `IN`,
> `NOT IN`, `BETWEEN`, `NOT BETWEEN`, `AS`. Comparison: `=`, `<>`,
> `<`, `>`, `<=`, `>=`. Arithmetic: `+`, `-`, `*`, `/`, `%`. Shift: `<<`,
> `>>`. PostgreSQL-specific: `@@` (Matches), `@>` (Contains), `<@`
> (Contained), `||` (Concatenate), `&&` (Overlap), `%` (Similarity), `<%`
> (WordSimilarity), `<<%` (StrictWordSimilarity), `<->`
> (SimilarityDistance), `<<->` (WordSimilarityDistance), `<<<->`
> (StrictWordSimilarityDistance), `->` (GetJsonField), `->>` (CastJsonField),
> `#>` (GetJsonPath), `#>>` (CastJsonPath), `?` (HasJsonKey), `?|`
> (HasAnyJsonKeys), `?&` (HasAllJsonKeys), `~` (Regex), `~*`
> (RegexCaseInsensitive), and pgvector's `<->` (EuclideanDistance), `<#>`
> (NegativeInnerProduct), `<=>` (CosineDistance).
> `BinOper::Custom(raw)` emits its raw string verbatim. Note the deliberate
> lexeme collisions: `%` serves both Mod and Similarity, `<->` both
> SimilarityDistance and EuclideanDistance. The only unary operator is
> `UnOper::Not` → `NOT`.
>
> The `?` family's lexemes are operator text like any other and carry no
> placeholder meaning: parameters are numbered `$N`
> (`sql.render.placeholders`), so a rendered `?` reaches the server as the
> JSON operator it is and nothing in the client rewrites it.
>
> `ESCAPE` is not an operator: it is grammatical only as the tail of a `LIKE`
> / `ILIKE` pattern, so it renders from `SimpleExpr::LikePattern` — the
> pattern as a value, then ` ESCAPE ` and the escape character as an inline
> constant — and there is no `BinOper` that could place it anywhere else.

> [spec:pgorm:def:sql.render.precedence+1]
> Parenthesis elision is driven by
> `inner_expr_well_known_greater_precedence(inner, outer)`, which returns true
> (safe to drop parens around `inner`) when: the inner expression is an atom —
> `Column`, `Tuple`, `Constant`, `FunctionCall`, `Value`, `Keyword`, `Case`, or
> `SubQuery` (the latter four are already self-wrapping); the inner expression
> is an arithmetic (`* / % + -`) or shift (`<< >>`) binary and the outer
> operator is a comparison, BETWEEN, IN, LIKE, or logical operator; or the
> inner expression is a comparison, IN, LIKE, or IS binary and the outer
> operator is logical (`AND`/`OR`/`NOT`). The Postgres-specific extension also
> treats an inner `@>`, `<@`, `%` (similarity), `<%`, `<<%`, `@@`, `?`, `?|`,
> or `?&` comparison as higher precedence than a logical outer operator — the
> membership of that set is "returns boolean", which is why the JSON existence
> operators join it and the JSON *accessors* (`->`, `->>`, `#>`, `#>>`, which
> return JSON or text) do not. All other combinations are considered unknown
> and keep their parentheses.

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

> [spec:pgorm:req:sql.render.select-order+2]
> `prepare_select_statement` MUST emit clauses in exactly this order: the
> statement's carried WITH clause when it has one (`query.build.with`), rendered
> through the same `prepare_with_clause` a `WithQuery` uses and therefore already
> ending in a separating space;
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

> [spec:pgorm:req:sql.render.joins+2]
> Join types MUST render as `JOIN`, `CROSS JOIN`, `INNER JOIN`, `LEFT JOIN`,
> `RIGHT JOIN`, `FULL OUTER JOIN`, followed by the joined table reference
> (prefixed `LATERAL ` when the join is marked lateral). A `JoinKind::Qualified`
> join then renders its constraint as an ` ON …` condition via
> `sql.render.condition-chain`; a `JoinKind::Cross` join renders nothing after
> the table, because PostgreSQL admits no `ON` after `CROSS JOIN` and the AST
> carries no condition for one (`sql.ast.select.join`).
> `JoinOn` has exactly one form, `Condition`, so `prepare_join_on` is total.
> There is no `USING (…)` output: the `JoinOn::Columns` variant that stood for
> it was never constructed by any builder and rendered `unimplemented!()`, and
> it MUST NOT return without a renderer.

> [spec:pgorm:sem:sql.render.locking]
> A lock clause renders as `FOR ` plus `UPDATE`, `NO KEY UPDATE`, `SHARE`, or
> `KEY SHARE`; then ` OF ` with comma-separated quoted table refs when tables
> are named; then optionally ` NOWAIT` or ` SKIP LOCKED`.

> [spec:pgorm:req:sql.render.window+3]
> A window specification is never emitted bare: `prepare_window_spec` wraps it
> in `( ` … ` )` (note the spaces inside the parentheses), and it is the only
> way a specification reaches the sink. Both spelling sites therefore agree —
> an inline windowed projection renders ` OVER ( … )` and a statement-level
> declaration renders ` WINDOW "name" AS ( … )` — and an empty specification
> renders as the legal `(  )`. A projection that references a declared window
> by name instead renders ` OVER "name"`, with any projection alias following
> the window in either form (` OVER … AS "alias"`).
>
> `OVER` reaches the sink only after a projection whose expression is a
> `SimpleExpr::FunctionCall`, because that is the only production PostgreSQL's
> grammar allows it after; a windowed column reference, arithmetic expression,
> `CASE` or `CAST` is rejected by the grammar no matter how it is spelled. The
> renderer carries no guard for this and MUST NOT grow one — the projection it
> would reject is not constructible (`sql.ast.window-statement`), so a guard
> would be dead code standing in for a type.
>
> Within the parentheses a specification renders `PARTITION BY expr, …`, then
> ` ORDER BY ` order-exprs, then the frame clause: ` RANGE ` or ` ROWS `,
> followed by either `BETWEEN start AND end` when an end bound exists or the
> start bound alone. Frame bounds render `UNBOUNDED PRECEDING`,
> `CURRENT ROW`, `UNBOUNDED FOLLOWING`; bounded offsets render the value —
> a `$N` parameter in the `build()` path, the literal inline — then a space
> and the keyword (`$1 PRECEDING`, `2 FOLLOWING`).

> [spec:pgorm:req:sql.render.subquery+1]
> A `SimpleExpr::SubQuery` MUST render its optional operator prefix (`EXISTS`,
> `ANY`, `SOME`, `ALL`) directly followed by the parenthesized sub-statement.
> A `SimpleExpr::Tuple` renders `(e1, e2, …)`; `SimpleExpr::Values` renders
> `(v1, v2, …)` with each element parameterized. As a FROM item,
> `FromItem::SubQuery` renders `(SELECT …) AS "alias"`, `FromItem::ValuesList`
> renders `(VALUES (…), (…)) AS "alias"`, and `FromItem::FunctionCall` renders
> `func(args) AS "alias"`; all three forms carry mandatory aliases, and
> `FromItem::Table` renders its `TableName` followed by ` AS "alias"` when an
> alias is bound. Contexts that require a plain identifier reference — DDL
> statements, index and foreign-key targets — take a `TableName` instead, so
> a value-bearing reference never reaches them
> (`[spec:pgorm:sem:sql.ddl.panics+4]`).

## CTEs

> [spec:pgorm:req:sql.render.cte+2]
> A `WithClause` renders `WITH ` followed by its comma-separated common table
> expressions; a `RecursiveWithClause` renders `WITH RECURSIVE ` followed by the
> single one it holds. Each CTE renders as: quoted table name; optional
> ` ("col", …)` column list; ` AS `; optional materialization hint
> (` MATERIALIZED ` or `NOT MATERIALIZED `); then the parenthesized
> sub-statement followed by a trailing space — so the separator between two CTEs
> reads `) , ` rather than `), `. For the recursive form the options render
> after the CTE: `SEARCH BREADTH FIRST BY ` / `SEARCH DEPTH FIRST BY ` expr
> ` SET "alias" `, and `CYCLE ` expr ` SET "col" USING "col" `. The renderer has
> nothing to refuse — the empty clause and the multi-CTE recursive clause are
> unrepresentable per `sql.ast.with` and `sql.ast.with.recursive` — so it
> carries no assertion and MUST NOT panic on a caller-built clause.
>
> The same `prepare_with_clause` serves both ways a clause reaches the sink: as
> the prefix a `SelectStatement` renders for its own carried clause
> (`sql.render.select-order`), and as the prefix of a `WithQuery`, whose attached
> statement is an INSERT, UPDATE or DELETE (`query.build.with.single`) reached
> through `SubQueryStatement`. The two are the same bytes, so a CTE query reads
> identically whichever route built it.

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

> [spec:pgorm:req:sql.render.on-conflict+1]
> When present, the conflict clause MUST render ` ON CONFLICT`, then its shape.
> `OnConflict::AnyDoNothing` MUST render ` DO NOTHING` and nothing else.
> `OnConflict::Targeted` MUST render ` (` the comma-separated target elements
> `)` — each a quoted conflict column or a rendered conflict expression —
> then, if the target carries one, ` WHERE ` and its condition; then the
> action. `ConflictAction::DoNothing` renders ` DO NOTHING`;
> `ConflictAction::Update` renders ` DO UPDATE SET ` with comma-separated
> assignments, where `ConflictAssignment::Column` renders
> `"col" = "excluded"."col"` (the `excluded` pseudo-table is double-quoted) and
> `ConflictAssignment::Expr` renders `"col" = expr`, followed by ` WHERE ` and
> its condition when the update carries a filter.
>
> Because the target list is non-empty and the assignment list is non-empty by
> construction, the renderer has no empty case to guard: every clause it can be
> handed is one the PostgreSQL grammar accepts. The one shape the grammar
> accepts but parse analysis does not — `ON CONFLICT DO UPDATE` with no
> inference specification — is unrepresentable per `sql.ast.on-conflict`, which
> is the only guard available since `sql.render.oracle` cannot see it.

> [spec:pgorm:req:sql.render.returning]
> A returning clause on INSERT, UPDATE, or DELETE MUST render as the final
> clause ` RETURNING ` followed by `*` (`ReturningClause::All`), a
> comma-separated list of column refs, or a comma-separated list of
> expressions. The pre-source `prepare_output` hook (SQL Server `OUTPUT`
> heritage) is a no-op in this backend.

> [spec:pgorm:req:sql.render.update-delete+1]
> `UPDATE ` renders the table, ` SET ` with comma-separated `"col" = expr`
> assignments, then WHERE and RETURNING. `DELETE ` renders `FROM ` and the
> table, then WHERE and RETURNING. Neither renders ORDER BY or LIMIT:
> PostgreSQL accepts neither on a write statement, and per `sql.ast.update`
> and `sql.ast.delete` the two statements hold nothing to render them from, so
> the renderer has no invalid clause to guard against. Row selection that needs
> an order or a limit is expressed by the caller as a subquery filter over a
> SELECT, which carries both.

## Custom expressions

> [spec:pgorm:req:sql.render.custom-expr+1]
> `SimpleExpr::Custom(s)` MUST be written verbatim, unescaped.
> `SimpleExpr::CustomWithExpr` carries a `CustomExpr`, whose template and
> substitutions are resolved against each other at CONSTRUCTION —
> `CustomExpr::new`, reached through `Expr::cust_with_values`,
> `cust_with_expr` and `cust_with_exprs`, all of which return
> `Result` — and never at render.
>
> Construction tokenizes the template with the SQL tokenizer (`sql.token`) and
> reads each `$` punctuation token: `$` followed by another `$` contributes one
> literal `$` (consuming both); `$` followed by an unquoted token parsing as a
> non-zero integer `N` denotes substitution `N`, counting from 1, and MAY be
> written any number of times. Every other `$` — one trailing the template, one
> standing before a space or punctuation, `$0`, or `$abc` — is a malformed
> placeholder and MUST be refused with
> `Error::Template { reason: MalformedPlaceholder | ZeroIndex }`; positional
> substitution of "the next value" is not a form this template language has.
> All other tokens pass through as literal text, and because the template is
> tokenized, placeholder-like text inside quoted tokens is neither substituted
> nor counted.
>
> The census MUST come out exact: the set of distinct `N` referenced MUST equal
> `1..=values.len()`. A reference past the end is
> `Error::Template { reason: IndexOutOfRange }`; a supplied value the template
> never names — including the value stranded by an arity hole such as `$1, $3`
> over three values — is `Error::Template { reason: UnreferencedValue }`.
> Rendering a placeholder as literal text, or dropping an unused value, is
> silent wrongness and is not permitted.
>
> What survives construction is a flat sequence of literal-text and
> resolved-expression segments carrying no indices, so rendering writes each
> text segment verbatim, renders each expression in place, and cannot fail or
> index out of range. This closes the render-time index panic that the previous
> version of this rule described, per `[dec:pgorm:no-panic]` and
> `[dec:pgorm:invalid-states-unrepresentable]`.
>
> A `SimpleExpr::AsEnum(type, expr)` at the top level is rewritten to a cast and
> renders as `CAST(expr AS type)`, with the type name written raw (unquoted) as
> a Custom expression.

## Parameter injection

> [spec:pgorm:sem:sql.render.inject+2]
> `inject_parameters(sql, params)` (`prepare.rs`) converts a parameterized SQL
> string back into inline SQL and returns `Result`. It tokenizes the input; a
> `$` punctuation token immediately followed by an unquoted token that parses
> as a non-zero `usize` `N` references parameter `N`, counting from 1, and MAY
> be referenced any number of times. Every other token is reproduced verbatim,
> a `$` that is not followed by such an integer included: unlike
> `sql.render.custom-expr`, `$$` is NOT an escape here, because what arrives is
> real SQL, where `$$` opens a dollar-quoted body rather than standing in for a
> literal `$`. Because quoted tokens are opaque to the tokenizer, `$N`
> sequences inside string literals or quoted identifiers are neither
> substituted nor counted.
>
> The census is settled before anything is written, and MUST come out exact:
> the distinct `N` referenced MUST equal `1..=params.len()`. `$0`, a reference
> past the end, and a parameter the SQL never names each return
> `Error::Template` — respectively `ZeroIndex`, `IndexOutOfRange` and
> `UnreferencedValue` — where the previous version of this rule panicked on the
> vector index. Each reference is paired with its parameter up front, so the
> writing walk holds no indices at all. String values are re-escaped on
> injection (e.g. `B'C` becomes `E'B\'C'`).

## DDL

> [spec:pgorm:def:sql.render.ddl.types+3]
> `prepare_column_type` defines the Rust-side `ColumnType` → PostgreSQL type
> name mapping (all lowercase): Char(n) → `char(n)`/`char`; String →
> `varchar(n)`/`varchar`; Text → `text`; SmallInteger → `smallint`; Integer →
> `integer`; BigInteger → `bigint`; Float → `real`; Double →
> `double precision`; Decimal → `decimal(p, s)`/`decimal`; Timestamp →
> `timestamp`; TimestampWithTimeZone → `timestamp with time zone`; Time →
> `time`; Date → `date`; Interval → `interval`, `interval(p)` or
> `interval <fields>` per `IntervalSpec`, the fractional-seconds precision
> spelled by the second-bearing field itself (`interval HOUR TO SECOND(3)`);
> Bytea →
> `bytea`; Bit → `bit(n)`/`bit`; VarBit → `varbit(n)`; Boolean → `bool`;
> Money → `money`; Json → `json`; JsonBinary → `jsonb`; Uuid →
> `uuid`; Array(t) → recursive element type plus `[]`; Vector →
> `vector(n)`/`vector`; Cidr → `cidr`; Inet → `inet`; MacAddr → `macaddr`;
> LTree → `ltree`; Custom/Enum → the identifier's raw string. The mapping is
> total — no variant is unsupported and none panics. An auto-increment column
> instead renders `smallserial`, `serial`, or `bigserial` by integer width;
> auto-increment on any other type renders that type's own spelling. Table
> DDL (`CREATE TABLE … ( … )`, `ALTER TABLE`
> add/modify/rename/drop column and add/drop foreign key, `DROP TABLE`,
> `TRUNCATE TABLE`, `ALTER TABLE … RENAME TO`), index DDL
> (`CREATE [UNIQUE ]INDEX … ON … [USING BTREE|GIN|HASH] (cols)` with
> optional ` NULLS NOT DISTINCT`), and foreign-key DDL (`FOREIGN KEY (…)
> REFERENCES … (…) [ON DELETE action] [ON UPDATE action]` with actions
> `RESTRICT`, `CASCADE`, `SET NULL`, `NO ACTION`, `SET DEFAULT`) are rendered
> by the same builder with identifiers quoted per `sql.render.ident-quoting`.

> [spec:pgorm:req:sql.render.ddl.enum-type+1]
> `CREATE TYPE` renders `CREATE TYPE name AS ENUM (…)` where each enum label
> is emitted through `prepare_value` — i.e. as a `$N` parameter in the
> `build()` path and as a quoted string inline in the `to_string()` path.
> `ALTER TYPE name` supports ` ADD VALUE v [BEFORE w | AFTER w]`,
> ` RENAME TO v`, and ` RENAME VALUE v TO w`; every label operand is likewise
> parameterized, but the `RENAME TO` target is a type name rather than a
> label and MUST render as a quoted identifier. `DROP TYPE [IF EXISTS ]name, … [CASCADE|RESTRICT]` renders
> type names via `TypeRef` with quoted, dot-joined parts. Callers executing
> these statements against PostgreSQL MUST use a rendering path that inlines
> the labels, since Postgres does not accept bind parameters in DDL.

> [spec:pgorm:sem:sql.render.ddl.extension+1]
> `CREATE EXTENSION [IF NOT EXISTS ]name [WITH SCHEMA s] [VERSION v]
> [CASCADE]` and `DROP EXTENSION [IF EXISTS ]name [CASCADE|RESTRICT]` MUST
> render the extension name and schema as quoted identifiers, escaped through
> `Alias` like any other identifier, and the version as a single-quoted string
> literal through `sql.render.string-escape` — the grammar takes a word or a
> string there, and a version like `v0.1.0` is not a word. The one string a
> DDL render still interpolates verbatim is `ColumnDef::extra`, which exists
> to carry SQL text the vocabulary cannot spell and is documented as caller
> responsibility by `sql.ddl.column-def`.
