# Expressions

Python expressions own `pgorm_query::SimpleExpr` values. Every operation clones
its inputs and composes another Rust expression. An expression can be reused
across queries and Python threads in the supported interpreter. Composing or
inspecting it does not connect to a database or invoke a compiler.

```python
from pgorm import Condition, TypeName, Value, bind, call, col, literal

name = col("name", table="account", schema="application")
predicate = Condition.all(
    col("active", table="account") == True,
    Condition.any(name.starts_with("A%"), name.is_null()),
)
computed = call("lower", name)
mixed_values = bind(Value(7, "i16")) + literal(3)

assert mixed_values.inspect().sql == "SELECT $1 + 3"
assert mixed_values.inspect().params == [Value(7, "i16")]
```

`Expr.inspect()` places the expression in a Rust `Query::select().expr(...)`
and calls `build()`. `Condition.inspect()` uses `SELECT TRUE` with the condition
in `cond_where`. Both return a `Compiled` object with `sql` and `params`.
`params` are detached `Value` wrappers of the Rust builder's ordered wire
parameters. Their `snapshot()` method preserves the native value tags.
Qualified enum identity lives in the expression's Rust cast and appears in
the SQL; its wire parameter retains the underlying Rust string/array tag.

## Input paths

| Python | Corresponding Rust boundary |
| --- | --- |
| `Identifier(name)` | owned `Name`; one validated identifier part |
| `col(name, table=..., schema=...)` | `Expr::col` with `ColumnRef` qualification |
| `bind(value)` | `Expr::value`; a separately collected parameter |
| `literal(value)` | `SimpleExpr::Constant`; rendered by Rust as a literal |
| `TypeName(name, schema=...)` | identifier-only Rust `TypeName` |
| `expr.cast(type_name, array=False)` | `SimpleExpr::cast_as_type` |
| `LikePattern(pattern, escape=...)` | `LikeExpr::new` and optional `escape` |
| `expr.as_(name)` | owned projection expression plus validated `Name` |
| `expr.asc()` / `expr.desc()` | owned expression plus Rust `Order` |
| `OrderBy(expr, Direction.Asc, nulls=Nulls.First)` | typed direction and `NullOrdering` |

Column, table, schema and alias names may be strings or `Identifier` objects.
Each part is 1–63 UTF-8 bytes without NUL. Dots are part of an identifier, not
qualification syntax: `col("a.b")` is one column name; use `table="a"` to
qualify column `b`. Type names are separate objects. Case, quotes, percent
signs, backslashes and Unicode reach the Rust builders unchanged.

Ordinary scalar operands are converted through `Value` and bound by default.
Use `literal(...)` to retain a literal node within an otherwise parameterized
expression. `bind` on an enum value carries its qualified Rust cast; enum
arrays carry the corresponding array cast. Explicit casts keep Rust's
source-typed parameter pins, such as `$1::text`, where the renderer uses them.

The statement builder's ordinary parameters have no lifetime brand and are
safe to reuse across independent statement builds: each Rust build starts
its own parameter collection. This does not expose or extend the borrowed
`pipeline::Binder` lifetime. Pipeline scopes are a separate API.

## Operators and conditions

Use `==`, `!=`, `<`, `<=`, `>`, `>=`, or the methods `eq`, `ne`, `lt`, `lte`,
`gt`, `gte`. Arithmetic uses `+`, `-`, `*`, `/` and `%` with the expression on
the left. These lower to the corresponding Rust `BinOper` through
`SimpleExpr::binary`. For a scalar on the left, write `bind(scalar)` or
`literal(scalar)` explicitly.

Combine expressions with `&`, `|` and `~`. Parenthesize comparisons because
Python's operator precedence still applies. SQL expressions, conditions,
projection aliases and orderings raise `ConstructionError` on Python truth
testing. Python `and`, `or`, `not`, chained comparisons and implicit truth
tests cannot compose SQL.

`Condition.all(*terms)` and `Condition.any(*terms)` invoke Rust's `Condition`
constructors, accepting expressions and nested conditions. `condition.add`
returns a new condition. Empty All is true; empty Any is false. Grouping,
flattening and negation follow the Rust condition implementation.

`is_null`, `is_not_null`, `between`, `not_between`, `is_in` and `is_not_in`
invoke the corresponding Rust `Expr` methods. Membership accepts a list or
tuple of values/expressions, including an empty list. The empty-list
implementation remains Rust's constant comparison, which `build()` renders
as `$1 = $2` with `("a", "b")` for IN and `("a", "a")` for NOT IN.
Use `is_null()` explicitly; an equality against a typed NULL retains ordinary
SQL equality semantics. `tuple_expr(*items)` constructs a nonempty Rust tuple.

## Text and functions

`starts_with`, `ends_with` and `contains_text` treat their arguments as text,
including `%`, `_`, backslashes and quotes. They lower to these Rust builders:

| Helper | Rust composition |
| --- | --- |
| `starts_with(text)` | `Func::starts_with(expression, text)` |
| `contains_text(text)` | `Func::named("strpos").args(...) > 0` |
| `ends_with(text)` | `Func::named("right").args(expression, Func::char_length(text)) == text` |

There is no second escaping implementation in the bindings. These helpers
use PostgreSQL string functions and ordinary Rust value expressions.

For pattern semantics, use `expr.like(LikePattern(...))`, `not_like`, `ilike`
or `not_ilike`. The pattern and optional single-character ESCAPE clause
are handled by Rust's `LikeExpr`; a plain string is rejected at this boundary.

`call(name, *arguments)` invokes the named Rust `Func` constructor for:
`lower`, `upper`, `abs`, `char_length`, `count`, `count_distinct`, `sum`, `avg`,
`min`, `max`, `round`, `coalesce`, `random`, `gen_random_uuid`, and PostgreSQL
18's `uuidv4`, `uuidv7`, `uuid_extract_timestamp` and `uuid_extract_version`.
`round` accepts one or two arguments, `coalesce` one or more, `random`,
`gen_random_uuid` and `uuidv4` none, `uuidv7` none or one (the `interval` its
embedded time is shifted by, Rust's `Func::uuidv7_shifted`), and the others
one. A call is an expression wherever one is taken, so
`ColumnDef("id", "uuid").default(call("uuidv7"))` gives a table a time-ordered
key. Unsupported names or argument
counts raise `UnsupportedCapabilityError`. The capability manifest records
these signatures in `expression_functions`. This name selects a supported
function constructor; it is not raw SQL.

The installed module's capability manifest maps each logical operation
to its Rust API. Arithmetic names such as `expr.add` refer to Python operators
(`Expr.__add__`). Projection/ordering objects retain their state until a
statement consumes a clone. They cannot accidentally become predicates.

## SQL/JSON

PostgreSQL's SQL/JSON functions, constructors and `IS JSON` are functions
returning an `Expr`. Each clause is a keyword argument applied through the
Rust builder's own method, so the SQL is `pgorm_query`'s:

```python
from pgorm import (JsonDefault, JsonQueryBehavior, JsonValueBehavior, col,
                   format_json, json_exists, json_object, json_query, json_value)

doc = col("doc")
size = json_value(doc, "$.size", returning="integer",
                  on_empty=JsonDefault(0), on_error=JsonValueBehavior.Error)
assert size.inspect().sql == (
    'SELECT JSON_VALUE("doc", CAST($1::text AS jsonpath) RETURNING integer '
    "DEFAULT 0 ON EMPTY ERROR ON ERROR)"
)
tagged = json_exists(doc, "$.tags[*] ? (@ == $Tag)", passing={"Tag": "blue"})
tags = json_query(doc, "$.tags[*]", shaping="with_wrapper",
                  on_empty=JsonQueryBehavior.EmptyArray)
row = json_object({"id": col("id"), "body": format_json(col("body"))},
                  absent_on_null=True, returning="jsonb")
```

| Python | Rust builder |
| --- | --- |
| `json_exists(context, path, *, passing, on_error)` | `Func::json_exists` |
| `json_value(context, path, *, passing, returning, on_empty, on_error)` | `Func::json_value` |
| `json_query(context, path, *, passing, returning, shaping, on_empty, on_error)` | `Func::json_query` |
| `json_object(entries, *, absent_on_null, unique_keys, returning)` | `Func::json_object` and `entry` |
| `json_array(*elements, null_on_null, returning)` | `Func::json_array` and `element` |
| `json_array_query(select, *, returning)` | `Func::json_array_query` |
| `json_objectagg(key, value, *, absent_on_null, unique_keys, returning, filter)` | `Func::json_objectagg` |
| `json_arrayagg(value, *, order_by, null_on_null, returning, filter)` | `Func::json_arrayagg` |
| `json_parse(input, *, unique_keys)` | `Func::json` (`JSON(..)`) |
| `json_scalar(operand)` | `Func::json_scalar` |
| `json_serialize(input, *, returning)` | `Func::json_serialize` |
| `format_json(operand)` | `Expr::format_json`, a `JsonInput` |
| `is_json(operand, kind=JsonKind.Value, *, unique_keys)` / `is_not_json` | `Expr::is_json` / `is_not_json` |

The path is a `str` that Rust binds as `text` cast to `jsonpath`, so it never
becomes statement text. `PASSING` takes a `dict` from variable name to value.
Each name is an identifier, quoted, so `$Tag` in the path names `"Tag"` exactly.
Values in positions that give them no type (`PASSING`, constructor members,
aggregate operands, `json_scalar`, `IS JSON`, the context) carry their type in
both render paths: `json_scalar(5)` is the JSON number 5, not the string `"5"`.
`format_json` marks an operand as JSON text. It returns a `JsonInput`, accepted
only in the positions SQL/JSON reads JSON, never as an expression.

Each function takes its own behaviour class, so a choice PostgreSQL refuses
cannot be written. `JsonExistsBehavior` is `True_`, `False_` (with the
underscore Python's keywords need), `Unknown` or `Error`. `JsonValueBehavior`
is `Null` or `Error`, and `JsonQueryBehavior` adds `EmptyArray` and
`EmptyObject`. `JsonDefault(value)` is `DEFAULT value` for either. PostgreSQL
refuses a parameter there (`42804`), so Rust writes the value as an escaped
literal in both render paths. A value carrying an enum cast is refused, because
the literal cannot keep the cast.

`json_query`'s `shaping` is one slot: `"with_wrapper"`,
`"with_conditional_wrapper"` or `"omit_quotes"`, because PostgreSQL refuses
`OMIT QUOTES` beside a wrapper. `returning` takes a `DataType` or a built-in
type name. `json_value` refuses `json` and `jsonb` with `ConstructionError`,
since PostgreSQL 18.6 returns `NULL` for every later row once one of those
evaluations is `NULL` (bug #19695); `json_query` reads JSON out of a
document instead. `json_object` takes a `dict` of string keys, bound as
values, or a list of `(key, value)` pairs whose keys may be expressions, so a
repeated key can still be tested against `unique_keys`. `json_arrayagg`'s
`order_by` is a list of `Expr.asc()` / `desc()`. The Rust builder places no
`NULLS FIRST` or `NULLS LAST` there, so an ordering that asks for one is
refused rather than dropped. `filter` takes an expression or a `Condition`.

`JSON(..)` is `json_parse` so that importing it cannot shadow Python's `json`
module. `json_serialize` reads its input through `JSON(..)`, so a `jsonb`
value serializes as its document on PostgreSQL 18.6. Both aggregates also run
as window functions, `json_arrayagg(..).over(window)` (see
[STATEMENTS.md](STATEMENTS.md#window-functions)).

### JSON_TABLE

`json_table` reads rows out of a JSON document. It is a `FROM` item, not an
expression: it returns a `FromItem`, which `Select.from_`, `join` and
`cross_join` take beside a `Table` (see [STATEMENTS.md](STATEMENTS.md)).

```python
from pgorm import Join, JsonQueryBehavior, JsonTableColumn as C, Table, json_table, literal, select

docs = Table("docs", alias="d")
items = json_table(
    docs.col("doc"), "$.items[*] ? (@.n >= $Min)",
    C.ordinality("i"),
    C.value("n", "integer"),
    C.query("tags", "jsonb", on_empty=JsonQueryBehavior.EmptyArray),
    C.exists("flagged", "boolean", path="$.flag"),
    C.nested("$.parts[*]", C.value("part", "text", path="$")),
    alias="jt", passing={"Min": 1},
)
query = select(docs.col("id"), items.col("n"), items.col("part")).from_(docs).from_(items)
labels = json_table(docs.col("doc"), "$.items[*]", C.value("label", "text"), alias="l")
kept = select(docs.col("id"), labels.col("label")).from_(docs).join(labels, literal(True), kind=Join.Left)
```

| Python | Rust builder |
| --- | --- |
| `json_table(context, path, column, *columns, alias, passing, path_name, on_error)` | `Func::json_table`, `column`, `passing`, `path_name`, `on_error`, `alias` |
| `JsonTableColumn.ordinality(name)` | `JsonTableColumn::ordinality` |
| `JsonTableColumn.value(name, kind, *, path, on_empty, on_error)` | `JsonTableColumn::value` |
| `JsonTableColumn.query(name, kind, *, path, shaping, on_empty, on_error)` | `JsonTableColumn::query` |
| `JsonTableColumn.exists(name, kind, *, path, on_error)` | `JsonTableColumn::exists` |
| `JsonTableColumn.nested(path, column, *columns, path_name)` | `JsonTableColumn::nested`, `column`, `path_name` |

The first column and the alias are required arguments, so a table with no
column (`42601` from the server) or no name cannot be built. Like a function in
`FROM`, it is implicitly `LATERAL`: its context may read the columns of the
items before it, after a comma or in a join, and `LEFT JOIN .. ON TRUE` keeps a
row whose document yields nothing. A nested path's rows join their parent's as
an outer join would. `items.col(name)` qualifies a column by the alias, and
`items.star()` selects them all.

Unlike the query functions' paths, these paths are literals: PostgreSQL
refuses a parameter for the root path (`0A000`) and its grammar takes only a
string constant for a column's or a nested path (`42601`). Rust writes each as
an escaped literal, never interpolated; a value the path needs goes through
`passing`, which binds it. Column names, path names, `PASSING` names and the
alias are identifiers, quoted case-exactly. A column without `path` reads
`$."name"`, its own name exactly as written.

Each column kind takes only its own behaviours, as the query function it
reads like does: `value` takes `JsonValueBehavior` or `JsonDefault`, `query`
takes `JsonQueryBehavior` or `JsonDefault` and the same `shaping` slot as
`json_query`, and `exists` takes `JsonExistsBehavior` for `on_error`, with no
`on_empty` because finding nothing is `false`. The table's own `on_error` is a
`JsonTableBehavior`, `Error` or `Empty`, the only two PostgreSQL admits there;
without one, a failing root path yields no rows. The server still refuses
what the builder does not track: a name used twice across the table and its
nested levels (`42712`), and `FORMAT JSON` on a `query` column whose type is
not a string, `json`, `jsonb` or `bytea` (`0A000`).

Run installed expression tests and Rust parity tests from the repository root:

```sh
target/python-check/bin/python -m unittest discover \
  -s pgorm-python/tests -p 'test_expressions.py' -v
PYO3_PYTHON="$PWD/target/python-dev/bin/python" cargo test \
  --manifest-path pgorm-python/Cargo.toml --target-dir target --locked --lib
```
