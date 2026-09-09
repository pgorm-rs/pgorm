# Runtime model descriptors

`Model` and `Column` declare application metadata entirely in Python. They use
the installed native statement builders and PostgreSQL Record decoder. Creating
a descriptor or constructing another query requires no compilation, HTTP
adapter or database access. Declarations do not run DDL. They do not instantiate
Rust derives, `EntityTrait` implementations or ActiveModel hooks; those are
available through [compiled entity registrations](ENTITIES.md).

```python
from pgorm import Column, Model, TypeName

Accounts = Model("accounts", {
    "id": Column("i32", primary_key=True),
    "display_name": Column("text", name="display name"),
    "note": Column("text", nullable=True),
    "mood": Column(TypeName("Mood", schema="application")),
    "tags": Column("text", array=True, nullable=True),
}, schema="application")
```

Mapping keys are the Python record identities. `Column.name` is the actual SQL
column name and defaults to its mapping key. Table, schema, physical column and
output names use native identifier validation and Rust quoting. Duplicate
physical columns, duplicate selected field identities and unknown fields fail
before execution. Descriptors copy their input mapping and remain immutable.

## Query and write

```python
async def example(connection):
    created = await Accounts.insert({
        "display_name": "Nora",
        "note": None,
        "mood": "calm",
    }).returning().one(connection)

    key = Accounts.key({"id": created["id"]})
    changed = await (
        Accounts.update({"display_name": "Updated"})
        .where_(key).returning("id", "display_name").one(connection)
    )
    rows = await (
        Accounts.select("id", "display_name")
        .filter(Accounts.col("id") >= created["id"])
        .order_by(Accounts.col("id").asc()).limit(20).all(connection)
    )
    deleted = await Accounts.delete().where_(key).execute(connection)
    return changed, rows, deleted
```

`find()` selects every declared field; `select(*fields)` selects a subset.
Queries provide `filter`, `order_by`, `limit`, `offset` and `join`. Reusing a
query preserves its original state. `as_(alias)` produces another descriptor
whose native expressions use that alias, including self-joins:

```python
left, right = Accounts.as_("a"), Accounts.as_("b")
query = left.select("id").join(right, left.col("id") == right.col("id"))
```

The joined query projects the selected fields from its root descriptor. Use
the native statement APIs when selecting an arbitrary cross-source
shape. `col(field)` validates the field and converts comparison inputs through
its declared native Value tag. `eq`, `ne`, ordering comparisons, `is_in`,
`is_null` and `is_not_null` produce native expressions; `asc`/`desc` produce
native orderings. `col(field).expr()` exposes the native expression for further
composition. Explicit native expressions keep their existing binding semantics.
Use `is_null()` for a NULL test on a non-nullable declaration.

`key(mapping)` constructs a native conjunction over all declared primary-key
fields, in declaration order. It requires every key field and rejects extras.
Keys may be composite. Nullable primary-key declarations are rejected. A key
declaration alone neither creates a constraint nor checks the database schema.

`insert(mapping)`, `update(mapping)` and `delete()` return write builders.
`execute(connection)` returns affected rows. `returning(*fields)` selects
declared output fields and returns a row-producing builder; no arguments means
all fields. Updates and deletes require `where_(predicate)` or explicit
`all_rows()`, using the native write guard. An empty update is an error. An empty
insert requests the Rust builder's default row. Inserts require an unaliased
table.

Missing mapping entries stay omitted, allowing database defaults on INSERT and
leaving fields unchanged on UPDATE. `None` is an explicit SQL NULL and requires
a nullable column. For JSON, `Value.json(None)` is JSON null, including in a
non-nullable JSON column. Plain lists/dicts in a JSON field are JSON values.
Values are copied into native owned Values at construction; later mutation of
the input mapping or containers cannot change the query.

## Records and types

`all`, `one` and `one_opt` accept a Pool or acquired Connection. They return
`ModelRecord` views over detached native Records, keyed by declared Python
identities. `record.model` identifies the descriptor, `record.native` retains
PostgreSQL field metadata and `record.tagged(field)` preserves native tags.
Mutable values returned by record lookup are independent copies.

Output names, native Value types, enum qualifications and nullability must
match the declaration. A mismatch raises `DecodeError`; it is never interpreted
as an absent row. Dynamic `one` requires exactly one row and `one_opt` at most
one, so multiple rows raise `DatabaseError`. Use explicit `limit(1)` to select
a first row. Errors do not imply rollback of a write already executed.

Supported scalar kinds match the native Record decoder: `bool`, `i8`, `i16`,
`i32`, `i64`, `u32`, `f32`, `f64`, `text`, `bytes`, `decimal`, `uuid`, `json`,
`date`, `time`, `datetime`, `datetime_utc`, `ipnetwork`, `mac_address` and `vector`.
Here `i8` corresponds to PostgreSQL's internal `"char"` and `u32` to `oid`.
`vector` requires the PostgreSQL vector extension. One-dimensional arrays use
`array=True`; nullable elements retain native Value semantics. Enums require a
schema-qualified `TypeName`, also for array elements.

Declarations require the canonical decoded tags. For example, PostgreSQL
`integer` produces `i32`; declaring it as `i64` is an error even if its value
fits. PostgreSQL `timestamptz` produces UTC datetimes. The Value-only variants
`u64`, Unicode `char`, `datetime_fixed` and `datetime_local` are not model column
kinds because the Record decoder does not produce those tags. The full value
conversion limits remain in [VALUES.md](VALUES.md). The capability manifest
records the model kinds and policies explicitly.

`query.statement` and `write.statement` expose the actual immutable native
builders. `inspect()` uses their Rust SQL/parameter output. Descriptor methods
lower to `NamedTable`, `Expr`/`SimpleExpr`, `Value`, `SelectStatement`,
`InsertStatement`, `UpdateStatement`, `DeleteStatement` and the existing native
Record execution path. Python contains no SQL renderer or optimizer.

Run the installed-wheel descriptor suite with the existing PostgreSQL fixture:

```sh
target/python-dev/bin/python pgorm-python/tests/with_postgres.py \
  target/python-check/bin/python -m unittest discover \
  -s pgorm-python/tests -p test_models.py -v
```
