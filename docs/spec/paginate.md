# Pagination

pgorm offers two pagination mechanisms: keyset ("cursor") pagination over
ordered columns (`src/executor/cursor.rs`) and classic LIMIT/OFFSET page
pagination (`src/executor/paginator.rs`). The cursor module also defines
`ValueHolder`, the `ToSql` adapter used by every executor path to bind
`pgorm_query::Value` parameters. These rules capture current behavior,
including the remaining gaps in parameter binding.

## Cursor pagination (`exec.cursor`)

> [spec:pgorm:def:exec.cursor+3]
> `Cursor<S, K>` wraps a `SelectStatement` plus the target table, an
> `Identity` of one or more order columns, an optional `Window` row
> limit, optional `before`/`after` boundary `ValueTuple`s, a `sort_asc`
> flag (default ascending), and a list of secondary order columns. `K` is
> the boundary shape the order columns fix — the `IntoIdentity::ValueType`
> of `[spec:pgorm:def:entity.relation.def+4]` — and defaults to
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

> [spec:pgorm:def:exec.cursor.binding+2]
> `ValueHolder` (cursor.rs) is a public newtype over `pgorm_query::Value`
> implementing `tokio_postgres::types::ToSql`; every executor path
> (select, insert, update, delete, cursor, paginator) wraps built
> statement values in it for parameter binding. It delegates per variant:
> `Bool` to the primitive `bool` impl; the integer variants
> (`TinyInt`/`SmallInt`/`Int`/`BigInt`/`Unsigned`/`BigUnsigned`) and the
> float variants (`Float`/`Double`) through the numeric coercion of
> `[spec:pgorm:req:exec.cursor.binding-coerce]`, which falls back to the
> corresponding primitive impl when the inferred type is outside the
> numeric family — `Unsigned` (u32) to tokio-postgres's `u32` impl (which
> targets `OID`), `BigUnsigned` (u64) taken as `i64` throughout;
> `Char` is stringified; `String`, `Bytes`, `Json`, the chrono
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
> `accepts` returns `true` for every Postgres type. This is deliberate, not
> an oversight: a `Value` carries no target type, and the types it
> legitimately binds against include ones no client-side check could
> enumerate — every enum and domain whose binary format is the text of its
> label among them — so a faithful `accepts` would reject working queries.
> The whole burden therefore sits in `to_sql`, which converts within the
> numeric family, errors on a numeric conversion it cannot make exactly,
> and writes every other variant in its own binary format, leaving genuine
> mismatches to be reported by the server at execution time.

> [spec:pgorm:req:exec.cursor.binding-coerce]
> When the Postgres type inferred for a placeholder is in the numeric
> family (`int2`, `int4`, `int8`, `float4`, `float8`, `numeric`),
> `ValueHolder::to_sql` MUST bind an integer- or float-valued `Value` in
> *that* type's wire format rather than in its own:
>
> - the integer variants (`TinyInt`, `SmallInt`, `Int`, `BigInt`,
>   `Unsigned`, and `BigUnsigned` taken as `i64`) convert to
>   `int2`/`int4`/`int8` through `i16`/`i32`/`i64::try_from`, and MUST
>   return a `ToSql` error — never panic — when the value does not fit;
>   to `float4`/`float8` by `as` conversion, rounding the way Postgres'
>   own integer-to-float cast does; and to `numeric` through an exact
>   `Decimal`.
> - `Float` and `Double` convert to `float8` by widening and to `float4`
>   by narrowing, erroring when a finite value narrows to an infinity, and
>   to `numeric` through `Decimal::try_from`, erroring when the value is
>   not representable. Binding a float against an integer type is an
>   error rather than a silent truncation.
>
> Any other inferred type falls through to the variant's own `ToSql` impl,
> so a `Unsigned` bound against `OID` and a `TinyInt` bound against
> `"char"` keep the encodings those impls define. Because `Value::Array`
> binds through `Vec<ValueHolder>`, which hands each element the array's
> member type, the same conversions apply element-wise.
>
> This is what makes an integer operand work against a floating-point
> column: `Expr::col(c).mul(2)` renders `"c" * $1`, for which Postgres
> infers `$1 :: float8`, and the `Int` value is written as a `float8`
> instead of producing an `08P01` protocol error.

> [spec:pgorm:req:exec.cursor.binding-gaps+2]
> `ValueHolder`'s `ToSql` implementation MUST bind every `Value` variant:
> no arm may `panic!`, `unimplemented!` or `todo!`. The former panicking
> arms are gone. `Value::TinyUnsigned` (u8) and `Value::SmallUnsigned`
> (u16) no longer exist as variants at all (see
> `[spec:pgorm:def:sql.value+1]`), so passing a `u8` or `u16` is a compile
> error rather than a runtime panic; `Value::Vector`, `Value::IpNetwork`
> and `Value::MacAddress` bind per `[spec:pgorm:def:exec.cursor.binding+2]`.
>
> Two limitations remain, neither of which panics. The commented-out
> time-crate arms (`TimeDate`, `TimeTime`, `TimeDateTime`,
> `TimeDateTimeWithTimeZone`) are vestigial — this fork's `Value` has no
> such variants, so there is nothing to bind. And outside the numeric
> family of `[spec:pgorm:req:exec.cursor.binding-coerce]` the inferred
> type is still ignored: every other variant is written in its own binary
> format whatever Postgres inferred, so a `String` bound against a `bytea`
> placeholder is reported by the server, not the client.
>
> The standing example of that gap used to be `bits_tests`, where saving
> an integer into a `BIT(n)` column made Postgres infer `bit` for a
> parameter the driver wrote as an `int8` (`22P03`). It is no longer one:
> `[spec:pgorm:req:sql.render.cast-param-type]` pins a cast operand's
> placeholder to the type the value is actually written as, and the test
> runs unignored.

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

> [spec:pgorm:sem:exec.paginator.raw+2]
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
> `SELECT * FROM (<statement>) AS "sub_statement"` — a custom expression
> (`Expr::cust_with_values` when bind values are present, `Expr::cust`
> otherwise) inside a fresh `SelectStatement`. Wrapping rather than
> splicing means `LIMIT` and `OFFSET` land outside the caller's own
> clauses instead of colliding with them, so a raw statement that already
> carries `ORDER BY` or `LIMIT` still pages correctly; PostgreSQL will
> not reorder rows a subquery sorted, so the caller's `ORDER BY` still
> governs page boundaries.
>
> Everything else — text the grammar rejects, a `;`-separated script, an
> `INSERT`/`UPDATE`/`DELETE`/DDL statement, a `SELECT ... INTO` — is
> neither a panic nor mangled SQL sent to the server. `paginate` records
> the reason, and every reader that can report it (`fetch_page`, and
> `num_items` with the page counts derived from it) returns it as a
> `Error::Query` naming what the statement actually parsed as, using
> PostgreSQL's own node name for anything it has no SQL keyword for.
