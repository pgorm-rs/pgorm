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

## Schemas

An entity declared with `schema_name` lives in that schema, and so does a
table, type or sequence a migration names as a `(schema, name)` pair. The
schema has to exist first, and a migration creates it with the same builders
as the rest — the prelude brings in pgorm-query's `Schema`:

```rust,ignore
async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
    tx.batch_execute(&Schema::create(Name::runtime("billing")).if_not_exists().to_string())
        .await?;
    let invoice = Table::create((Name::runtime("billing"), Name::runtime("invoice")))
        .col(ColumnDef::new(Name::runtime("id")).integer().not_null())
        .to_owned();
    tx.batch_execute(&invoice.to_string()).await?;
    Ok(())
}
```

`Schema::drop(..)` and `Schema::rename(..)` drop and rename one. A drop is
refused while the schema holds anything unless it `cascade()`s, which drops
what it holds too. The prelude's `Schema` is pgorm-query's statement builder;
`pgorm::Schema`, which generates an entity's DDL, is a different type, so a
migration that uses both names that one by its path.
