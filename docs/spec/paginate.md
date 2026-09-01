# Pagination

pgorm offers two pagination mechanisms: keyset ("cursor") pagination over
ordered columns (`src/executor/cursor.rs`) and classic LIMIT/OFFSET page
pagination (`src/executor/paginator.rs`). The cursor module also defines
`ValueHolder`, the `ToSql` adapter used by every executor path to bind
`pgorm_query::Value` parameters. These rules capture current behavior,
including panicking gaps in parameter binding.

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

> [spec:pgorm:def:exec.cursor.binding]
> `ValueHolder` (cursor.rs) is a public newtype over `pgorm_query::Value`
> implementing `tokio_postgres::types::ToSql`; every executor path
> (select, insert, update, delete, cursor, paginator) wraps built
> statement values in it for parameter binding. It delegates per variant:
> `Bool`/`TinyInt`/`SmallInt`/`Int`/`BigInt`/`Float`/`Double` to the
> corresponding primitive impls; `Unsigned` (u32) to tokio-postgres's
> `u32` impl (which targets `OID`); `BigUnsigned` (u64) is cast to `i64`;
> `Char` is stringified; `String`, `Bytes`, `Json`, the chrono
> date/time variants, `Uuid`, and `Decimal` bind their payload, with
> `None` payloads emitted as SQL `NULL` (`IsNull::Yes`); `Array`
> recursively wraps its elements in `ValueHolder` (a `None` array is
> `NULL`). `accepts` returns `true` for every Postgres type, so type
> mismatches are not caught client-side; they surface as errors from
> Postgres at execution time.

> [spec:pgorm:req:exec.cursor.binding-gaps]
> Some `Value` variants cannot be bound and panic instead of erroring;
> callers MUST NOT pass them as parameters. `Value::TinyUnsigned` (u8)
> and `Value::SmallUnsigned` (u16) hit `unimplemented!` ("u8 not
> supported" / "u16 not supported"), and `Value::Vector`,
> `Value::IpNetwork`, and `Value::MacAddress` hit `todo!()` in
> `ValueHolder`'s `ToSql` implementation. The time-crate date/time
> variants have no binding arm (commented out). These are known
> limitations of the current `ToSql` implementation.

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
