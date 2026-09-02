# Query execution

The executor layer (`src/executor/`) turns built statements into database
round-trips and decodes `tokio_postgres` rows back into Rust values. It is
split into a decoding surface (`QueryResult` and the `TryGetable` family,
`src/executor/query.rs`) and a CRUD execution surface (`Selector`,
`Inserter`, `Updater`, `Deleter`, `src/executor/{select,insert,update,delete,execute}.rs`).
These rules capture what the code does today, including known gaps.

## Decoding (`exec.decode`)

> [spec:pgorm:def:exec.decode]
> `QueryResult` is a `#[repr(transparent)]` wrapper around a single
> `tokio_postgres::Row`. Values are extracted through the `TryGetable`
> trait, which has three entry points: `try_get_by` (any
> `tokio_postgres::row::RowIndex`, i.e. a column name or ordinal),
> `try_get` (a prefix plus column name, concatenated as `{pre}{col}` with
> no separator; an empty prefix uses the bare column name), and
> `try_get_by_index` (ordinal position in the select list).
>
> `QueryResult` re-exposes these as `try_get_by`, `try_get`,
> `try_get_by_index`, `try_get_many`, and `try_get_many_by_index`, each
> converting the internal `TryGetError` into `DbErr`. `column_names`
> returns the result set's column names in order.

> [spec:pgorm:sem:exec.decode.null]
> Decoding delegates to `Row::try_get`. A resulting
> `tokio_postgres::Error` is classified by `TryGetError::postgres`: when
> the error's `source()` downcasts to `tokio_postgres::types::WasNull`
> it becomes `TryGetError::Null(...)`; any other error (including one with
> no source at all) becomes `TryGetError::DbErr(DbErr::Postgres(...))`.
>
> The blanket `TryGetable for Option<T>` impl converts
> `TryGetError::Null` into `Ok(None)` and propagates every other error,
> so SQL `NULL` is only an error when decoding into a non-`Option` type.
> `From<TryGetError> for DbErr` renders the null case as
> `DbErr::Type("A null value was encountered while decoding {s}")`.

> [spec:pgorm:sem:exec.decode.null-context]
> Null-decode errors do not carry structured column context. The payload
> of `TryGetError::Null` is the `Display` output of the underlying
> `tokio_postgres::Error` (e.g. "error deserializing column 0"), not the
> requested column name or prefix. The internal helper
> `err_null_idx_col` (query.rs:304) that would format the index is dead
> code (`#[allow(dead_code)]`) and returns the literal string `"TODO"`;
> the index formatting is commented out. This is a known limitation of
> the current behavior, not a contract to preserve.

> [spec:pgorm:def:exec.decode.types+1]
> The scalar Rust types implementing `TryGetable` by direct delegation to
> `Row::try_get` (and therefore accepting exactly the Postgres types
> tokio-postgres's `FromSql` accepts for them) are: `bool`, `i8`, `i16`,
> `i32`, `i64`, `f32`, `f64`, `String`, and `Vec<u8>`.
>
> Feature-gated additions: `serde_json::Value` (`with-json`);
> `chrono::NaiveDate`, `NaiveTime`, `NaiveDateTime`,
> `DateTime<FixedOffset>`, `DateTime<Utc>`, `DateTime<Local>`
> (`with-chrono`); `rust_decimal::Decimal` (`with-rust_decimal`).
> There is no time-crate or bigdecimal support: chrono is the only
> datetime path and `Decimal` the only arbitrary-precision numeric.
> With `with-uuid`, `uuid::Uuid` and its format wrappers
> (`uuid::fmt::Braced`, `Hyphenated`, `Simple`, `Urn`) decode by first
> reading a `uuid::Uuid` and then converting.
>
> The three Postgres-specific payload types behind `Value::Vector`,
> `Value::IpNetwork` and `Value::MacAddress` also implement `TryGetable`,
> unconditionally (they are not feature-gated). `pgvector::Vector`
> delegates straight to `Row::try_get`, using the `FromSql` impl pgvector
> provides under its `postgres` feature (enabled by `pgorm-query`), which
> accepts any type named `vector`. `ipnetwork::IpNetwork` and
> `mac_address::MacAddress` ship no `FromSql` impl and the orphan rule
> forbids writing one for them, so each decodes through a private local
> newtype — `InetSql` and `MacAddrSql` — that implements `FromSql` over
> `postgres_protocol::types::inet_from_sql` and
> `postgres_protocol::types::macaddr_from_sql` respectively and is
> unwrapped by the `TryGetable` impl. `InetSql` accepts `INET` and `CIDR`;
> `MacAddrSql` accepts `MACADDR` only, so the 8-byte `MACADDR8` is
> rejected. All three are re-exported from `pgorm_query` (and hence
> `pgorm::entity::prelude`) as `Vector`, `IpNetwork` and `MacAddress`, so
> callers need not depend on the payload crates directly.
>
> None of the three has a `Vec<T>` array impl: `exec.decode.array` does
> not cover them.

> [spec:pgorm:sem:exec.decode.u32-oid]
> `u32` implements `TryGetable` by decoding a `tokio_postgres::types::Oid`,
> so it reads Postgres `OID` columns, not `INT4`. Likewise `Vec<u32>`
> (under `postgres-array`) decodes as `Vec<Oid>`. No other unsigned
> integer widths implement `TryGetable`; `u8`, `u16`, and `u64` cannot be
> decoded from a row.

> [spec:pgorm:def:exec.decode.array+1]
> Under the `postgres-array` feature, `Vec<T>` implements `TryGetable` by
> direct array delegation for a subset of the `exec.decode.types` scalars:
> `bool`, `i8`, `i16`, `i32`, `i64`, `f32`, `f64`, `String`, plus the
> feature-gated json/chrono/decimal types and the uuid format
> wrappers, which decode as `Vec<uuid::Uuid>` then convert element-wise.
> `Vec<u32>` decodes as `Vec<Oid>` per `exec.decode.u32-oid`. The subset
> is proper: `Vec<u8>` is bytes rather than an array, and
> `pgvector::Vector`, `ipnetwork::IpNetwork` and
> `mac_address::MacAddress` have no `Vec<T>` impl at all — the array
> macros delegate to `Row::try_get` for `Vec<$type>`, which the two
> newtype-decoded types cannot satisfy without a separate unwrapping
> macro.

> [spec:pgorm:def:exec.decode.many]
> `TryGetableMany` extracts tuples from a row: `try_get_many` takes a
> prefix and a slice of column names, `try_get_many_by_index` reads
> ordinals starting at 0. It is implemented for any single
> `T: TryGetable` (using the first column), for `(T,)`, and for tuples of
> arity 2 through 12 of `TryGetable` elements. `find_by_statement`
> constructs a `SelectorRaw<SelectGetableValue<Self, C>>` from a raw SQL
> string and values, with column names supplied by iterating the `C`
> identifier enum.

> [spec:pgorm:req:exec.decode.many-arity]
> Callers of `try_get_many` MUST supply at least as many column names as
> the tuple arity: a shorter slice yields a type error of the form
> "Expect {N} column names supplied but got slice of length {len}".
> Surplus column names are ignored. The index-based path performs no such
> check; it simply reads ordinals `0..N`.

> [spec:pgorm:def:exec.decode.json]
> Under `with-json`, `TryGetableFromJson` decodes any
> `serde::Deserialize` type from a JSON/JSONB column via the
> `tokio_postgres::types::Json<T>` wrapper. A blanket impl provides
> `TryGetable` for every `TryGetableFromJson` type; consequently a type
> can implement `ActiveEnum` or `TryGetableFromJson` but not both.
>
> `TryGetableArray` (blanket-implemented for `TryGetableFromJson` types)
> provides `Vec<T>` decoding by reading the column as a
> `serde_json::Value` and running `from_json_vec`, which deserializes
> each element of a JSON array and fails with
> `DbErr::Json("Value is not an Array")` for any non-array value.

> [spec:pgorm:def:exec.decode.from-u64+1]
> `TryFromU64` converts a `u64` (e.g. a rows-affected-derived id) into a
> primary-key value type. Numeric impls (`i8`, `i16`, `i32`, `i64`, `u8`,
> `u16`, `u32`, `u64`) use checked `TryInto`, failing with
> `DbErr::TryIntoErr` on overflow. `String` converts via `to_string`.
> Every other implementor — `bool`, `f32`, `f64`, `Vec<u8>`,
> `serde_json::Value`, the chrono types, `Decimal`,
> `uuid::Uuid`, and tuples of arity 2 through 12 — unconditionally
> returns `DbErr::ConvertFromU64`.

## CRUD execution (`exec.crud`)

> [spec:pgorm:def:exec.crud]
> Statement execution is mediated by selector types: `Selector<S>` holds
> a `SelectStatement` plus a `SelectorTrait` implementor, and
> `SelectorRaw<S>` holds a raw SQL string plus `Values`. `SelectorTrait`
> has a single method, `from_raw_query_result`, turning one `QueryResult`
> into an item. Implementors: `SelectModel<M>` (one `FromQueryResult`
> model, empty prefix), `SelectTwoModel<M, N>` (decodes
> `(M, Option<N>)` using the `SelectA`/`SelectB` column prefixes),
> `SelectGetableValue<T, C>` (tuple by named columns from the `C` enum),
> and `SelectGetableTuple<T>` (tuple by ordinal).
>
> Conversions: `Select::into_model`, `into_partial_model`, `into_values`,
> `into_tuple`, `from_raw_sql`; `SelectorRaw::from_statement`,
> `with_columns`, `into_model`. Executing a `Selector` first builds SQL
> with the Postgres `QueryBuilder`, then binds each `Value` through the
> `ValueHolder` `ToSql` adapter (see `exec.cursor.binding`).

> [spec:pgorm:sem:exec.crud.select]
> `Selector::one` and `one_opt` set `LIMIT 1` on the query, then execute
> through the connection's `query_opt`. `SelectorRaw::one` and `one_opt`
> execute the raw statement as-is (no limit is injected).
>
> `one` returns the decoded item and fails with `DbErr::RecordNotFound`
> when zero rows are returned; `one_opt` returns `Ok(None)` in that
> case. This is a deliberate pgorm difference from SeaORM, where `one`
> returns an `Option`. `all` executes via `query_all` and decodes every
> row through `from_raw_query_result`, aborting on the first decode
> error. `Select::one`/`one_opt`/`all` and
> `SelectTwo::one`/`one_opt`/`all` delegate through `into_model`.

> [spec:pgorm:sem:exec.crud.consolidate]
> `SelectTwoMany::all` executes as a `SelectTwoModel` select and
> consolidates `(E::Model, Option<F::Model>)` rows into
> `(E::Model, Vec<F::Model>)`. Grouping keys on the left entity's
> primary key with arity-specialized keys (unary value, pair, or vector).
> Children are collected in row order; each distinct left key yields
> exactly one output entry; left rows with no right model produce an
> empty `Vec`. `SelectTwoMany` deliberately exposes no `one` and no
> pagination: `one()` was dropped, and `paginate`/`count` are absent
> because a page boundary could split one parent's children.

> [spec:pgorm:sem:exec.crud.insert]
> `Insert::exec` appends a `RETURNING` clause of the entity's primary-key
> columns and resolves `InsertResult::last_insert_id` (typed as the
> entity's `PrimaryKey::ValueType`) in one of two modes. When the insert
> captured a client-supplied primary-key `ValueTuple`, the statement runs
> through `execute`; zero rows affected fails with
> `DbErr::RecordNotInserted`, and `last_insert_id` is reconstructed from
> the cached tuple. Otherwise the statement runs through `query_all` and
> the **last** returned row's primary-key columns are read by name;
> an empty result fails with `DbErr::RecordNotInserted`, and a decode
> failure of the key columns fails with `DbErr::UnpackInsertId`.

> [spec:pgorm:sem:exec.crud.insert-returning]
> `Insert::exec_with_returning` appends a `RETURNING` clause of **all**
> entity columns and decodes the inserted model through
> `SelectorRaw::<SelectModel<Model>>::one_opt`; when no row comes back it
> fails with `DbErr::RecordNotFound`. `Insert::exec_without_returning`
> appends no `RETURNING` clause and returns the rows-affected count as
> `u64`.

> [spec:pgorm:sem:exec.crud.try-insert]
> `TryInsert::exec`, `exec_without_returning`, and `exec_with_returning`
> wrap the corresponding `Insert` executions in `TryInsertResult`. When
> the underlying insert statement has no columns (e.g. `insert_many`
> with an empty iterator), they return `TryInsertResult::Empty` without
> touching the database — the failsafe for empty batch inserts. A
> `DbErr::RecordNotInserted` from the inner execution becomes
> `TryInsertResult::Conflicted`; success becomes
> `TryInsertResult::Inserted(..)`; every other error propagates.

> [spec:pgorm:sem:exec.crud.update]
> `Updater::exec` short-circuits when the update statement carries no SET
> values, returning a default `UpdateResult` (zero `rows_affected`)
> without a database round-trip; otherwise it executes and returns
> `UpdateResult { rows_affected }`. With `check_record_exists` enabled,
> zero rows affected fails with `DbErr::RecordNotUpdated`.
>
> `UpdateOne::exec` returns the updated model: it appends a `RETURNING`
> clause of all entity columns and decodes through `SelectorRaw::one`, so
> an update matching zero rows surfaces the `DbErr::RecordNotFound` of
> `exec.crud.select`. On the no-op path (nothing to set) it instead
> re-fetches the current model by primary key, failing with
> `DbErr::UpdateGetPrimaryKey` when the active model has no primary-key
> value. `UpdateMany::exec_with_returning` appends the same full-column
> `RETURNING` and returns `Vec<Model>` via `all`; its no-op path returns
> an empty `Vec`.

> [spec:pgorm:sem:exec.crud.delete]
> `DeleteOne::exec`, `DeleteMany::exec`, and `Deleter::exec` all build
> the `DeleteStatement`, bind values through `ValueHolder`, execute, and
> return `DeleteResult { rows_affected }`. There is no existence check:
> deleting zero rows is `Ok` with `rows_affected: 0`, never an error.

> [spec:pgorm:def:exec.crud.exec-result]
> `ExecResult` is a `#[repr(transparent)]` wrapper over a `u64`
> rows-affected count (`ExecResultHolder`), exposed via
> `rows_affected()`. It carries no `last_insert_id`; insert ids are
> obtained through `RETURNING` per `exec.crud.insert`.

## Streaming (`exec.stream`)

> [spec:pgorm:def:exec.stream]
> Row-level streaming is exposed through `ConnectionTrait::query_raw`, which
> takes the same `BorrowToSql` `ExactSizeIterator` params as `execute_raw`
> and returns a `tokio_postgres::RowStream`: rows are read from the
> connection as they arrive instead of being buffered into a `Vec` the way
> `query_all` does. It is implemented by `DatabaseConnection`,
> `&DatabaseConnection`, `DatabaseTransaction`, `InstrumentedConnection`,
> and `InstrumentedTransaction`.
>
> On top of it the executor exposes `stream` on `SelectorRaw`, `Selector`,
> `Select`, and `SelectTwo`, plus `stream_partial_model` on `Select` and
> `SelectTwo`, each returning
> `PinBoxSendStream<'db, Result<Item, DbErr>>` — an alias for
> `Pin<Box<dyn Stream<Item = ..> + Send + 'db>>`. Unlike `PinBoxStream`
> (used by `Paginator::into_stream`) it is `Send`, so a streamed select can
> be consumed from a spawned task. `SelectTwoMany` deliberately has no
> `stream`: its output requires all rows of a parent before any entry is
> complete (see `exec.crud.consolidate`). Page-batched and keyset-windowed
> consumption remain available through `exec.paginator` and `exec.cursor`.

> [spec:pgorm:sem:exec.stream.decode]
> `SelectorRaw::stream` binds `Values` through the `ValueHolder` `ToSql`
> adapter exactly as `all` does, then maps the `RowStream` item-wise: each
> `Ok(row)` is decoded by `S::from_raw_query_result`, and each transport
> error becomes `DbErr::Postgres`. Decoding is lazy and per-item — no row is
> decoded until it is polled, and a decode failure is yielded as one `Err`
> item rather than discarding the rest of the result set, which is the
> deliberate difference from `all` (which aborts at the first bad row).
>
> The adapter is a stateless `map`: pgorm neither fuses the stream after an
> error nor cancels the in-flight query. Dropping the stream before it is
> exhausted is safe — the tokio-postgres connection task keeps paging
> through and discarding the remaining messages, so the connection stays
> usable — but the query still runs to completion server-side. A stream MUST
> NOT be polled after the connection or transaction it came from is dropped:
> `RowStream` is `'static` and does not borrow the connection, so doing so
> compiles, and the stream then yields `DbErr::Postgres` for a closed
> connection.
>
> The metric layer records `query_raw` at stream *creation*, timing only the
> round-trip that produced the stream and reporting `rows: None` — the row
> count is not knowable up front, and `MetricsCollector`'s hooks are `async`
> so they cannot be invoked from the stream's `Drop`.
