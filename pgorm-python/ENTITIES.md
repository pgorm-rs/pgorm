# Compiled application entities

`pgorm.entity(name)` exposes a concrete Rust entity compiled into the installed
native module. Queries own a real `pgorm::Select<E>`; records own `E::Model`;
writes call `E::ActiveModel` and its `ActiveModelBehavior` hooks. All calls run
inside Python's process, using PostgreSQL's native protocol.

The standalone wheel has an empty registry. Runtime `Table`, SELECT and CRUD
builders work without registrations. Applications that already have Rust
entities can build a wheel containing those types and the common Python API.
Python cannot create Rust generic instantiations at runtime.

## Build an application wheel

Use one native module containing both the common bindings and the application
registrations. Disable the binding crate's default `standalone-module` feature
and provide the following initializer in the application's Rust crate:

```rust
use pyo3::prelude::*;

#[pymodule(gil_used = true)]
fn _native(module: &Bound<'_, PyModule>) -> PyResult<()> {
    let mut registry = pgorm_python::entities::Registry::default();
    registry.entity::<my_entities::account::Entity>("app.Account")?;
    pgorm_python::install(module, registry)
}
```

The library target is named `_native` with `crate-type = ["cdylib", "rlib"]`.
Depend on this checkout's `pgorm-python` with `default-features = false`, the
same checkout's `pgorm`, your entity crate, and `pyo3 = "=0.29.2"`. Define an
`extension-module` feature enabling `pyo3/extension-module`. Registration
requires `E: EntityTrait + Send + Sync + 'static`,
`E::Model: IntoActiveModel<E::ActiveModel> + Sync + 'static`, and
`E::ActiveModel: Send + Sync + 'static`. Duplicate names and duplicate Rust
entity types fail before the entry is added.

Copy the matching `pgorm-python/python/pgorm` facade into the application's
`python/pgorm` directory. Its Maturin configuration is:

```toml
[build-system]
requires = ["maturin==1.15.0"]
build-backend = "maturin"

[project]
name = "pgorm"
dynamic = ["version"]
requires-python = ">=3.14,<3.15"

[tool.maturin]
bindings = "pyo3"
python-source = "python"
module-name = "pgorm._native"
features = ["extension-module"]
```

The application crate version must match the common facade's package version
(currently 0.2.0). Build with Maturin and CPython 3.14, then install that wheel
in the application's environment. It supplies the `pgorm` distribution and
replaces the standalone wheel there. Separate extensions containing duplicate
copies of the binding classes are not a supported registration mechanism.

The independent [application crate](tests/application-binding) and
[build checker](checks/entities.py) are executable examples. The checker
materializes the facade next to the downstream Rust crate, runs Rust parity
tests, builds a wheel, and installs it into a fresh environment:

```sh
target/python-dev/bin/python pgorm-python/tests/with_postgres.py \
  target/python-dev/bin/python pgorm-python/checks/entities.py
```

An existing disposable server can instead be supplied with `PGORM_TEST_DSN`.
The checker uses its own `python_entities` schema; the database account needs
schema creation privileges. The output wheel and success report are written
under `target/python-entities`.

## Query and write

Using the example registration:

```python
import pgorm as p

Account = p.entity("app.Account")
query = Account.find().filter(Account.col("id") >= 10)
query = query.order_by(Account.col("id").expr().desc()).limit(20)
print(query.inspect().sql)

async with pool.connection() as connection:
    accounts = await query.all(connection)
    first = await query.one_opt(connection)
    active = Account.active().set("id", 42).set("display name", "Nora")
    inserted = await active.insert(connection)
    updated = await inserted.into_active().set("note", "Reviewed").update(connection)
    affected = await updated.into_active().delete(connection)
```

Entity terminals currently require an acquired `Connection`. They return
native asyncio awaitables; calling a terminal schedules its operation. Await
each operation before reusing the connection. An outstanding operation can be
cancelled through its future or `asyncio.wait_for`; cancellation discards the
connection if the operation's completion is uncertain. Closing its connection
or pool also cancels an outstanding Rust hook. Cancellation does not guarantee
rollback of a submitted write.

`all` returns a list of detached models. Rust's `one` and `one_opt` apply
`LIMIT 1`, including when an earlier query limit was larger. `one` raises
`DatabaseError` on absence; `one_opt` returns `None`. `inspect(terminal="one")`
or `inspect(terminal="one_opt")` includes that terminal's limit. A decoding
failure remains `DecodeError`. These are the registered Rust selector's
semantics; the dynamic `fetch_one` terminal has its own strict cardinality
contract. Entity streams and arbitrary partial entity projections are not
advertised by this registration API.

## Values, state and hooks

Column lookup and model keys use SQL names. `describe()` also reports the
separate JSON key, SQL type, nullability, primary key and concrete Rust type
names. Plain Python inputs use the declared SQL column type as a conversion
hint. Explicit `Value` tags preserve the caller's chosen Rust value variant;
the real Rust model/ActiveModel setter performs the final conversion checks.
An unusual custom field type may therefore require an explicit tagged value.
Qualified enum input must match the declared column enum identity.

`dict(model)` returns Python values, and `model.tagged("id")` retains the Rust
value tag. `model.with_value(column, value)` clones and calls `ModelTrait::set`;
it changes the detached model and performs no database write. `into_active()`
calls the real Rust conversion, usually producing `Unchanged` fields. To write
a changed field, call `set` on that ActiveModel.

`ActiveValue.state` distinguishes `NotSet`, `Set` and `Unchanged`.
`ActiveValue.value` is absent only for `NotSet`; a SQL NULL is an explicit
tagged value. `set`, `not_set` and `reset` return independent ActiveModel
wrappers. `Account.active()` calls the application's `ActiveModelBehavior::new`
and preserves its defaults. Each write operates on a clone, so the Python
wrapper is reusable; it is not a mutable object that marks itself saved.

Insert, update and delete run the actual before/after hooks. A before-hook
failure prevents the subsequent write. An after-hook failure is reported even
though the write may already have committed. The binding adds no implicit
transaction or retries around these methods.

Registered models decode with their actual `Model::from_query_result` and then
use checked Rust-value-to-Python conversion. Their custom Rust decoders retain
their own behavior. The additional PostgreSQL wire exactness policies for
dynamic `Record` results are scoped to that result form; they do not replace an
application's typed decoder. `capabilities()` records this distinction and
lists each registration's types and supported terminals.
