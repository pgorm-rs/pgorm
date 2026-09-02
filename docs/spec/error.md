# Error Model

`src/error.rs` defines the crate-wide error taxonomy. Every fallible public
operation returns `Result<_, DbErr>`; the finer-grained enums below feed into
it.

## Taxonomy

> [spec:pgorm:def:error.model+1]
> `DbErr` is the crate-wide error enum. Driver and pool failures convert in
> via `From`: `Postgres(tokio_postgres::Error)` (the variant every
> `ConnectionTrait` call and transaction commit produces on database failure;
> its `Display` includes the server `DbError` detail when present) and
> `Pool(pgorm_pool::PoolError)` (produced by `DatabasePool::get`; pool
> exhaustion and acquisition timeouts surface here). The remaining variants
> are constructed by pgorm itself: `TryIntoErr { from, into, source }`,
> `Conn(RuntimeErr)`, `Exec(RuntimeErr)`, `Query(RuntimeErr)`,
> `ConvertFromU64(&'static str)`, `UnpackInsertId`, `UpdateGetPrimaryKey`,
> `AttrNotSet(String)`, `Type(String)`, `Json(String)`, `RecordNotFound`,
> `RecordNotInserted`, `RecordNotUpdated`, and `Custom(String)`.
>
> `DbErr` implements `PartialEq`/`Eq` by comparing `Display` strings, so two
> errors with distinct payloads but identical rendered messages compare
> equal. A separate `ColumnFromStrErr(String)` covers `FromStr` failures on
> entity columns.

> [spec:pgorm:def:error.model.runtime+1]
> `RuntimeErr` has exactly one variant, `Internal(String)`, wrapping
> pgorm-internal failure messages, and is the payload of the `Conn`, `Exec`,
> and `Query` variants of `DbErr`. Crate-private helpers (`conn_err`,
> `exec_err`, `query_err`, `type_err`, `json_err`) build the corresponding
> `DbErr` variants from anything `ToString`.

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
