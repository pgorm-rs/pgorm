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
        .column(s.ColumnDef("id", "integer").auto_increment())
        .column(s.ColumnDef("name", s.DataType("varchar", length=80)).not_null())
        .column(s.ColumnDef("created_at", "timestamptz"))
        .primary_key("id")
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
auto increment, check constraints and generated expressions, whose kind is
always named: `.generated(expr, "stored")` or `.generated(expr, "virtual")`,
since PostgreSQL 17 requires `STORED` and 18 reads a bare one as `VIRTUAL`.
`.not_null(name=None, no_inherit=False)` is the column's one `NOT NULL`
constraint, which PostgreSQL 18 records by name — the one given, or one it
derives — and which a table that inherits this one takes unless
`no_inherit=True`. `.check(condition, name=None, not_enforced=False)` adds a
`CHECK`; `not_enforced=True` is PostgreSQL 18's `NOT ENFORCED`, recorded and
never checked. `.collate(name, schema=None)` is the column's one collation,
written after its type: `s.ColumnDef("code", "text").collate("C")` compares and
sorts the column's text bytewise, and a later call replaces an earlier.
`modify_column` writes it on the retype, the only place PostgreSQL changes a
column's collation. The server refuses a collation that does not exist
(`42704`) and one on a type without collations (`42804`). Keys are
the table's: `CreateTable.primary_key(*columns)` declares its one primary key,
which a later call replaces, and `CreateTable.unique(*columns, name=...,
nulls_not_distinct=...)` adds a unique key, so a table cannot be written with
two primary keys. Either takes `without_overlaps=column`, PostgreSQL 18's
temporal key: the column, a range or multirange, is written last as `column
WITHOUT OVERLAPS`, and two rows may then share the other columns only while
their periods do not overlap. Its GiST index needs the `btree_gist` extension
for the other columns. `add_primary_key` and `add_unique` take the same
keywords. `CreateTable` also supports `check(condition, name=None,
not_enforced=False)` and `if_not_exists`. Use
unqualified `p.col(...)` expressions
inside checks and generated columns. Table targets must be `p.Table` without
an alias; `schema=` retains the qualified identifier.

`CreateIndex` requires a table and first column. `.column(name, descending=True)`
adds an ordered column; `.unique()`, `.nulls_not_distinct()`, `.if_not_exists()`
and `.method("btree" | "hash" | "gin" | "gist" | "spgist" | "brin")` select native
options. `nulls_not_distinct()` also selects uniqueness and requires PostgreSQL
15 or later. `drop_index(table, name)` uses the table's schema to qualify the
index name.

Table changes use `add_column`, `modify_column`, `drop_column`, `rename_column`,
`set_expression`, `drop_expression`, `add_not_null`, `add_check`,
`validate_constraint`, `alter_constraint`, `drop_constraint`,
`rename_constraint`, `rename_table`, `truncate` and `drop_table`. Each returns a ready native DDL statement. A `modify_column`
definition selects Rust's corresponding type, nullability and default changes;
a generated column's expression is changed by `set_expression(table, column,
expr)`, which the rows already written take, and removed by
`drop_expression(table, column, if_exists=False)`, which leaves a stored column
plain with its values. `add_not_null(table, column, name=None,
no_inherit=False, not_valid=False)` adds a `NOT NULL` constraint over a column
that exists; with `not_valid=True` the rows already there are left unchecked
until `validate_constraint(table, name)` checks them, while new rows are held
to it at once. `add_check(table, condition, name=None, not_enforced=False, no_inherit=False,
not_valid=False)` adds a `CHECK` to a table that exists; with `not_valid=True`
the rows already there are left for `validate_constraint` while new rows are
held to it at once. `no_inherit=True`, which `ColumnDef.check` and
`CreateTable.check` take too, keeps a `CHECK` from tables that inherit this
one; a partitioned table refuses it. `alter_constraint(table, name,
"inherit" | "no_inherit")` changes whether a `NOT NULL` constraint passes to
inheriting tables, and `alter_constraint(table, name, "enforced" |
"not_enforced")` whether a foreign key, or on PostgreSQL 19 a `CHECK`, is
enforced; enforcing one checks the rows already there.
`drop_constraint(table, name, if_exists=False, cascade=False)` drops a
constraint of any kind by name — a key, a foreign key, a `CHECK` or a `NOT
NULL` — and `rename_constraint(table, name, new_name)` renames one, a key's
index with it. Database
validation and privileges still apply; these APIs do not introspect the
database or compute migrations.

`DataType` accepts the closed built-in names listed in
`p.capabilities()["schema_policy"]["column_types"]`. `length=` applies to char,
varchar, bit, varbit and vector; varbit requires it. `numeric` accepts
`precision=1..1000` with optional `scale=0..1000`. The six built-in range types
and their multiranges are named as PostgreSQL names them, `int4range` through
`tstzmultirange`, the column type a `without_overlaps` key ends with. A
`CreatedRange` or `CreatedMultirange` names a range type a schema created, Rust's
`ColumnType::CreatedRange` / `CreatedMultirange`, written quoted and
schema-qualified as it was given.
A `p.TypeName` references an
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
Create schemas explicitly before their objects, with `s.create_schema`
([below](#schemas)).

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

Like SQL expressions and statements, every `pgorm.schema` object — `DataType`,
`ColumnDef`, `CreateTable`, `CreateIndex`, `DDL` and `EntitySchema` — and the
`Compiled` an inspection returns raise `ConstructionError` on Python truth
testing, so `if statement:` or `statement or fallback` fails where it is written
rather than reading every statement as true.

## Schemas

A schema is the namespace a `p.Table(..., schema=...)`, a `p.TypeName` or a
sequence is qualified by. It is named by one identifier, a `str` or a
`p.Identifier`, quoted, and never qualified itself.

```python
statements = [
    s.create_schema("billing", if_not_exists=True),
    s.create_table(p.Table("invoice", schema="billing")).column(s.ColumnDef("id", "integer")),
    s.rename_schema("billing", "ledger"),
    s.drop_schema("ledger", "archive", if_exists=True, cascade=True),
]
```

`create_schema(name, if_not_exists=False)` creates one; a schema that exists
is refused (`42P06`) unless `if_not_exists`, and PostgreSQL keeps names
starting `pg_` for itself (`42939`). `drop_schema(name, *names,
if_exists=False, cascade=False)` drops every schema it names. A schema that
holds anything is refused (`2BP01`) unless `cascade=True`, which drops what it
holds as well; a missing one is refused (`3F000`) unless `if_exists`.
`rename_schema(name, new_name)` renames one, its contents moving with it; it
has no `if_exists`, PostgreSQL having none for it. `AUTHORIZATION` and `OWNER
TO`, which name a role, are not built.

## Domains

A domain is a named type over a base type, with a default, a `NOT NULL` and
named `CHECK`s of its own. It is named as an enum type is: a `str` or a
`p.Identifier`, or a `p.TypeName` carrying its schema. A column whose type is a
domain names it as a `p.TypeName`, exactly as a column of an enum type does.

```python
quantity = p.TypeName("quantity", schema="billing")
statements = [
    s.create_domain(quantity, "integer").default(1).not_null()
        .check(p.domain_value() > 0, name="positive"),
    s.create_table(p.Table("line", schema="billing"))
        .column(s.ColumnDef("qty", quantity)),
    s.add_domain_check(quantity, p.domain_value() < 1000, name="small", not_valid=True),
    s.validate_domain_constraint(quantity, "small"),
    s.drop_domain(quantity, cascade=True),
]
```

`CreateDomain(name, base)`, also spelled `create_domain`, builds as
`CreateTable` does, each method returning a new builder: the base is any type
`ColumnDef` takes, `collate(name, schema=None)` names a collation for a base
type that has one (`42804` otherwise), `default(value)` and the pair
`not_null()` / `null()` are one slot each, the later call winning, and
`check(condition, name=None)` adds a check, in order. A check's condition is an
expression over `p.domain_value()`, the keyword `VALUE`: inside a domain's
check PostgreSQL reads it as the value tested.

Each `ALTER DOMAIN` action is its own function, over the domain named first:
`set_domain_default(domain, value)`, `drop_domain_default(domain)`,
`set_domain_not_null(domain)`, `drop_domain_not_null(domain)`,
`add_domain_check(domain, condition, name=None, not_valid=False)`,
`drop_domain_constraint(domain, name, if_exists=False, cascade=False)`,
`rename_domain_constraint(domain, name, new_name)`,
`validate_domain_constraint(domain, name)` and `rename_domain(domain,
new_name)`. None takes `if_exists`, PostgreSQL having none for `ALTER DOMAIN`.
`drop_domain(domain, *domains, if_exists=False, cascade=False)` drops every
domain it names; one a column is typed with is refused (`2BP01`) unless
`cascade=True`, which drops the column too.

A value outside a check (`23514`) or a null under `NOT NULL` (`23502`) is
refused whether it is written to a column of the domain or already there when
`add_domain_check`, `set_domain_not_null` or `validate_domain_constraint` runs;
`not_valid=True` leaves the values already there for
`validate_domain_constraint` to check. A name a type or a constraint already
has is refused (`42710`), and a missing domain or constraint (`42704`) unless
the drop says `if_exists`. An insert the connection prepared before
`set_domain_default` or `drop_domain_default` keeps the default it was planned
with: PostgreSQL's plan cache does not track a domain's default.

## Views, materialized views and tables from a query

A view, a materialized view and `CREATE TABLE AS` wrap a `Select` in a name.
PostgreSQL stores a view's definition as SQL text, so the query's values are
written into it as literals by Rust's renderer, and `inspect().params` is
empty. The name is a `p.Table` without an alias; `columns=` names the leading
columns the query returns. The query is read as a CTE body or a subquery is:
nothing can order it once it is a definition, so a `limit_with_ties` without
`order_by`, or a WINDOW clause PostgreSQL would refuse, raises
`ConstructionError` as the function is called.

```python
account = p.Table("account", schema="app")
active = p.Select(p.col("id"), p.col("name")).from_(account).where_(p.col("active") == True)
view = s.create_view(p.Table("active_account", schema="app"), active, check_option="cascaded")
snapshot = p.Table("account_snapshot", schema="app")
statements = [
    view,
    s.create_materialized_view(snapshot, active, with_no_data=True),
    s.create_index(snapshot, "id", name="account_snapshot_id").unique(),
    s.refresh_materialized_view(snapshot),
    s.refresh_materialized_view(snapshot, concurrently=True),
    s.create_table_as(p.Table("account_copy", schema="app"), active, columns=["account_id"]),
]
```

`create_view(name, query, columns=(), or_replace=False, check_option=None,
security_barrier=False, security_invoker=False)` creates a view. Rows written
through an automatically updatable view reach its table; with
`check_option="local"` or `"cascaded"` a row the view would not show is
refused (`44000`), `"cascaded"` consulting the views beneath it too.
`or_replace=True` may add columns after the view's own and replaces its
options. `security_barrier` evaluates the view's conditions before a caller's,
and `security_invoker` reads the underlying tables with the querying role's
privileges. `create_recursive_view(name, columns, query, ...)` names at least
one column and takes no check option. Its query is a non-recursive term joined
by `union` or `union_all` to an arm that reads the view (see
[STATEMENTS.md](STATEMENTS.md#set-operations)). PostgreSQL defines the view as
a `WITH RECURSIVE` query named by the view's bare name, so the arm reads it
unqualified; a schema-qualified name looks for a relation that does not exist
yet (`42P01`):

```python
counter = p.Table("counter", schema="app")
step = p.Select(p.col("n") + 1).from_(p.Table("counter")).where_(p.col("n") < 4)
counting = s.create_recursive_view(counter, ["n"], p.Select(p.literal(1).as_("n")).union_all(step))
```

`drop_view(name, if_exists=False, cascade=False)` drops a view.

`create_materialized_view(name, query, columns=(), if_not_exists=False,
with_no_data=False)` stores the query's rows, or none until a refresh;
`refresh_materialized_view(name, concurrently=False, with_no_data=False)`
replaces them. A concurrent refresh needs a unique index on the view and a
populated view, and cannot be combined with `with_no_data`, which is refused
as the statement is built. `drop_materialized_view` drops one.
`create_table_as(name, query, columns=(), if_not_exists=False,
with_no_data=False)` creates an ordinary table from the query's columns and,
unless `with_no_data`, its rows.

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
