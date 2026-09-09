# Runtime statement builders

`Select`, `Insert`, `Update` and `Delete` own the corresponding Rust query
builders. The lowercase `select`, `insert`, `update` and `delete` exports are
aliases for their constructors. Methods return new builders; inputs and earlier
query versions remain reusable. No application schema needs compilation.

```python
from pgorm import Condition, Join, Nulls, Table, call, select

account = Table("account", schema="application", alias="a")
event = Table("event", schema="application", alias="e")
count = call("count", event.col("id"))

query = (
    select(account.col("name"), count.as_("events"))
    .from_(account)
    .join(event, account.col("id") == event.col("account_id"), kind=Join.Left)
    .where_(Condition.all(
        account.col("active").eq(True),
        Condition.any(event.col("kind") == "click", event.col("id").is_null()),
    ))
    .group_by(account.col("name"))
    .having(count > 2)
    .order_by(count.desc(nulls=Nulls.Last))
    .limit(5)
)

compiled = query.inspect()
print(compiled.sql)
print([value.snapshot() for value in compiled.params])
```

`Table` owns a Rust `NamedTable`, with its optional schema and alias. `table.col`
uses the alias if present; otherwise it retains the table's qualification.
`table.star()` selects that table's columns. All identifier parts follow the
limits and quoting rules in [EXPRESSIONS.md](EXPRESSIONS.md).

`select()` starts with `*`. Explicit projections are expressions or
`expr.as_(name)` aliases. `.select(*items)` replaces the existing projection and
requires at least one item. `.from_` adds a source. `.join` requires both the
table and ON expression/condition and accepts `Join.Inner`, `Left`, `Right` or
`Full`; `.cross_join` takes just its table.

Repeated `.where_` and `.having` calls combine conditions through the Rust
condition builder. `.group_by` accepts expressions, `.order_by` accepts typed
`OrderBy` objects, and `.distinct()` sets Rust's DISTINCT option.
Limits and offsets use the actual Rust `u64` builder methods, restricted to
PostgreSQL's non-negative signed-bigint range. Booleans, floats, strings,
negative values and overflow raise `ConstructionError`. Pass `None` to
`limit` or `offset` to reset that clause.

## Writes

```python
from pgorm import ConflictTarget, Table, col, delete, insert, literal, update

account = Table("account", schema="application")

create = (
    insert(account).columns("id", "name")
    .values(1, "Alice")
    .values(literal(2), "O'Brien")
    .on_conflict(ConflictTarget("id").update("name"))
    .returning(col("id"), col("name"))
)
rename = update(account).set("name", "new name").where_(account.col("id") == 1)
remove = delete(account).where_(account.col("id") == 2).returning()
```

Values use the [native conversion policy](VALUES.md): inferred Python integers
are `i64`, and explicit `Value(number, "i32")` selects an `i32` parameter for
columns requiring that Rust wire type. Literal and bound expression operands
remain selectable on writes.

INSERT columns are distinct identifiers and must be chosen before adding rows.
Each `values(*items)` call adds one row using Rust's fallible arity check.
An insert with no rows raises an error; `default_values()` explicitly requests
one default row through Rust's `or_default_values`. It cannot be mixed with
columns or ordinary rows. Its current PostgreSQL spelling is `VALUES (DEFAULT)`.

UPDATE requires at least one assignment. Duplicate assignments raise an error.
UPDATE and DELETE require a `.where_(...)` call or explicit `.all_rows()` before
inspection/execution preparation. A predicate can intentionally be true: this
guard requires authored intent and does not prove that a query affects few rows.
`.all_rows()` permits a statement without a predicate; it does not remove an
existing predicate.

`.returning(*items)` accepts expressions and projection aliases. With no items
it requests `RETURNING *`. It replaces the previous RETURNING clause through
the Rust builder.

## Conflict actions

`Conflict.ignore()` selects Rust's untargeted `ON CONFLICT DO NOTHING`.
`ConflictTarget(*columns)` requires a nonempty target and preserves the Rust
target/action states:

| Operation | Rust API |
| --- | --- |
| `target.where_(predicate)` | `ConflictTarget::cond_where` for a partial-index target |
| `target.ignore()` | `ConflictTarget::do_nothing` |
| `target.update(*columns)` | nonempty `ConflictUpdate` from EXCLUDED column values |
| `target.set(column, value)` | nonempty `ConflictUpdate` with an expression assignment |
| `action.update(column)` | `ConflictUpdate::update_column` |
| `action.set(column, value)` | `ConflictUpdate::value` |
| `action.where_(predicate)` | `ConflictUpdate::cond_where` for the update action |

`insert.on_conflict` accepts a completed ignore or update action. An incomplete
target and an empty update assignment set cannot reach the Rust INSERT builder.

## Inspection and execution preparation

`inspect()` validates the public builder state and invokes that statement's
Rust `build()` method. It returns `Compiled.sql` and independently owned
`Compiled.params`. More than 65,535 bound parameters raises `ConstructionError`
before execution preparation. SQL and parameters are not combined implicitly.

The extension's Rust `statements::compile` entry point uses exactly these same
validated builders for execution preparation. It accepts native statements,
`Compiled` objects and explicit `RawSQL`, and rejects ordinary strings. These
are dynamic statement operations; compiled entities and ActiveModels have
separate APIs and hooks.

## Explicit application SQL

```python
from pgorm import RawSQL

query = RawSQL("SELECT $1::int8, $2::text", [42, "O'Brien"])
assert query.inspect().sql == "SELECT $1::int8, $2::text"
print(query.inline_sql())
```

`RawSQL` owns the supplied SQL text and separate Rust values. Inspection retains
the template unchanged. Raw execution retains the normal PostgreSQL/pgorm
syntax, type and placeholder-arity errors. Constructing a raw template does not
validate its SQL grammar or implicitly inline values.

`inline_sql()` explicitly invokes `pgorm_query::inject_parameters`, using the
Rust PostgreSQL lexer and value renderer. It respects quoted strings,
dollar-quoted bodies and comments, validates placeholder indices/arity, and
returns owned SQL text. An invalid substitution raises `ConstructionError`.
There is no Python substitution algorithm and no fallback from an unsupported
builder operation to raw SQL. Raw SQL remains an application-authored API;
generated security campaigns have their own stricter policy.

Run installed statement tests and native Rust parity tests:

```sh
target/python-check/bin/python -m unittest discover \
  -s pgorm-python/tests -p 'test_statements.py' -v
PYO3_PYTHON="$PWD/target/python-dev/bin/python" cargo test \
  --manifest-path pgorm-python/Cargo.toml --target-dir target --locked --lib
```
