# pgorm

A fork of pgorm focused entirely on Postgres support.

## Primary differences with SeaORM

- Supports ONLY Postgres
- Uses deadpool for the database pool
- Uses tokio-postgres for the Postgres engine (i.e. no sqlx functionality)
- Support for using pgparse to validate syntactic validity of SQL strings
- More effective use of statements (you pass the arguments with the statement so it is prepared properly)
- All Postgres-specific functionality is expected to be present
- Significant performance and stability gains
- Significantly simplified migration experience
- Scoped transactions
- Improved debugging
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
