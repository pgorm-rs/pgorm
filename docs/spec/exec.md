# Query execution

The executor layer (`src/executor/`) turns built statements into database
round-trips and decodes `tokio_postgres` rows back into Rust values. It is
split into a decoding surface (`QueryResult` and the `TryGetable` family,
`src/executor/query.rs`) and a CRUD execution surface (`Selector`,
`src/executor/{select,insert,update,delete,execute}.rs`).
These rules capture what the code does today, including known gaps.

## Decoding (`exec.decode`)

> [spec:pgorm:def:exec.decode+2]
> `QueryResult` is a `#[repr(transparent)]` wrapper around a single
> `tokio_postgres::Row`. Values are extracted through the `TryGetable`
> trait, which has three entry points: `try_get_by` (any
> `tokio_postgres::row::RowIndex`, i.e. a column name or ordinal),
> `try_get` (a prefix plus column name, concatenated as `{pre}{col}` with
> no separator; an empty prefix uses the bare column name), and
> `try_get_by_index` (ordinal position in the select list). Its fourth
> method extracts nothing: `accepts` asks which PostgreSQL types the
> implementing type can decode, for use before a row exists
> (`exec.verify.accepts`).
>
> `QueryResult` re-exposes these as `try_get_by`, `try_get`,
> `try_get_by_index`, `try_get_many`, and `try_get_many_by_index`, each
> converting the internal `TryGetError` into `Error`. `column_names`
> returns the result set's column names in order.

> [spec:pgorm:sem:exec.decode.null+1]
> Decoding delegates to `Row::try_get`. A resulting
> `tokio_postgres::Error` is classified by `TryGetError::postgres`: when
> the error's `source()` downcasts to `tokio_postgres::types::WasNull`
> it becomes `TryGetError::Null(...)`; any other error (including one with
> no source at all) becomes `TryGetError::Db(Error::Postgres(...))`.
>
> The blanket `TryGetable for Option<T>` impl converts
> `TryGetError::Null` into `Ok(None)` and propagates every other error,
> so SQL `NULL` is only an error when decoding into a non-`Option` type.
> `From<TryGetError> for Error` renders the null case as
> `Error::Type("A null value was encountered while decoding {s}")`.

> [spec:pgorm:sem:exec.decode.null-context+1]
> Null-decode errors do not carry structured column context. The payload
> of `TryGetError::Null` is the `Display` output of the underlying
> `tokio_postgres::Error` (e.g. "error deserializing column 0"), not the
> requested column name or prefix. The internal helper
> `err_null_idx_col` (query.rs:494) that would format the index is dead
> code (`#[allow(dead_code)]`) and returns the literal string `"TODO"`;
> the index formatting is commented out. This is a known limitation of
> the current behavior, not a contract to preserve. The absent-row test of
> `exec.decode.absent` therefore reads nullness off the row itself rather
> than off this payload.

> [spec:pgorm:req:exec.decode.absent]
> A `LEFT JOIN` that matched nothing still produces a row, one whose
> right-hand columns are all `NULL`. The joined decode reads its related
> side through `FromQueryResult::from_query_result_optional`, which MUST
> tell that unmatched row apart from a related row that is present and
> fails to decode: a decode failure MUST NOT be reported as a missing row.
>
> The witness is the set of columns the target reads under the prefix
> `pre`:
>
> - When `expected_columns` reports them
>   (`entity.traits.from-query-result`), the row is absent only if every
>   reported column — looked up as `{pre}{name}`, exactly as
>   `TryGetable::try_get` looks it up — is a column of the result set
>   *and* holds SQL `NULL`. A reported column the statement does not
>   return leaves the witness incomplete; that is a projection mistake
>   rather than an absent row, so the decode error MUST propagate.
> - A target that reports no columns (a hand-written implementation) is
>   judged against the result set instead: absent only if the row carries
>   at least one column whose name starts with `pre` *and* every such
>   column is `NULL`. An empty `pre` names every column of the row.
>
> Only that shape is `Ok(None)`. Every other decode failure propagates —
> a column whose PostgreSQL type the field cannot decode, an enum label
> with no Rust variant, a malformed JSON payload, or a `NULL` read into a
> non-`Option` field of a row whose remaining witness columns are not
> `NULL`.
>
> The nullness test is `QueryResult::all_null(pre, cols)` and
> `QueryResult::all_null_under(pre)`. Both read each column as
> `Option<AnyValue>`, where `AnyValue` is a private `FromSql` target that
> accepts every PostgreSQL type and reads none of the bytes, so a column
> holding a value the caller could not decode still counts as present.
> `all_null` answers `false` for an empty column list and for a column the
> result set does not carry; `all_null_under` answers `false` when no
> column carries the prefix.
>
> Two shapes are indistinguishable in the result set and are both read as
> absent: an unmatched outer join, and a matched row whose every witness
> column is genuinely `NULL`. An entity's primary key is `NOT NULL` and
> every entity model reads it, so the second arises only for a projection
> that leaves the key out.

> [spec:pgorm:def:exec.decode.types+2]
> The scalar Rust types implementing `TryGetable` by direct delegation to
> `Row::try_get` (and therefore accepting exactly the Postgres types
> tokio-postgres's `FromSql` accepts for them) are: `bool`, `i8`, `i16`,
> `i32`, `i64`, `f32`, `f64`, `String`, `Vec<u8>`, and
> `rust_decimal::Decimal`.
>
> `Decimal` is not feature-gated, and `rust_decimal` is a required
> dependency of `pgorm` rather than an optional one, because
> `[spec:pgorm:def:exec.cursor.binding+4]`'s bind path is always compiled
> and needs `Decimal`'s `ToSql` in every configuration — both for
> `Value::Decimal`, an unconditional variant of the value model
> (`[spec:pgorm:def:sql.value+2]`), and for the `numeric` arm of
> `[spec:pgorm:req:exec.cursor.binding-coerce+2]`, which is how *every*
> integer and float reaches a `numeric` placeholder. `rust_decimal` is
> compiled in any case, `pgorm-query` depending on it unconditionally; only
> this edge turns on the `db-tokio-postgres` feature that carries the impl.
> A `with-rust_decimal` feature would therefore gate nothing it claimed to,
> and turning it off did not drop a dependency — it only failed to build.
>
> Feature-gated additions: `serde_json::Value` (`with-json`);
> `chrono::NaiveDate`, `NaiveTime`, `NaiveDateTime`,
> `DateTime<FixedOffset>`, `DateTime<Utc>`, `DateTime<Local>`
> (`with-chrono`). These three features are weaker than they look for the
> same reason — the underlying types are unconditional in `pgorm-query` and
> their `FromSql`/`ToSql` impls come from `postgres-types` through
> tokio-postgres's own always-on features — so what they gate is the
> conversion surface, not the dependency. They still build with the feature
> off, which is why they remain.
>
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

> [spec:pgorm:def:exec.decode.json+1]
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
> `Error::Json("Value is not an Array")` for any non-array value.

> [spec:pgorm:def:exec.decode.from-u64+2]
> `TryFromU64` converts a `u64` (e.g. a rows-affected-derived id) into a
> primary-key value type. Numeric impls (`i8`, `i16`, `i32`, `i64`, `u8`,
> `u16`, `u32`, `u64`) use checked `TryInto`, failing with
> `Error::Conversion` on overflow. `String` converts via `to_string`.
> Every other implementor — `bool`, `f32`, `f64`, `Vec<u8>`,
> `serde_json::Value`, the chrono types, `Decimal`,
> `uuid::Uuid`, and tuples of arity 2 through 12 — unconditionally
> returns `Error::ConvertFromU64`.

## Statement verification (`exec.verify`)

> [spec:pgorm:def:exec.verify]
> `VerifyStatement::verify::<M>(sql)` (`src/executor/verify.rs`) prepares
> `sql` on the connection and checks the result columns PostgreSQL
> describes against the columns `M` reports. It is implemented for
> `DatabaseConnection` and `DatabaseTransaction`, and by a blanket impl for
> every reference to an implementor; `M` is any `FromQueryResult + 'static`
> type, the `'static` bound being what naming the target in an error costs.
> The metric wrappers (`metric.layer`) do not implement it — verification is
> not a statement they have anything to report about — so an instrumented
> caller verifies on the `inner()` connection.
> The statement is prepared through `prepare`, not `prepare_cached`, and
> dropped, so verifying neither reuses nor seeds a cached statement.
>
> `FromQueryResult::expected_columns` answers `Option<Vec<ExpectedColumn>>`
> — one entry per column the type reads, in read order. An
> `ExpectedColumn` carries the column `name`, the `rust_type` spelling of
> the field decoding it, and that type's acceptance function
> (`exec.verify.accepts`). Every reported name MUST appear among the
> statement's result columns, and each matched column's PostgreSQL type
> MUST satisfy `accepts`. The first failure of either test is returned and
> the remaining columns are not examined. Names are matched bare — the
> empty prefix `find_by_statement` decodes with — against the first result
> column of that name, which is the one `Row::try_get` would read.
>
> Nothing is executed, so the check costs one prepare round trip and no
> rows: it answers for a statement whose result set is empty exactly as it
> does for one with data, which is the point. A statement the server
> refuses to prepare at all fails as `Error::Postgres` before any column is
> compared.

> [spec:pgorm:sem:exec.verify.accepts]
> `TryGetable::accepts(ty)` answers whether a column of PostgreSQL type
> `ty` can be decoded into the implementing Rust type. Every built-in impl
> delegates to the `FromSql::accepts` of the type its decode actually
> reads, so acceptance and decoding cannot disagree: the
> `exec.decode.types` scalars and the `exec.decode.array` vectors delegate
> to themselves, the uuid format wrappers to `uuid::Uuid`, `u32` to `Oid`
> (`exec.decode.u32-oid`), `IpNetwork` and `MacAddress` to the private
> `InetSql` and `MacAddrSql` newtypes, and the `TryGetableFromJson` blanket
> impl accepts `JSON` and `JSONB`. `Option<T>` delegates to `T`: a nullable
> field accepts exactly what its payload accepts, and nullability plays no
> part (`exec.verify.limits`).
>
> The trait's default answers `true`. An implementation that does not
> override it — a `DeriveActiveEnum` mapping, a `DeriveValueType` newtype,
> the `TryGetableArray`-backed `Vec<T>`, or any hand-written impl —
> therefore accepts every column type, and verification reports nothing
> about that field rather than guessing at a decode path it cannot see.

> [spec:pgorm:req:exec.verify.errors]
> A failed verification MUST name what to change. `Error::Verify` wraps
> `VerifyError`, whose variants are distinct rather than one message
> shape: `ColumnMissing` carries the target, the column it reads, and the
> columns the statement does return; `ColumnType` carries the target, the
> column, the Rust type it decodes into, and the PostgreSQL type the
> statement returns for it; `Unreflected` carries the target
> (`exec.verify.manual`). The target is `std::any::type_name::<M>()`.
> `VerifyError` is `#[non_exhaustive]`, and verification returns these
> rather than panicking, per `[dec:pgorm:no-panic]`.

> [spec:pgorm:req:exec.verify.manual]
> `FromQueryResult::expected_columns` defaults to `None`, so a hand-written
> impl reports no columns. Verifying such a target MUST answer
> `VerifyError::Unreflected` rather than `Ok(())`: a decode pgorm cannot
> see into is unverifiable, and answering `Ok` would claim more than the
> evidence supports. A hand-written impl opts back in by overriding
> `expected_columns`. Both derives generate it
> (`macros.derive.from-query-result`, `macros.derive.model`), so derived
> targets — plain structs, entity `Model`s, and `DerivePartialModel`
> structs, which derive `FromQueryResult` alongside it — are verifiable
> without further work.

> [spec:pgorm:req:exec.verify.limits+1]
> Verification checks column names and type acceptance, and MUST NOT be
> read as checking more.
>
> Nullability is out. PostgreSQL's describe answer carries no NOT NULL
> flag for a result column, so a `T` field is not reported for a column
> that can be NULL, nor an `Option<T>` field for one that cannot; a NULL
> arriving in a non-`Option` field remains `exec.decode.null`'s error at
> decode time. Parameters are out: `verify` binds no values, and a
> parameter-count mismatch already fails at Bind on every execution rather
> than only once rows exist. Row counts and query semantics are out.
>
> Columns the statement returns but the target does not read are not an
> error: reading a subset is a legitimate projection. Prefixed decoding is
> out — only the empty prefix is checked, so the `s{i}_` prefixes a source
> graph decodes under (`[spec:pgorm:sem:query.graph.decode+1]`) and the tuple
> targets of `TryGetableMany` (`exec.decode.many`), which are not
> `FromQueryResult` types at all, have no verification path. A pass is
> therefore not a proof
> that decoding will succeed; it closes the specific hole where an empty
> result set hides a target that names a column the statement does not
> return, or reads one into a type that cannot decode it.

## CRUD execution (`exec.crud`)

> [spec:pgorm:def:exec.crud+1]
> Statement execution is mediated by selector types: `Selector<S>` holds
> a `SelectStatement` plus a `SelectorTrait` implementor, and
> `SelectorRaw<S>` holds a raw SQL string plus `Values`. `SelectorTrait`
> has a single method, `from_raw_query_result`, turning one `QueryResult`
> into an item. Implementors: `SelectModel<M>` (one `FromQueryResult`
> model, empty prefix), `SelectGetableValue<T, C>` (tuple by named columns
> from the `C` enum), `SelectGetableTuple<T>` (tuple by ordinal), and
> `GraphRow<E, S>` (a source graph's declared tuple, one prefix per decoded
> source — `[spec:pgorm:sem:query.graph.decode+1]`).
>
> Conversions from a builder: `Select::into_model`, `into_partial_model`,
> `into_values`, `into_tuple`, `from_raw_sql`; the constructors that take a
> statement or a string directly are enumerated by
> `exec.crud.selector-entry`. Executing a `Selector` first builds SQL
> with the Postgres `QueryBuilder`, then binds each `Value` through the
> `ValueHolder` `ToSql` adapter (see `exec.cursor.binding`).

> [spec:pgorm:sem:exec.crud.selector-entry+1]
> A caller who has built a `SelectStatement` outside the entity builders —
> a CTE used as the driving table has no entity behind it at all — MUST
> still be able to name a decode target without collapsing the statement to
> a string. `Selector` therefore has one constructor per decode shape:
> `from_select::<M>(SelectStatement)` for a `FromQueryResult` type,
> `into_tuple::<T>(SelectStatement)` for an ordinal tuple, and
> `with_columns::<T, C>(SelectStatement)` for a tuple named by an `Iden`
> enum. `SelectorRaw` mirrors the last two over `(String, Values)`:
> `from_statement::<M>`, `into_tuple::<T>` and `with_columns::<T, C>`.
>
> Those constructors name the row type twice — `Selector<S>`'s `S` is not
> constrained by the call, so the target appears once to pick the selector
> type and once as the constructor's own parameter, before the statement is
> mentioned at all. The statement-first spelling is therefore the trait pair
> `DecodeSelect` and `DecodeRaw`, whose `into_model::<M>`, `into_tuple::<T>`
> and `into_values::<T, C>` name it once and read in the order the rest of
> the builder does. `DecodeSelect` is implemented for `SelectStatement`;
> `DecodeRaw` for `(S, Values)` where `S: Into<String>`, which is what
> `SelectStatement::build`, `Pipeline::into_sql` and the `prql!` macro all
> hand back, so a compiled query flows into a decode without being taken
> apart first. They delegate to the constructors and add no behaviour of
> their own; both spellings remain, and both are in `entity::prelude`.
>
> The `Selector` constructors yield an ordinary `Selector`, so what they
> build is not a lesser statement: `one` still injects `LIMIT 1`
> (`exec.crud.select`), the empty-projection guard still runs before
> anything reaches the server (`query.build.modifiers`), and `all`, `stream`
> and the paginator are the same code as for an entity query. The
> `SelectorRaw` constructors carry the raw statement's semantics unchanged —
> no injected limit, and no projection guard, since a caller-supplied string
> has no projection list to inspect.

> [spec:pgorm:sem:exec.crud.select+3]
> `Selector::one` and `one_opt` set `LIMIT 1` on the query, then execute
> through the connection's `query_opt`. The limit is set on the statement
> the selector holds, which for a CTE query is the carrying select and never
> a CTE body (`query.build.with.attach`). `SelectorRaw::one` and `one_opt`
> execute the raw statement as-is (no limit is injected), so a raw statement
> that returns more than one row is a `query_opt` protocol error rather than
> a first row.
>
> `one` returns the decoded item and fails with `Error::RecordNotFound`
> when zero rows are returned; `one_opt` returns `Ok(None)` in that
> case. This is a deliberate pgorm difference from SeaORM, where `one`
> returns an `Option`. `all` executes via `query_all` and decodes every
> row through `from_raw_query_result`, aborting on the first decode
> error. `Select::one`/`one_opt`/`all` delegate through `into_model`, and a
> source graph's terminals through its own selector
> (`[spec:pgorm:sem:query.graph.terminals+1]`).

> [spec:pgorm:req:exec.crud.exec-vocabulary]
> A CRUD terminal's name MUST determine the shape of what it returns, so
> that a reader of the call site needs no knowledge of which constructor
> produced the builder. Exactly three terminal names exist, and each MUST
> mean the same thing on every builder that offers it:
>
> - `exec` MUST emit no `RETURNING` clause and MUST return the rows-affected
>   count as a bare `u64`.
> - `exec_returning_pk` MUST return the inserted row's primary key, typed as
>   the entity's `PrimaryKey::ValueType`.
> - `exec_returning_model` MUST return exactly one `Model`;
>   `exec_returning_models` MUST return `Vec<Model>`.
>
> The mapping is therefore total:
>
> | Builder | `exec` | `exec_returning_pk` | model-returning |
> | --- | --- | --- | --- |
> | `Insert<A>` | `u64` | `PrimaryKey::ValueType` | `exec_returning_model` → `Model` |
> | `TryInsert<A>` | `TryInsertResult<u64>` | `TryInsertResult<ValueType>` | `exec_returning_model` → `TryInsertResult<Model>` |
> | `UpdateOne<A>` | *absent* | *absent* | `exec_returning_model` → `Model` |
> | `UpdateMany<E>` | `u64` | *absent* | `exec_returning_models` → `Vec<Model>` |
> | `DeleteOne<A>` | `u64` | *absent* | *absent* |
> | `DeleteMany<E>` | `u64` | *absent* | *absent* |
>
> `TryInsert`'s wrapper is the receiver type's contract, not a per-method
> variation: every `TryInsert` terminal MUST wrap the corresponding `Insert`
> terminal's shape in `TryInsertResult`.
>
> `UpdateOne` MUST NOT offer a bare `exec`. Updating one model by primary key
> always reads the row back, so there is no count-shaped answer to give; a
> caller wanting a count uses `Update::many` filtered to the key. The
> absence is the point: no `exec` on `UpdateOne` can be misread as a count.
>
> The retired spellings MUST NOT exist. `exec_without_returning` is `exec`;
> `exec_with_returning` is `exec_returning_model` (or `exec_returning_models`
> on `UpdateMany`); the old `Insert::exec`, which returned a primary key
> under a name that promised nothing, is `exec_returning_pk`.

> [spec:pgorm:sem:exec.crud.insert+4]
> `Insert::exec_returning_pk` appends a `RETURNING` clause of the entity's
> primary-key columns and resolves the key (typed as the entity's
> `PrimaryKey::ValueType`) from that clause and from nothing else. There is
> exactly one mode: the statement runs through `query_all`, the **last**
> returned row's primary-key columns are read by name, an empty result fails
> with `Error::RecordNotInserted`, and a decode failure of the key columns
> fails with `Error::UnpackInsertId`. This holds whether or not the caller
> supplied the key: an entity whose key is not auto-increment MUST be answered
> from the row the database wrote, exactly as an auto-increment one is.
>
> The client-supplied-key mode is deleted and MUST NOT return. It ran the
> statement through `execute`, discarded the `RETURNING` rows it had already
> asked for, and rebuilt the key from a `ValueTuple` the builder had cached —
> so the answer was the key the caller *asked* to write rather than the key of
> the row that got written. Under `ON CONFLICT DO UPDATE` those differ: an
> insert of `id = 42` that conflicts on some other unique column updates the
> existing row and reports `42`, a primary key naming no row. Reading
> `RETURNING` reports the conflict row's own key. With the cache gone, a
> mistyped `PrimaryKey::ValueType` on an insert surfaces as the
> `Error::UnpackInsertId` of a failed column decode rather than as the
> `Error::Type` the tuple reconstruction raised; `sql.value.tuple` still
> guards `exec.crud.update`'s no-op re-fetch, which does rebuild a key from a
> tuple.
>
> The key is returned bare. There is no `InsertResult` wrapper and no
> `last_insert_id` field: a one-field struct whose field repeated the
> method's promise carried no information the name does not, and
> "last insert id" named a MySQL affordance rather than the `RETURNING`ed
> primary key this actually is.

> [spec:pgorm:sem:exec.crud.insert-returning+2]
> `Insert::exec_returning_model` appends a `RETURNING` clause of **all**
> entity columns and decodes the inserted model through
> `SelectorRaw::<SelectModel<Model>>::one_opt`; when no row comes back it
> fails with `Error::RecordNotFound`. `Insert::exec`
> appends no `RETURNING` clause and returns the rows-affected count as
> `u64`.

> [spec:pgorm:sem:exec.crud.try-insert+3]
> `TryInsert::exec`, `exec_returning_pk`, and `exec_returning_model`
> wrap the corresponding `Insert` executions in `TryInsertResult`. When
> the underlying insert statement has no columns (e.g. `Insert::many`
> over an empty iterator), they return `TryInsertResult::Empty` without
> touching the database — the failsafe for empty batch inserts.
>
> All three otherwise report an insert that the conflict clause skipped
> as `TryInsertResult::Conflicted`, each reading the signal its own
> execution yields for "no row was written". `exec_returning_pk` maps a
> `Error::RecordNotInserted`, which `exec.crud.insert` raises only when
> the primary-key `RETURNING` came back empty. `exec`
> maps a zero rows-affected count and `exec_returning_model` maps a
> missing `RETURNING` row — both only when the statement carries an
> `ON CONFLICT` clause, since neither signal can otherwise be attributed
> to a conflict. Without such a clause those two keep the plain `Insert`
> outcome, `TryInsertResult::Inserted(0)` and `Error::RecordNotFound`
> respectively. Success becomes `TryInsertResult::Inserted(..)`; every
> other error propagates.

> [spec:pgorm:sem:exec.crud.update+5]
> `UpdateMany::exec` short-circuits when the update statement carries no SET
> values, returning `0` without a database round-trip; otherwise it
> executes and returns the rows-affected count as `u64`.
>
> An update whose `WHERE` matches nothing is `Ok(0)`, never an error. There
> is no `Updater` and no `check_record_exists`: the count is the whole
> answer and the caller decides what zero means. `Error::RecordNotUpdated`
> is deleted with them — it had no other producer, and an error variant no
> code path can raise is a promise the error model cannot keep.
>
> `UpdateOne::exec_returning_model` returns the updated model: it appends a
> `RETURNING` clause of all entity columns and decodes through
> `SelectorRaw::one`, so an update matching zero rows surfaces the
> `Error::RecordNotFound` of `exec.crud.select`. On the no-op path (nothing
> to set) it instead re-fetches the current model by primary key. That
> re-fetch keeps a `Error::PrimaryKeyNotSet` guard for an active model with
> no primary-key value, but the guard is defensive only: `query.build.update`
> rejects an unset primary key when the `UpdateOne` is built, so no caller
> can reach the terminal with one. Rebuilding the typed key from that tuple
> goes through `sql.value.tuple`, so a shape or element-type disagreement
> with the entity's declared `ValueType` fails with `Error::Type` naming the
> table and the mismatch, rather than panicking.
> `UpdateMany::exec_returning_models` appends the
> same full-column `RETURNING` and returns `Vec<Model>` via `all`; its
> no-op path returns an empty `Vec`.

> [spec:pgorm:sem:exec.crud.delete+1]
> `DeleteOne::exec` and `DeleteMany::exec` both build
> the `DeleteStatement`, bind values through `ValueHolder`, execute, and
> return the rows-affected count as `u64`. There is no existence check:
> deleting zero rows is `Ok(0)`, never an error.
>
> There is no `Deleter` and no `DeleteResult`. The intermediate builder had
> no caller and no capability the two entry points lack, and the result
> struct was a one-field wrapper over the count that `exec` now returns
> directly — the same `u64` every other `exec` yields, per
> `exec.crud.exec-vocabulary`.

> [spec:pgorm:def:exec.crud.exec-result+1]
> `ExecResult` is a `#[repr(transparent)]` wrapper over a `u64`
> rows-affected count (`ExecResultHolder`), exposed via
> `rows_affected()`. It carries no primary key; inserted keys are
> obtained through `RETURNING` per `exec.crud.insert`.

## Streaming (`exec.stream`)

> [spec:pgorm:def:exec.stream+2]
> Row-level streaming is exposed through `ConnectionTrait::query_raw`, which
> takes the same `BorrowToSql` `ExactSizeIterator` params as `execute_raw`
> and returns a `tokio_postgres::RowStream`: rows are read from the
> connection as they arrive instead of being buffered into a `Vec` the way
> `query_all` does. It is implemented by `DatabaseConnection`,
> `&DatabaseConnection`, `DatabaseTransaction`, `InstrumentedConnection`,
> and `InstrumentedTransaction`.
>
> On top of it the executor exposes `stream` on `SelectorRaw`, `Selector`,
> `Select`, and `SelectGraph`, plus `stream_partial_model` on `Select`,
> each returning `PinBoxSendStream<'db, Result<Item, Error>>` — an alias for
> `Pin<Box<dyn Stream<Item = ..> + Send + 'db>>`. Unlike `PinBoxStream`
> (used by `Paginator::into_stream`) it is `Send`, so a streamed select can
> be consumed from a spawned task. The graph's grouped read has no streamed
> form: its output requires all rows of a root before any entry is complete
> (`[spec:pgorm:sem:query.graph.grouped+1]`). Page-batched and keyset-windowed
> consumption remain available through `exec.paginator` and `exec.cursor`.

> [spec:pgorm:sem:exec.stream.decode+1]
> `SelectorRaw::stream` binds `Values` through the `ValueHolder` `ToSql`
> adapter exactly as `all` does, then maps the `RowStream` item-wise: each
> `Ok(row)` is decoded by `S::from_raw_query_result`, and each transport
> error becomes `Error::Postgres`. Decoding is lazy and per-item — no row is
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
> compiles, and the stream then yields `Error::Postgres` for a closed
> connection.
>
> The metric layer records `query_raw` at stream *creation*, timing only the
> round-trip that produced the stream and reporting `rows: None` — the row
> count is not knowable up front, and `MetricsCollector`'s hooks are `async`
> so they cannot be invoked from the stream's `Drop`.
