# Explicit schema construction

`pgorm.schema` owns native Rust DDL builders. Constructing a statement, importing
a module or declaring a `pgorm.Model` performs no database work. Execute each
statement explicitly through `Pool.execute` or `Connection.execute`.

```python
import asyncio
import os
import pgorm as p
from pgorm import schema as s

async def main() -> None:
    table = p.Table("python_schema_example")
    statement = (
        s.create_table(table)
        .column(s.ColumnDef("id", "integer").primary_key().auto_increment())
        .column(s.ColumnDef("name", s.DataType("varchar", length=80)).not_null())
        .column(s.ColumnDef("created_at", "timestamptz"))
    )
    print(statement.inspect().sql)
    async with p.Pool(os.environ["PGORM_TEST_DSN"]) as pool:
        await pool.execute(statement)
        try:
            await pool.execute(s.create_index(table, "name", name="example_name_idx"))
            row = await pool.fetch_one(
                p.insert(table).columns("name").values("Nora")
                .returning(p.col("id"), p.col("name"))
            )
            print(dict(row))
        finally:
            await pool.execute(s.drop_table(table))

if __name__ == "__main__":
    asyncio.run(main())
```

Builder methods return new objects. `ColumnDef` supports nullability, defaults,
primary keys, uniqueness, auto increment, check constraints and stored generated
expressions. `CreateTable` supports composite primary keys, named unique
constraints, checks and `if_not_exists`. Use unqualified `p.col(...)` expressions
inside checks and generated columns. Table targets must be `p.Table` without
an alias; `schema=` retains the qualified identifier.

`CreateIndex` requires a table and first column. `.column(name, descending=True)`
adds an ordered column; `.unique()`, `.nulls_not_distinct()`, `.if_not_exists()`
and `.method("btree" | "hash" | "gin" | "gist" | "spgist" | "brin")` select native
options. `nulls_not_distinct()` also selects uniqueness and requires PostgreSQL
15 or later. `drop_index(table, name)` uses the table's schema to qualify the
index name.

Table changes use `add_column`, `modify_column`, `drop_column`, `rename_column`,
`rename_table`, `truncate` and `drop_table`. Each returns a ready native DDL
statement. A `modify_column` definition selects Rust's corresponding type,
nullability and default changes. Database validation and privileges still apply;
these APIs do not introspect the database or compute migrations.

`DataType` accepts the closed built-in names listed in
`p.capabilities()["schema_policy"]["column_types"]`. `length=` applies to char,
varchar, bit, varbit and vector; varbit requires it. `numeric` accepts
`precision=1..1000` with optional `scale=0..1000`. A `p.TypeName` references an
existing named type with full schema identity. `.array()` retains that element
type. A type usable in DDL is not necessarily decodable as a dynamic `Record`;
the separate `result_policy` lists result conversion support.

```python
mood = p.TypeName('Mood "name"', schema="application")
enum = s.create_enum(mood, ["calm", "O'Brien", ""])
column = s.ColumnDef("mood", mood).default(p.Value("calm", mood))
array_column = s.ColumnDef("moods", s.DataType(mood).array())
```

`create_enum`, `add_enum_value` (optional `before=` or `after=`),
`rename_enum_value`, `rename_enum` and `drop_enum` preserve quoted type names
and escaped labels. Empty labels are valid; labels must contain no NUL and fit
PostgreSQL's 63-byte limit. Creating a named column does not create its type.
Create schemas explicitly before their objects, for example with `p.RawSQL`.

Rust's PostgreSQL driver caches type definitions on physical connections.
After changing existing enum labels or renaming a type, close and recreate
application pools before querying those types. Closing one acquired Python
connection returns it to the pool and does not refresh the driver's cache.
These bindings preserve the native DDL execution behavior; they do not coordinate
live schema changes across application connections.

DDL inspection returns `Compiled` with no parameters: Rust's DDL renderer emits
typed literals for defaults and checks because these statements cannot use
query placeholders. Python does not concatenate identifiers or values into
DDL. Arbitrary column options, partial/expression/concurrent indexes and runtime
foreign-key construction are not exposed. `schema_policy` records these limits.

## Registered Rust entities

In an application wheel, `s.from_entity(p.entity("app.Account"))` invokes the
real monomorphized `pgorm::Schema` methods for that registered Rust entity. The
returned `EntitySchema` has `.enums`, `.table`, `.indexes` and `.comments`.
Each property returns detached native statements. Execute enums first, then the
table, then indexes and comments; deduplicate shared enum types when creating
multiple entities. Foreign keys or other dependencies may require an
application-specific creation order.

```python
async def create_account_schema(connection: p.Connection) -> None:
    generated = s.from_entity(p.entity("app.Account"))
    for statement in generated.enums:
        await connection.execute(statement)
    await connection.execute(generated.table)
    for statement in generated.indexes:
        await connection.execute(statement)
    for statement in generated.comments:
        await connection.execute(statement)
```

This example requires a registered `app.Account` and its schema to exist.
Runtime Python `Model` descriptors use the explicit builders above.
`operations["schema.from_entity"]["registration_required"]` distinguishes this
path from runtime DDL; `registrations.entities` lists this wheel's entities.
