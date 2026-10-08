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
requires at least one item. `.from_` adds a source; a second is comma-joined.
`.join` requires both the source and ON expression/condition and accepts
`Join.Inner`, `Left`, `Right` or `Full`; `.cross_join` takes just its source.

A source is a `Table` or a `FromItem`, a `FROM` item that is not a named table.
`json_table` builds one (see [EXPRESSIONS.md](EXPRESSIONS.md#json_table)). A
`FromItem` always has an alias, as PostgreSQL requires of such items, so
`item.col(name)` and `item.star()` qualify its columns by a name the caller
chose. Anything else raises `ConstructionError`.

Repeated `.where_` and `.having` calls combine conditions through the Rust
condition builder. `.group_by` accepts expressions, `.order_by` accepts typed
`OrderBy` objects, and `.distinct()` sets Rust's DISTINCT option.
Limits and offsets use the actual Rust `u64` builder methods, restricted to
PostgreSQL's non-negative signed-bigint range. Booleans, floats, strings,
negative values and overflow raise `ConstructionError`. Pass `None` to
`limit` or `offset` to reset that clause.

## Window functions

A SELECT item calls a function over a window with `expr.over(window)`, which
returns a `WindowedExpr`; `.as_(name)` names it. `Window` owns a Rust
`WindowStatement`: `.partition_by(*expressions)`, `.order_by(*orderings)`
with the same `OrderBy` objects as a statement, and `.frame(frame)`, which
replaces any frame already set. Each method returns a new window.

```python
from pgorm import FrameExclusion, FrameType, Table, Window, call, select, window_function

reading = Table("reading", alias="r")
running = Window().partition_by(reading.col("kind")).order_by(reading.col("at").asc())
query = select(
    reading.col("id"),
    call("sum", reading.col("weight")).over(running).as_("running"),
    window_function("row_number").over("by_kind").as_("n"),
    call("avg", reading.col("weight")).over(
        Window().order_by(reading.col("id").asc())
        .frame(FrameType.Rows.preceding(1).and_following(1).exclude(FrameExclusion.CurrentRow))
    ),
).from_(reading).window("by_kind", Window().partition_by(reading.col("kind")))
```

`over` takes a `Window`, written inline as `OVER (..)`, or the name of the
window the statement declares with `.window(name, window)`, written
`OVER "name"` beside a `WINDOW "name" AS (..)` clause. Rust's builder holds one
named window, so a second `.window` call replaces the first. Window names are
identifiers, quoted case-exactly like every other.

PostgreSQL writes `OVER` only after a function call, and Rust's
`expr_window` takes only a `WindowFunction`: a `FunctionCall` or one of the
SQL/JSON aggregates. `over` follows that: a call from `call(..)`, a
`json_arrayagg(..)` or a `json_objectagg(..)` takes a window, and anything
else — a column, arithmetic, a cast, another SQL/JSON function — raises
`ConstructionError`. The functions PostgreSQL computes only over a window come
from `window_function(name, *arguments)`: `row_number`, `rank`, `dense_rank`,
`percent_rank` and `cume_dist` take no argument, `ntile`, `first_value` and
`last_value` one, `nth_value` two, and `lag` and `lead` one to three. It
returns a `WindowFunction`, whose only method is `over`, so such a function
cannot stand in a query without its window (`42809` from the server). Another
name or argument count raises `UnsupportedCapabilityError`. Arguments are
bound like any value; a count PostgreSQL types `integer`, such as `ntile`'s,
`nth_value`'s or `lag`'s offset, is written with `literal(n)`, because a Python
`int` is bound as `bigint`.

A frame is begun from its mode, `FrameType.Range`, `Rows` or `Groups`, whose
four methods name the start: `unbounded_preceding()`, `preceding(offset)`,
`current_row()` and `following(offset)`. There is no unbounded-following
start. Each start offers only the ends that may follow it, as Rust's typestate
does: after a preceding start `and_preceding`, `and_current_row`,
`and_following` and `and_unbounded_following`; after `current_row()` all but
`and_preceding`; after a following start only `and_following` and
`and_unbounded_following`. The missing methods do not exist, so `ROWS BETWEEN
CURRENT ROW AND 1 PRECEDING` cannot be written. A preceding or current-row
start stands alone as a whole frame; a following start does not, because
PostgreSQL reads a lone start as running to the current row behind it, and
`Window.frame` refuses one with `ConstructionError`. `exclude(FrameExclusion)`
— `CurrentRow`, `Group`, `Ties` or `NoOthers` — is a method of a frame, never
of a window.

An offset is any value or expression without a column reference. Under `Rows`
and `Groups` it is a row or peer-group count; under `Range` it is a distance
in the ordering column's own values, such as a `Decimal` over a numeric
column or `literal("1 day").cast(TypeName("interval"))` over a timestamp. The
builder does not know the ordering column's type, so the server refuses an
offset type that does not pair with it (`0A000` under `Range`), a `Range`
offset without exactly one ordering column (`42P20`), and `DISTINCT` in a
windowed aggregate (`0A000`).

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

### Row versions in RETURNING

PostgreSQL 18's RETURNING reads a written row as it was before the write and as
the statement left it. `ReturningRow.Old` and `ReturningRow.New` name the two
versions: `.col(name)` reads one column (Rust's `(ReturningRow, column)`) and
`.star()` every column. An UPDATE's `old` is the row before the update. A
DELETE's `new` and an inserted row's `old` read NULL in every column. A row
that `ON CONFLICT DO UPDATE` updated has the existing row as its `old`, so
`ReturningRow.Old.col("id").is_null()` says which rows an upsert inserted:

```python
from pgorm import ConflictTarget, ReturningRow, col, insert

upsert = (
    insert(account).columns("id", "name").values(1, "Alice")
    .on_conflict(ConflictTarget("id").update("name"))
    .returning(col("id"), ReturningRow.Old.col("id").is_null().as_("inserted"))
)
```

`returning(..., old_as=name, new_as=name)` renames a version, writing
`RETURNING WITH (OLD AS "name", NEW AS "name")`, and the list then reads it
as an ordinary table: `col("visits", table=name)`. Use it when the target, its
alias or another relation of the statement is called `old` or `new`, which
would otherwise take the keyword silently. A renamed version no longer answers
to the keyword (`42P01`), a name that clashes with one of the statement's
relations is refused (`42712`), and so is one name for both versions. The
names are identifiers, quoted.

## MERGE

```python
from pgorm import MatchedAction, MergeInsert, MergeUpdate, ReturningRow, Table, merge

target, source = Table("account").as_("t"), Table("staged").as_("s")
sync = (
    merge(target, source, target.col("id") == source.col("id"))
    .when_matched(MergeUpdate("name", source.col("name")))
    .when_matched(MatchedAction.Delete, condition=source.col("name").is_null())
    .when_not_matched(MergeInsert("id", source.col("id")).and_value("name", source.col("name")))
    .when_not_matched_by_source(MatchedAction.Delete)
    .returning_action()
    .returning(target.col("id"), ReturningRow.Old.col("name").as_("was"))
)
```

`merge(target, source, on)` returns a `PendingMerge`, Rust's `PendingMerge`:
PostgreSQL refuses a MERGE with no WHEN clause (`42601`), so it has no
`inspect()` and execution refuses it with `ConstructionError`. Its first arm
returns the `Merge` statement. There are three kinds of row, each with its own
method: `when_matched` takes a target row the condition paired with a source
row, `when_not_matched` a source row it paired with none, and
`when_not_matched_by_source` a target row no source row matched. Each takes an
action and an optional `condition=`, which adds `AND condition` to the arm.

Actions are typed by the row they take, as in Rust. A target row is updated
(`MergeUpdate`), deleted (`MatchedAction.Delete`) or left alone
(`MatchedAction.DoNothing`). A source row is inserted (`MergeInsert`, or
`NotMatchedAction.InsertDefaultValues`) or skipped (`NotMatchedAction.DoNothing`).
Anything else raises `TypeError`, so an insert for a target row cannot be
built. `MergeUpdate(column, value)` and `MergeInsert(column, value)` take
their first pair at construction, so neither is ever empty, and
`.and_value(column, value)` adds another. A column is a bare name.
`MergeInsert.overriding(Overriding.SystemValue)` or `Overriding.UserValue`
writes `OVERRIDING ..` for identity columns.

Within a kind, a row takes the first conditional arm whose condition holds, in
call order, and otherwise the kind's one unconditional arm. The unconditional
arm always renders after the conditional ones, the only place PostgreSQL
accepts it, and a later unconditional arm of the same kind replaces it. A row
that `DoNothing` left alone is not returned.

`returning(*items, old_as=, new_as=)` is the statement's RETURNING list, as for
the other writes. `returning_action()` adds `merge_action()` as its first
column: `INSERT`, `UPDATE` or `DELETE` for each row. It has no expression form,
because PostgreSQL resolves it only in a MERGE's RETURNING list. A column both
the target and the source have must be qualified (`42702`), and `*` is the
source's columns followed by the target's. `only()` writes `ONLY` before the
target, leaving inheriting tables alone.

The source is a `Table`. A query as the source is a common table expression:
`With(name, select)` names it, `merge.with_(With(..))` attaches it, and the
merge reads `Table(name)`. `With(name, query).cte(name, query)` adds more. A
MERGE is itself a common table expression's body: `Select(...).with_(With("m",
merge))` reads the rows its RETURNING list yields. A value in a CTE's own
`SELECT` list is assigned to no column, so a bound one needs a cast.

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
`Compiled` objects and explicit `RawSQL`, and rejects ordinary strings and a
`PendingMerge`. These
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
