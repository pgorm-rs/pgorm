# Pagination

pgorm offers two pagination mechanisms: keyset ("cursor") pagination over
ordered columns (`src/executor/cursor.rs`) and classic LIMIT/OFFSET page
pagination (`src/executor/paginator.rs`). The cursor module also defines
`ValueHolder`, the `ToSql` adapter used by every executor path to bind
`pgorm_query::Value` parameters, and with it the wire-type contract every
bound parameter is held to.

## Cursor pagination (`exec.cursor`)

> [spec:pgorm:def:exec.cursor+3]
> `Cursor<S, K>` wraps a `SelectStatement` plus the target table, an
> `Identity` of one or more order columns, an optional `Window` row
> limit, optional `before`/`after` boundary `ValueTuple`s, a `sort_asc`
> flag (default ascending), and a list of secondary order columns. `K` is
> the boundary shape the order columns fix — the `IntoIdentity::ValueType`
> of `[spec:pgorm:def:entity.relation.def+5]` — and defaults to
> `ValueTuple`. The boundaries are set by `before`/`after`, whose arity `K`
> fixes, or by `before_with`/`after_with`, which take the cursor's whole
> sort key including its secondary order columns and so cannot be typed by
> `K` (`[spec:pgorm:sem:exec.cursor.keyset+3]`). Cursors are created via
> `Select::cursor_by` (order columns on the entity's table) and, for joined
> selects, `SelectTwo::cursor_by` / `cursor_by_other` (order columns on the
> first or second entity respectively), each returning a cursor keyed by
> its argument's `ValueType`. `CursorTrait` names the `SelectorTrait` used
> to decode rows; `into_model` and `into_partial_model` re-target the
> decoded type and carry `K` across unchanged. `Cursor` also implements
> `QuerySelect` and `QueryOrder` for further query modification.
>
> `S` is unconstrained on the struct: only `Cursor::all` requires
> `S: SelectorTrait`, because only fetching needs to decode. `SelectUndecoded`
> exploits that — it is the `S` the `select_only` typestates' `cursor_by`
> returns (`query.build.modifiers`), and being no `SelectorTrait` it makes a
> cursor over a caller's projection unfetchable until `into_model` or
> `into_partial_model` names the row type.

**Deprecation.** `SelectTwo::cursor_by` / `cursor_by_other` are re-homed
onto the graph as `cursor_by` / `cursor_by_on`
(`[spec:pgorm:sem:query.graph.cursor]`): only the entry points move — the
`Cursor` type, the keyset machinery of
`[spec:pgorm:sem:exec.cursor.keyset+3]` and everything else this rule
states are unchanged. The two pair entry points remain normative while
`SelectTwo` exists and are retired with the pair surface
(graph/pair-deletion).

> [spec:pgorm:sem:exec.cursor.keyset+3]
> A cursor's *keyset* is the column list its rows are totally ordered by:
> the order columns, qualified with the cursor's table, followed by each
> unary secondary order entry qualified with its own table
> (`[spec:pgorm:sem:exec.cursor.order+1]`). `ORDER BY` and the boundary
> comparison MUST both be built from that one list, so the row order and
> the predicate that resumes it cannot disagree about where a page ends.
>
> `after(values)` filters to rows strictly beyond the boundary in the
> logical sort direction: column `>` value when ascending, `<` when
> descending. `before(values)` is the mirror image (`<` ascending, `>`
> descending). For a key of n columns the boundary is the row-value
> comparison `(c1, ..., cn) ⋈ (v1, ..., vn)` written out as
> `(c1 = v1 AND ... AND cn ⋈ vn) OR (c1 = v1 AND ... AND c(n-1) ⋈ v(n-1)) OR ... OR (c1 ⋈ v1)`
> where `⋈` is the direction comparison — one generic fold over the keyset,
> at every arity. Conditions are added to the composed query's `WHERE` via
> `cond_where`; both `before` and `after` may be set simultaneously.
>
> A boundary may be given at either of two arities, and its arity selects
> how much of the keyset it compares. At the arity of the order columns it
> compares those alone — the only shape available when there are no
> secondary entries, and the fallback for a joined cursor: it can say no
> more than "past every row sharing this order-column value", so resuming a
> page that ended part-way through such a run drops the rest of that run.
> At the arity of the whole keyset it compares the whole keyset, which
> names a single row and resumes from it exactly. Any other length MUST be
> reported when the filters are composed, as
> `Error::Query(RuntimeError::Internal)` reading "cursor boundary of arity
> {n} does not match {m} order column(s)" — with {m} written "{primary} or
> {keyset}" when the cursor has secondary entries. No arity mismatch
> panics.
>
> The order-column arity is a type error rather than a runtime one.
> `before` and `after` accept any `V: IntoBoundary<K>`, and for the `K` a
> `cursor_by` argument fixes, the only tuples implementing
> `IntoBoundary<K>` are those of the same length — so
> `cursor_by((A, B)).after(1)` does not compile, and neither does
> `cursor_by(A).after((1, 2))`. The one exception is a runtime-built
> `Identity`, whose `ValueType` is `ValueTuple`: that `K` admits any
> `IntoValueTuple`, keeping the arity check for execution. The extended
> arity has no such `K` to be checked against — it is the order columns'
> length plus a secondary count fixed at run time — so `before_with` and
> `after_with` take any `IntoValueTuple` and are checked when the filters
> are composed. They accept either arity, so widening a call site is
> adding `_with` and the trailing values.
>
> One limitation follows from comparing a joined table's column: under an
> outer join a secondary key value may be `NULL`, and a `NULL` operand
> makes the comparison `NULL` rather than true, so a row whose tiebreak
> column is null is excluded from the page that `ORDER BY` would place it
> in. Rows with a null keyset value are reachable through the order-column
> boundary, not through an extended one.
>
> The comparison direction is read when the filters are composed, from the
> cursor's final `sort_asc` — not at the call to `before`/`after`. So
> `after(x).desc()` and `desc().after(x)` build the same query, and both
> mean "the rows following `x` in descending order", i.e. those less than
> `x`, rather than the ascending sense the `after(x)` call site suggests.

> [spec:pgorm:sem:exec.cursor.window+1]
> The row limit is a single `Option<Window>`, where `Window` is
> `First(u64)` or `Last(u64)` and `Window::rows` projects out the count.
> A "first" and a "last" limit therefore cannot both be set: `first(N)`
> and `last(N)` each replace the whole window, so the most recent call
> wins. A set window applies `LIMIT rows`. `Last` fetches the window from
> the far end by flipping the emitted SQL sort order: the SQL order is
> ascending iff `sort_asc` XNOR `Last` (i.e. `asc` + `First` or `desc` +
> `Last` emit `ASC`; the other two combinations emit `DESC`). When the
> window is `Last`, the fetched buffer is reversed in memory after
> decoding, so `all` always returns rows in the cursor's logical
> (`asc`/`desc`) order regardless of windowing direction.

> [spec:pgorm:sem:exec.cursor.order+1]
> `Cursor::all` composes each execution onto a *copy* of the stored query:
> the limit, then the order clause, then the boundary filters are applied
> to the clone, which is then built and executed via `query_all` and
> decoded row by row with the selector. The stored query stays the one the
> caller handed the cursor, so a cursor MUST be re-executable — the clauses
> a `SelectStatement` replaces per call (`LIMIT`, `ORDER BY`) and the
> `WHERE` it only ever conjoins to are alike here, and a moved boundary or
> a flipped direction replaces the previous execution's rather than
> intersecting with it. `after(5)` then `after(2)` therefore pages from 2,
> not from `id > 5 AND id > 2`, and toggling `asc`/`desc` with a boundary
> set does not compose a page that is empty by construction.
>
> Ordering first clears any pre-existing `ORDER BY` on the copy, then
> orders by the cursor's keyset — its order columns qualified with its
> table, in declared order, then its secondary order entries qualified with
> theirs — all using the single resolved direction of
> `exec.cursor.window`. This is the same list the boundary comparison of
> `[spec:pgorm:sem:exec.cursor.keyset+3]` is built from. Only
> `Identity::Unary` secondary entries take part; composite secondary
> identities are silently ignored, in the ordering and in the boundary
> alike. `SelectTwo::cursor_by` automatically installs the other entity's
> primary-key columns as secondary order entries (and `cursor_by_other`
> installs the first entity's), so a joined cursor is totally ordered and
> can be resumed mid-tie through `after_with` / `before_with`.

> [spec:pgorm:def:exec.cursor.binding+4]
> `ValueHolder` (cursor.rs) is a public newtype over `pgorm_query::Value`
> implementing `tokio_postgres::types::ToSql`; every executor path
> (select, insert, update, delete, cursor, paginator) wraps built
> statement values in it for parameter binding. It delegates per variant:
> `Bool` to the primitive `bool` impl; the integer variants
> (`TinyInt`/`SmallInt`/`Int`/`BigInt`/`Unsigned`/`BigUnsigned`) and the
> float variants (`Float`/`Double`) through the numeric coercion of
> `[spec:pgorm:req:exec.cursor.binding-coerce+2]`, which also covers the
> `oid` and `"char"` targets the corresponding primitive impls define —
> `BigUnsigned` (u64) reaching all of them through a checked conversion to
> `i64`; `Char` is stringified; `String`, `Bytes`, `Json`, the chrono
> date/time variants, `Uuid`, and `Decimal` bind their payload, with
> `None` payloads emitted as SQL `NULL` (`IsNull::Yes`); `Array`
> recursively wraps its elements in `ValueHolder` (a `None` array is
> `NULL`), so the numeric coercion also applies element-wise against the
> array's member type.
>
> `Vector` delegates to `pgvector`'s own `ToSql` impl, which `pgorm-query`
> enables through pgvector's `postgres` feature. `IpNetwork` and
> `MacAddress` are encoded directly with
> `postgres_protocol::types::inet_to_sql` (address, prefix length, and
> `is_cidr` = 0 — so an `IpNetwork` keeps its prefix rather than being
> flattened to a host address) and `postgres_protocol::types::macaddr_to_sql`
> (the six raw bytes). Neither `ipnetwork` nor `mac_address` ships a `ToSql`
> impl and the orphan rule forbids adding one here, so these two wire
> formats are written by hand using the same helpers `postgres-types` uses
> for its own `cidr`/`eui48` support.
>
> `accepts` returns `true` for every Postgres type — not because every type
> is bindable, but because `accepts` is a static method with no access to
> the `Value` whose representation is the question. The acceptance decision
> is per-variant, so it lives where the variant is in hand: `to_sql`, which
> `to_sql_checked!` reaches and which runs client-side while the bind
> message is being encoded, before anything is sent. What it accepts is
> `[spec:pgorm:req:exec.cursor.binding-accepts]`.

> [spec:pgorm:req:exec.cursor.binding-coerce+2]
> When the Postgres type inferred for a placeholder is in the numeric
> family (`int2`, `int4`, `int8`, `float4`, `float8`, `numeric`),
> `ValueHolder::to_sql` MUST bind an integer- or float-valued `Value` in
> *that* type's wire format rather than in its own:
>
> - the integer variants (`TinyInt`, `SmallInt`, `Int`, `BigInt`,
>   `Unsigned`, `BigUnsigned`) convert to `int2`/`int4`/`int8` through
>   `i16`/`i32`/`i64::try_from`, and MUST return a `ToSql` error — never
>   panic — when the value does not fit; to `float4`/`float8` by `as`
>   conversion, rounding the way Postgres' own integer-to-float cast
>   does; and to `numeric` through an exact `Decimal`.
> - `BigUnsigned` (u64) reaches all of those through `i64::try_from`,
>   Postgres having no unsigned 64-bit type to write it in. The
>   conversion is checked, so a value above `i64::MAX` is the same
>   `ToSql` error as any other out-of-range integer, whatever the
>   inferred type — it MUST NOT wrap to a negative `int8`, which would
>   both store the wrong number and silently match the wrong rows. The
>   bound form therefore agrees with the inline literal rendering of
>   `[spec:pgorm:sem:sql.value.render]`, which prints the full `u64`:
>   the two spellings of a `Value` never mean different numbers.
> - `Float` and `Double` convert to `float8` by widening and to `float4`
>   by narrowing, erroring when a finite value narrows to an infinity, and
>   to `numeric` through `Decimal::try_from`, erroring when the value is
>   not representable. Binding a float against an integer type is an
>   error rather than a silent truncation.
>
> Two further integer targets are converted the same checked way rather
> than fallen through to a primitive impl: `oid` through `u32::try_from`
> and `"char"` through `i8::try_from`, so a `Unsigned` bound against `oid`
> and a `TinyInt` bound against `"char"` keep the encodings those impls
> define while a value that does not fit is an error instead of a
> reinterpretation. Every *other* inferred type is refused by
> `[spec:pgorm:req:exec.cursor.binding-accepts]`: an integer or a float has
> no representation `bytea`, `text`, `bool` or `uuid` could receive, and
> writing one anyway is what let a mismatch through. Because `Value::Array`
> binds through `Vec<ValueHolder>`, which hands each element the array's
> member type, both the conversions and the refusal apply element-wise.
>
> This is what makes an integer operand work against a floating-point
> column: `Expr::col(c).mul(2)` renders `"c" * $1`, for which Postgres
> infers `$1 :: float8`, and the `Int` value is written as a `float8`
> instead of producing an `08P01` protocol error.

> [spec:pgorm:req:exec.cursor.binding-gaps+3]
> `ValueHolder`'s `ToSql` implementation MUST reach every `Value` variant:
> no arm may `panic!`, `unimplemented!` or `todo!`, and no arm may reach a
> panic in a delegate. The former panicking arms are gone.
> `Value::TinyUnsigned` (u8) and `Value::SmallUnsigned` (u16) no longer
> exist as variants at all (see `[spec:pgorm:def:sql.value+2]`), so passing
> a `u8` or `u16` is a compile error rather than a runtime panic;
> `Value::Vector`, `Value::IpNetwork` and `Value::MacAddress` bind per
> `[spec:pgorm:def:exec.cursor.binding+4]`.
>
> The `Array` arm is where the delegate clause bites: `Vec<T>`'s own
> `to_sql` reads its member type out of `Kind::Array` and `panic!`s on
> anything else, so the arm MUST establish that the target is an array
> itself and raise the refusal of
> `[spec:pgorm:req:exec.cursor.binding-accepts]` when it is not. A
> `Value::Array` against a scalar placeholder is reachable — an active
> model whose column type is a scalar `jsonb` but whose Rust field is a
> `Vec` produces exactly that when the JSON-flattening branch of
> `cast_enum_as` is compiled out — so this is a live path, not a
> defensive one.
>
> One vestige remains, which was never reachable: the commented-out
> time-crate arms (`TimeDate`, `TimeTime`, `TimeDateTime`,
> `TimeDateTimeWithTimeZone`). This fork's `Value` has no such variants, so
> there is nothing to bind.
>
> An inferred type outside the numeric family is no longer ignored. It was:
> every non-numeric variant used to be written in its own binary format
> whatever Postgres inferred, and the claim that the server would report
> the difference was false — `"1234"` bound to an `int4` placeholder was
> accepted by the server and read as 825373492, the integer those four
> ASCII bytes spell. The same path serves predicates and writes, so a
> mismatch selected and stored wrong values rather than erroring.
> `[spec:pgorm:req:exec.cursor.binding-accepts]` closes it.
>
> `bits_tests` used to be the standing example of the gap, where saving an
> integer into a `BIT(n)` column made Postgres infer `bit` for a parameter
> the driver wrote as an `int8` (`22P03`). It is no longer one:
> `[spec:pgorm:req:sql.render.cast-param-type]` pins a cast operand's
> placeholder to the type the value is actually written as, and the test
> runs unignored.

> [spec:pgorm:req:exec.cursor.binding-accepts]
> A `Value` MUST NOT be written into a placeholder whose inferred Postgres
> type its binary representation is not the wire format of.
> `ValueHolder::to_sql` decides this per variant and raises a `ToSql` error
> — `` cannot bind a `<Variant>` value to Postgres type `<type>` `` — which
> tokio-postgres surfaces while encoding the bind message, so the statement
> never reaches the server. The failure arrives as an `Error::Postgres`
> carrying no `DbError`, the same shape as any other encoding failure (see
> `[spec:pgorm:sem:error.model.sql-class+3]`).
>
> The question the check asks is about *representation*, not about meaning.
> A type is accepted when the bytes this variant writes are the bytes that
> type is sent in; whether they mean what the caller intended is the
> caller's business. That is why `timestamp` and `timestamptz` are
> interchangeable below and why the numeric family is a family.
>
> Two things are transparent to the decision:
>
> - **Domains.** A domain's values are sent in the representation of the
>   type it is built over, so both the acceptance check and the encoding
>   are made against the base type, unwrapped transitively. Only the
>   diagnostic names the type as the schema declares it, so a refusal on a
>   `positive_int` says `positive_int`, not `int4`.
> - **`NULL`.** A `None` payload is sent as a length of -1 with no bytes at
>   all. Having no representation, it has none to mismatch, and binds
>   against whatever type Postgres inferred.
>
> The accepted targets, per variant:
>
> | `Value` variant | accepted Postgres types |
> | --- | --- |
> | `Bool` | `bool` |
> | `TinyInt`, `SmallInt`, `Int`, `BigInt`, `Unsigned`, `BigUnsigned` | `int2`, `int4`, `int8`, `oid`, `"char"`, `float4`, `float8`, `numeric` — converted per `[spec:pgorm:req:exec.cursor.binding-coerce+2]` |
> | `Float`, `Double` | `float4`, `float8`, `numeric`; `int2`/`int4`/`int8` are a distinct "without loss" refusal rather than a representation mismatch |
> | `String`, `Char` | `text`, `varchar`, `bpchar`, `name`, `xml`, `unknown`; every enum (a label is sent as its own text); and `citext`, `ltree`, `lquery`, `ltxtquery` — the text-backed extension types `postgres-types` itself names |
> | `Bytes` | `bytea` |
> | `Json` | `json`, `jsonb` |
> | `ChronoDate` | `date` |
> | `ChronoTime` | `time` |
> | `ChronoDateTime`, `ChronoDateTimeUtc`, `ChronoDateTimeLocal`, `ChronoDateTimeWithTimeZone` | `timestamp`, `timestamptz` |
> | `Uuid` | `uuid` |
> | `Decimal` | `numeric` |
> | `Array` | any array type; each element is then held to the member type by the same rule |
> | `Vector` | a type named `vector` |
> | `IpNetwork` | `inet`, `cidr` |
> | `MacAddress` | `macaddr` |
>
> All four chrono datetime variants take `timestamp` and `timestamptz`
> alike because the two share one representation — microseconds since
> 2000-01-01 — and differ only in whether that instant is read as a wall
> clock or as UTC. `postgres-types` draws the line in the same place for
> its own `SystemTime` impl. A `Value`-level check cannot tell the two
> apart and MUST NOT pretend to.
>
> Two consequences are deliberate. Postgres `money` is an `int8` on the
> wire, not a `numeric`, so a `Decimal` bound against it is now refused
> rather than writing `numeric` bytes into a parameter that would read them
> as a 64-bit integer; codegen maps a `money` column to `Decimal`
> (`pgorm-codegen/src/entity/column.rs`), so this turns a silent corruption
> into an error at the bind. And a column whose `save_as` cast does not
> reach its operand — `eq_any` over a `save_as` column, or a cursor keyset
> filter, both of which build the predicate on a raw `Expr` — leaves the
> placeholder typed by the column rather than by the cast, and a variant
> that does not match it is refused where it previously produced a server
> error or wrong bytes.


## Offset pagination (`exec.paginator`)

> [spec:pgorm:def:exec.paginator+1]
> `Paginator<'db, C, S>` holds either the `SelectStatement` to page over
> or the report explaining why the source could not be turned into one,
> plus a zero-based current `page`, a `page_size`, a borrowed connection,
> and a phantom selector. Carrying the failure rather than a stand-in
> statement is what lets `paginate` — whose signature returns no `Result`
> — accept a source it cannot page over without panicking and without
> inventing SQL to send in its place.
> `ItemsAndPagesNumber` carries `number_of_items` and `number_of_pages`.
> `PaginatorTrait::paginate(db, page_size)` constructs a paginator and is
> implemented for `Selector<S>`, `SelectorRaw<S>`, `Select<E>`, and
> `SelectTwo<E, F>` (the latter two via `into_model`). The trait also
> provides `count`, defined as `paginate(db, 1).num_items()`.
> `PinBoxStream` is the pinned boxed stream alias returned by
> `into_stream`.

> [spec:pgorm:req:exec.paginator.page-size+2]
> `page_size` is a `NonZeroU64` — in the `paginate` signature and in the
> `Paginator` field it is stored in — so a zero page size is not a value
> a caller can supply and not a state the paginator can hold. There is
> no assertion to trip, no panic to catch and no `Error` to recover:
> zero is rejected by the compiler at the call site. The page-count
> division of `[spec:pgorm:sem:exec.paginator.count]` is therefore total
> by construction, and `PaginatorTrait::count` names its page size of
> one as `NonZeroU64::MIN` rather than a literal the type would refuse.

> [spec:pgorm:sem:exec.paginator.fetch+2]
> `fetch_page(page)` executes a clone of the query with
> `LIMIT page_size OFFSET page_size * page`; pages are zero-indexed and
> the paginator's own cursor is not consulted or advanced. The offset is
> computed with checked multiplication: a `page` whose offset would not
> fit a `u64` is an `Error::Query`, not a debug-build panic and not a
> release-build wrap to a small offset that would silently serve the
> wrong rows. Rows are decoded through the selector, aborting on the
> first decode error. `fetch()` is `fetch_page(cur_page())`; `next()`
> increments the page counter without fetching; `cur_page()` reports it,
> starting at 0.

> [spec:pgorm:sem:exec.paginator.count]
> `num_items` counts by wrapping the paginator's query — with limit,
> offset, and `ORDER BY` stripped — as a subquery aliased `sub_query`
> under `SELECT COUNT(*) AS num_items`, decoding the count as `i64` and
> casting to `u64`; if the count query yields no row the result is 0.
> `num_pages` and `num_items_and_pages` derive the page count as
> `items / page_size + (items % page_size > 0)` — ceiling division, so a
> partial trailing page counts as a page and zero items yield zero
> pages. Each call re-runs the count query; item and page numbers are
> not cached.

> [spec:pgorm:sem:exec.paginator.iterate]
> `fetch_and_next` fetches the current page, then increments the page
> counter, returning `Some(items)` when the page is non-empty and `None`
> when it is empty. Termination is therefore detected only by an empty
> fetch: a result set that is an exact multiple of `page_size` costs one
> extra query returning zero rows. `into_stream` wraps this loop as an
> async stream yielding `Ok(Vec<S::Item>)` per non-empty page; the
> stream ends at the first empty page and yields the error (then ends)
> if any fetch fails.

> [spec:pgorm:sem:exec.paginator.raw+3]
> Paginating a `SelectorRaw` MUST decide what the raw statement is by
> parsing it with libpg_query — the PostgreSQL server's own parser, the
> same `pg_query` 6.2.0 the render oracle and `sql!` use
> (`[spec:pgorm:req:sql.render.oracle]`, `[spec:pgorm:def:macros.sql+1]`)
> — and MUST NOT decide it by inspecting the statement's text. The
> statement is accepted only when it parses, holds exactly one statement,
> and that statement is a `SelectStmt` carrying no `INTO` clause. A
> `WITH ... SELECT` therefore qualifies: PostgreSQL hangs the `WITH`
> clause off the `SelectStmt` itself rather than making it a statement of
> its own, so a CTE pages like any other `SELECT`. `VALUES (…), (…)` and
> set operations qualify for the same reason — both parse as a
> `SelectStmt`.
>
> An accepted statement is taken at the extent the parser reports for it,
> which excludes any terminating `;` that a subquery position would
> refuse, and is wrapped whole as
> `SELECT * FROM (<statement>\n) AS "sub_statement"`. Wrapping rather than
> splicing means `LIMIT` and `OFFSET` land outside the caller's own
> clauses instead of colliding with them, so a raw statement that already
> carries `ORDER BY` or `LIMIT` still pages correctly; PostgreSQL will
> not reorder rows a subquery sorted, so the caller's `ORDER BY` still
> governs page boundaries. The newline before the closing parenthesis is
> load-bearing: a statement ending in a `--` comment would otherwise
> swallow it.
>
> The wrapped statement's text MUST be copied verbatim and MUST NOT be
> re-lexed or rewritten — not by the `sql.token` tokenizer, which knows
> neither PostgreSQL comments nor dollar quoting
> (`[spec:pgorm:sem:sql.token.limits]`), and not by any other walk over
> the text. The caller's `$N` markers therefore keep the numbers the
> caller gave them, which is sound because nothing is bound ahead of
> them: the page clauses the paginator appends are numbered from `$N+1`
> where `N` is the count of bind values supplied, and the count query
> (`[spec:pgorm:sem:exec.paginator.count]`) appends no markers at all.
> Comment bodies, dollar-quoted strings (tagged and untagged),
> single-quoted and E-string literals, and bracketed subscripts therefore
> read the same paginated as they do executed directly, and a `$99`
> written inside any of them stays text.
>
> A marker the caller supplied no value for is refused at `paginate`
> rather than indexed. Which `$N` are markers is again PostgreSQL's own
> answer rather than a guess from the text — the statement is scanned
> with libpg_query's scanner — and any marker numbered above the count of
> bind values is recorded as the reason there is no statement to page
> over, naming the marker and how many values were supplied.
>
> Everything else — text the grammar rejects, a `;`-separated script, an
> `INSERT`/`UPDATE`/`DELETE`/DDL statement, a `SELECT ... INTO` — is
> neither a panic nor mangled SQL sent to the server. `paginate` records
> the reason, and every reader that can report it (`fetch_page`, and
> `num_items` with the page counts derived from it) returns it as a
> `Error::Query` naming what the statement actually parsed as, using
> PostgreSQL's own node name for anything it has no SQL keyword for.
