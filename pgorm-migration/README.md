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
statements until it is returned. DDL applied any other way is followed by the
same call:

```rust,ignore
pool.get().await?.batch_execute("ALTER DOMAIN quantity SET DEFAULT 2").await?;
pool.clear_statement_caches();
```

That call reaches only the runner's own pool. For the pools of other
processes on the same database — other replicas, workers — the same batch
also sends a notification on `statement_cache_channel()`, inside its
transaction so that PostgreSQL delivers it exactly when the batch commits. A
pool built with that channel as its `statement_cache_channel` listens there
through one connection of its own and clears on every notification:

```rust,ignore
let pool = pgorm::connect_with(
    config,
    pgorm::NoTls,
    pgorm::ManagerConfig {
        statement_cache_channel: Some(pgorm::STATEMENT_CACHE_CHANNEL.to_owned()),
        ..Default::default()
    },
    |builder| builder,
)?;
```

The setting is off by default, since it costs each pool that takes it an idle
connection. The listener reconnects if its connection drops, and clears once
each time it listens again, for whatever was notified while it was gone. The
channel is `STATEMENT_CACHE_CHANNEL` (`pgorm_statement_caches`) unless a
migrator overrides `statement_cache_channel()`; `None` sends nothing. The
notification goes out whether or not anything listens, and a run that applied
nothing sends none. Python and JavaScript pools take the same setting as
`statement_cache_channel` and `statementCacheChannel`.
