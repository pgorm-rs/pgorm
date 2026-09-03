# pgorm-migration

A minimal, up-only migration runner for pgorm. Migrations are plain Rust —
each one implements `MigrationTrait` with an `up` that receives a connection —
and applied migrations are recorded in a tracking table. There are no down
migrations, no rollback, and no CLI: you write forward migrations and run
them from your own binary.

A starter migration crate lives at
[`template/migration/`](template/migration/) — copy it into your project,
add your migration modules to the migrator, and run it against your
database.
