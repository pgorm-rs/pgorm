# Pagination

pgorm offers two pagination mechanisms: keyset ("cursor") pagination over
ordered columns (`src/executor/cursor.rs`) and classic LIMIT/OFFSET page
pagination (`src/executor/paginator.rs`). The cursor module also defines
`ValueHolder`, the `ToSql` adapter used by every executor path to bind
`pgorm_query::Value` parameters. These rules capture current behavior,
including the remaining gaps in parameter binding.

## Cursor pagination (`exec.cursor`)

> [spec:pgorm:def:exec.cursor]
> `Cursor<S>` wraps a `SelectStatement` plus the target table, an
> `Identity` of one or more order columns, optional `first`/`last` row
> limits, optional `before`/`after` boundary `ValueTuple`s, a `sort_asc`
> flag (default ascending), and a list of secondary order columns.
> Cursors are created via `Select::cursor_by` (order columns on the
> entity's table) and, for joined selects, `SelectTwo::cursor_by` /
> `cursor_by_other` (order columns on the first or second entity
> respectively). `CursorTrait` names the `SelectorTrait` used to decode
> rows; `into_model` and `into_partial_model` re-target the decoded type.
> `Cursor` also implements `QuerySelect` and `QueryOrder` for further
> query modification.

> [spec:pgorm:sem:exec.cursor.keyset]
> `after(values)` filters to rows strictly beyond the boundary in the
> logical sort direction: column `>` value when ascending, `<` when
> descending. `before(values)` is the mirror image (`<` ascending, `>`
> descending). For a composite order key of n columns, the boundary
> expands to the row-value emulation
> `(c1 = v1 AND ... AND cn ⋈ vn) OR (c1 = v1 AND ... AND c(n-1) ⋈ v(n-1)) OR ... OR (c1 ⋈ v1)`
> where `⋈` is the direction comparison — specialized forms for unary,
> binary, and ternary identities and a generic fold for `Identity::Many`.
> Conditions are added to the query's `WHERE` via `cond_where`; both
> `before` and `after` may be set simultaneously.
>
> The arity of the boundary tuple must match the arity of the order
> columns; any mismatch (including differing lengths for
> `Identity::Many`) panics with "column arity mismatch".

> [spec:pgorm:sem:exec.cursor.window]
> `first(N)` and `last(N)` are mutually exclusive: each clears the other,
> so the most recent call wins. Either applies `LIMIT N`. `last` fetches
> the window from the far end by flipping the emitted SQL sort order:
> the SQL order is ascending iff `sort_asc` XNOR no-`last` (i.e. `asc` +
> `first` or `desc` + `last` emit `ASC`; the other two combinations emit
> `DESC`). When `last` was used, the fetched buffer is reversed in
> memory after decoding, so `all` always returns rows in the cursor's
> logical (`asc`/`desc`) order regardless of windowing direction.

> [spec:pgorm:sem:exec.cursor.order]
> `Cursor::all` composes the query by applying the limit, then the order
> clause, then the boundary filters, before building and executing via
> `query_all` and decoding each row with the selector. Ordering first
> clears any pre-existing `ORDER BY` on the query, then orders by each
> order column (qualified with the cursor's table) in declared order,
> then by each secondary order entry — all using the single resolved
> direction of `exec.cursor.window`. Only `Identity::Unary` secondary
> entries are applied; composite secondary identities are silently
> ignored. `SelectTwo::cursor_by` automatically installs the other
> entity's primary-key columns as secondary order entries (and
> `cursor_by_other` installs the first entity's), giving joined cursors
> a deterministic tiebreak.

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

> [spec:pgorm:def:exec.paginator]
> `Paginator<'db, C, S>` holds a `SelectStatement`, a zero-based current
> `page`, a `page_size`, a borrowed connection, and a phantom selector.
> `ItemsAndPagesNumber` carries `number_of_items` and `number_of_pages`.
> `PaginatorTrait::paginate(db, page_size)` constructs a paginator and is
> implemented for `Selector<S>`, `SelectorRaw<S>`, `Select<E>`, and
> `SelectTwo<E, F>` (the latter two via `into_model`). The trait also
> provides `count`, defined as `paginate(db, 1).num_items()`.
> `PinBoxStream` is the pinned boxed stream alias returned by
> `into_stream`.

> [spec:pgorm:req:exec.paginator.page-size]
> `page_size` MUST be non-zero. Both the `Selector` and `SelectorRaw`
> `paginate` implementations assert this and panic with "page_size
> should not be zero"; a zero page size is never a recoverable `DbErr`.

> [spec:pgorm:sem:exec.paginator.fetch]
> `fetch_page(page)` executes a clone of the query with
> `LIMIT page_size OFFSET page_size * page`; pages are zero-indexed and
> the paginator's own cursor is not consulted or advanced. Rows are
> decoded through the selector, aborting on the first decode error.
> `fetch()` is `fetch_page(cur_page())`; `next()` increments the page
> counter without fetching; `cur_page()` reports it, starting at 0.

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

> [spec:pgorm:sem:exec.paginator.raw]
> Paginating a `SelectorRaw` rewrites the raw statement so limit and
> offset can be appended: the SQL is trimmed and its first six
> characters (the `SELECT` keyword) sliced off, and the remainder is
> embedded as a custom expression (`Expr::cust_with_values` when bind
> values are present, `Expr::cust` otherwise) inside a fresh
> `SelectStatement`. Consequently only raw statements that begin with a
> `SELECT` keyword (after leading whitespace) survive this
> transformation; no validation is performed on the sliced prefix.
