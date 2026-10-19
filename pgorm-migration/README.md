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

## Domains

A column whose type is a domain needs the domain first, and a migration
creates it with `Domain`, checks written over `Expr::domain_value()`, the
`VALUE` a check tests:

```rust,ignore
async fn up(&self, tx: &DatabaseTransaction<'_>) -> Result<(), Error> {
    let quantity = Domain::create(Name::runtime("quantity"), ColumnType::Integer)
        .default(1)
        .not_null()
        .check(DomainCheck::new(Expr::domain_value().gt(0)).name(Name::runtime("positive")))
        .to_string();
    tx.batch_execute(&quantity).await?;
    Ok(())
}
```

`Domain::alter(..)` takes one action per statement — a default, `NOT NULL`, a
check added (`NOT VALID` or not), validated, renamed or dropped, or the
domain renamed — and `Domain::drop(..)` is refused while a column has the
type unless it `cascade()`s. A changed default reaches only statements
prepared after it: PostgreSQL's plan cache keeps the default an insert was
planned with, and pgorm prepares each statement once per pooled connection.
`up` answers this itself: once a batch that applied a migration commits, it
calls `clear_statement_caches()` on the pool it was given, and each of that
pool's connections prepares its statements again before it is next lent. A
connection the application held checked out across the run keeps its old
statements until it is returned, and another pool on the same database is not
the runner's to clear. DDL applied any other way is followed by the same call:

```rust,ignore
pool.get().await?.batch_execute("ALTER DOMAIN quantity SET DEFAULT 2").await?;
pool.clear_statement_caches();
```
