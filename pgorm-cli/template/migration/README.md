# Running Migrator CLI

Migrations are up-only: there is no `down`, `fresh`, `refresh` or `reset`.
Roll a mistake forward with a new migration.

Set `DATABASE_URL` to your PostgreSQL connection string before running.

- Generate a new migration file
    ```sh
    cargo run -- generate MIGRATION_NAME
    ```
- Apply all pending migrations
    ```sh
    cargo run
    ```
    ```sh
    cargo run -- up
    ```
- Apply the first 10 pending migrations
    ```sh
    cargo run -- up 10
    ```
- Check the status of all migrations
    ```sh
    cargo run -- status
    ```
