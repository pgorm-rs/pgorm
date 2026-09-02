# Error Model

`src/error.rs` defines the crate-wide error taxonomy. Every fallible public
operation returns `Result<_, DbErr>`; the finer-grained enums below feed into
it.

## Taxonomy

> [spec:pgorm:def:error.model+3]
> `DbErr` is the crate-wide error enum. Driver and pool failures convert in
> via `From`: `Postgres(tokio_postgres::Error)` (the variant every
> `ConnectionTrait` call and transaction commit produces on database failure;
> its `Display` includes the server `DbError` detail when present) and
> `Pool(pgorm_pool::PoolError)` (produced by `DatabasePool::get`; pool
> exhaustion and acquisition timeouts surface here). The remaining variants
> are constructed by pgorm itself: `TryIntoErr { from, into, source }`,
> `Query(RuntimeErr)`, `ConvertFromU64(&'static str)`, `UnpackInsertId`,
> `PrimaryKeyNotSet`, `AttrNotSet(String)`, `Type(String)`,
> `Json(String)`, `RecordNotFound`, `RecordNotInserted`, `RecordNotUpdated`,
> and `Custom(String)`. Every variant MUST have at least one construction
> site: variants that no code can produce are removed rather than kept as
> documentation.
>
> `DbErr` implements `PartialEq`/`Eq` by comparing `Display` strings, so two
> errors with distinct payloads but identical rendered messages compare
> equal. A separate `ColumnFromStrErr(String)` covers `FromStr` failures on
> entity columns.

> [spec:pgorm:def:error.model.runtime+2]
> `RuntimeErr` has exactly one variant, `Internal(String)`, wrapping
> pgorm-internal failure messages, and is the payload of the sole
> `RuntimeErr`-carrying variant of `DbErr`, `Query`. Three crate-private
> helpers build a `DbErr` from anything `ToString`: `query_err`
> (`Query(Internal(..))`), `type_err` (`Type`) and `json_err` (`Json`). Each
> helper has at least one call site; a helper whose variant or whose call
> sites are gone is removed rather than kept behind `#[allow(dead_code)]`.

## SQLSTATE classification

> [spec:pgorm:sem:error.model.sql-class+2]
> `SqlErr` classifies constraint failures:
> `UniqueConstraintViolation(String)` for SQLSTATE `23505` and
> `ForeignKeyConstraintViolation(String)` for SQLSTATE `23503`, each carrying
> the server message. `DbErr::sql_err()` is the classifier entry point,
> returning `None` for anything that is not one of these.
>
> The classifier inspects only `DbErr::Postgres`, taking the driver error's
> `as_db_error()` and matching the resulting `DbError::code()` against those
> two SQLSTATEs; the `SqlErr` payload is that same `DbError`'s `message()`.
> A `DbErr::Postgres` with no server-side `DbError` (a transport or protocol
> failure), any other SQLSTATE, and every non-`Postgres` variant all yield
> `None`.
