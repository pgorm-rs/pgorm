# pgorm

A fork of [SeaORM](https://github.com/SeaQL/sea-orm) focused entirely on Postgres support.

## Primary differences with SeaORM

- Supports ONLY Postgres
- Uses deadpool for the database pool
- Uses tokio-postgres for the Postgres engine (i.e. no sqlx functionality)
- More effective use of statements (you pass the arguments with the statement so it is prepared properly)
- All Postgres-specific functionality is expected to be present
- Significant performance and stability gains
- Invalid SQL is a compile error where the type system can make it one: the
  query and DDL builders refuse to construct statements PostgreSQL would
  reject, and the render paths do not panic
- Every rendered statement in the test suite is validated against the real
  PostgreSQL grammar via libpg_query; the same parser backs the optional
  `sql!` macro (compile-time checking of raw SQL) and query fingerprinting
  in the metrics layer
- Pared-back migrations: `pgorm-migration` is a minimal up-only runner with no down migrations or rollback
- Scoped transactions
- Opt-in metrics layer in `pgorm::metric` — wrap a pool to instrument it, pay nothing if you don't (see [METRICS.md](METRICS.md))
- From<...> implementation for ActiveValue fields (less `ActiveValue::Set(...)`, more `.into()`)
- `pgorm-query` (fork of `sea-query`) is in-tree and all non-Postgres functionality is removed
- Failsafe behaviour for `insert_many` on an empty iterator

## License

Licensed under either of

-   Apache License, Version 2.0
    ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
-   MIT license
    ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
