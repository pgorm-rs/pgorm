# Error Model

`src/error.rs` defines the crate-wide error taxonomy. Every fallible public
operation returns `Result<_, DbErr>`; the finer-grained enums below feed into
it.

## Taxonomy

> [spec:pgorm:def:error.model]
> `DbErr` is the crate-wide error enum. Driver and pool failures convert in
> via `From`: `Postgres(tokio_postgres::Error)` (the variant every
> `ConnectionTrait` call and transaction commit produces on database failure;
> its `Display` includes the server `DbError` detail when present) and
> `Pool(pgorm_pool::PoolError)` (produced by `DatabasePool::get`). The
> remaining variants are constructed by pgorm itself:
> `ConnectionAcquire(ConnAcquireErr)`, `TryIntoErr { from, into, source }`,
> `Conn(RuntimeErr)`, `Exec(RuntimeErr)`, `Query(RuntimeErr)`,
> `ConvertFromU64(&'static str)`, `UnpackInsertId`, `UpdateGetPrimaryKey`,
> `AttrNotSet(String)`, `Type(String)`, `Json(String)`, `RecordNotFound`,
> `RecordNotInserted`, `RecordNotUpdated`, and `Custom(String)`.
>
> `DbErr` implements `PartialEq`/`Eq` by comparing `Display` strings, so two
> errors with distinct payloads but identical rendered messages compare
> equal. A separate `ColumnFromStrErr(String)` covers `FromStr` failures on
> entity columns.

> [spec:pgorm:def:error.model.runtime]
> `RuntimeErr` has exactly one variant, `Internal(String)`, wrapping
> pgorm-internal failure messages; crate-private helpers (`conn_err`,
> `exec_err`, `query_err`, `type_err`, `json_err`) build the corresponding
> `DbErr` variants from anything `ToString`. `ConnAcquireErr` enumerates pool
> acquisition failures as `Timeout` and `ConnectionClosed` — but no code path
> currently constructs `DbErr::ConnectionAcquire`; in practice pool
> exhaustion and acquisition timeouts surface as `DbErr::Pool` instead, and
> the variant is retained for API compatibility.

## SQLSTATE classification

> [spec:pgorm:sem:error.model.sql-class]
> `SqlErr` classifies constraint failures:
> `UniqueConstraintViolation(String)` for SQLSTATE `23505` and
> `ForeignKeyConstraintViolation(String)` for SQLSTATE `23503`, each carrying
> the server message. `DbErr::sql_err()` is the classifier entry point,
> returning `None` for anything that is not one of these.
>
> Known limitation: the classifier body is vestigial sqlx-era code, gated on
> `sqlx-*` cargo features that no longer exist in this fork (sqlx is not a
> dependency), so `sql_err()` unconditionally returns `None` in current
> builds — it does not yet inspect `DbErr::Postgres` /
> `tokio_postgres::error::DbError::code()`. Callers needing SQLSTATE
> discrimination must match on `DbErr::Postgres` and use
> `tokio_postgres::Error::as_db_error()` themselves.
