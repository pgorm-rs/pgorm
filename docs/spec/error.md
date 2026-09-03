# Error Model

`src/error.rs` defines the crate-wide error taxonomy. Every fallible public
operation returns `Result<_, Error>`; the finer-grained enums below feed into
it.

## Taxonomy

> [spec:pgorm:def:error.model+4]
> `Error` is the crate-wide error enum. Driver and pool failures convert in
> via `From`: `Postgres(tokio_postgres::Error)` (the variant every
> `ConnectionTrait` call and transaction commit produces on database failure;
> its `Display` includes the server `DbError` detail when present) and
> `Pool(pgorm_pool::PoolError)` (produced by `DatabasePool::get`; pool
> exhaustion and acquisition timeouts surface here). The remaining variants
> are constructed by pgorm itself: `Conversion { from, into, source }`,
> `Query(RuntimeError)`, `ConvertFromU64(&'static str)`, `UnpackInsertId`,
> `PrimaryKeyNotSet`, `AttrNotSet(String)`, `Type(String)`,
> `Json(String)`, `RecordNotFound`, `RecordNotInserted`, `RecordNotUpdated`,
> and `Custom(String)`. Every variant MUST have at least one construction
> site: variants that no code can produce are removed rather than kept as
> documentation.
>
> `Error` implements `PartialEq`/`Eq` by comparing `Display` strings, so two
> errors with distinct payloads but identical rendered messages compare
> equal. A separate `ColumnFromStrError(String)` covers `FromStr` failures on
> entity columns.
>
> The crate root exports `Error` alongside
> `pub type Result<T, E = Error> = std::result::Result<T, E>`. The default
> type parameter makes `pgorm::Result<T>` the ordinary spelling while still
> admitting a foreign error type, as the transaction closures do.
>
> Every public name in this taxonomy MUST be spelled in full: `Error`,
> `RuntimeError`, `ColumnFromStrError`, `SqlError`, and pgorm-query's
> `ValueTypeError` / `ValueTupleError`. An `Err`-abbreviated spelling MUST NOT
> be reintroduced, and no alias is kept behind for a superseded one. A variant
> name MUST NOT restate its enum: the conversion failure is `Error::Conversion`
> and the wrapped database failure is `TryGetError::Db`.

> [spec:pgorm:def:error.model.runtime+3]
> `RuntimeError` has exactly one variant, `Internal(String)`, wrapping
> pgorm-internal failure messages, and is the payload of the sole
> `RuntimeError`-carrying variant of `Error`, `Query`. Three crate-private
> helpers build an `Error` from anything `ToString`: `query_err`
> (`Query(Internal(..))`), `type_err` (`Type`) and `json_err` (`Json`). Each
> helper has at least one call site; a helper whose variant or whose call
> sites are gone is removed rather than kept behind `#[allow(dead_code)]`.

## SQLSTATE classification

> [spec:pgorm:sem:error.model.sql-class+3]
> `SqlError` classifies constraint failures:
> `UniqueConstraintViolation(String)` for SQLSTATE `23505` and
> `ForeignKeyConstraintViolation(String)` for SQLSTATE `23503`, each carrying
> the server message. `Error::sql_error()` is the classifier entry point,
> returning `None` for anything that is not one of these.
>
> The classifier inspects only `Error::Postgres`, taking the driver error's
> `as_db_error()` and matching the resulting `DbError::code()` against those
> two SQLSTATEs; the `SqlError` payload is that same `DbError`'s `message()`.
> An `Error::Postgres` with no server-side `DbError` (a transport or protocol
> failure), any other SQLSTATE, and every non-`Postgres` variant all yield
> `None`.
