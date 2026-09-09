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
| `Identifier(name)` | owned `Alias`; one validated identifier part |
| `col(name, table=..., schema=...)` | `Expr::col` with `ColumnRef` qualification |
| `bind(value)` | `Expr::value`; a separately collected parameter |
| `literal(value)` | `SimpleExpr::Constant`; rendered by Rust as a literal |
| `TypeName(name, schema=...)` | identifier-only Rust `TypeName` |
| `expr.cast(type_name, array=False)` | `SimpleExpr::cast_as_type` |
| `LikePattern(pattern, escape=...)` | `LikeExpr::new` and optional `escape` |
| `expr.as_(name)` | owned projection expression plus validated `Alias` |
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
| `contains_text(text)` | `Func::cust("strpos").args(...) > 0` |
| `ends_with(text)` | `Func::cust("right").args(expression, Func::char_length(text)) == text` |

There is no second escaping implementation in the bindings. These helpers
use PostgreSQL string functions and ordinary Rust value expressions.

For pattern semantics, use `expr.like(LikePattern(...))`, `not_like`, `ilike`
or `not_ilike`. The pattern and optional single-character ESCAPE clause
are handled by Rust's `LikeExpr`; a plain string is rejected at this boundary.

`call(name, *arguments)` invokes the named Rust `Func` constructor for:
`lower`, `upper`, `abs`, `char_length`, `count`, `count_distinct`, `sum`, `avg`,
`min`, `max`, `round`, `coalesce`, `random` and `gen_random_uuid`.
`round` accepts one or two arguments, `coalesce` one or more, `random` and
`gen_random_uuid` none, and the others one. Unsupported names or argument
counts raise `UnsupportedCapabilityError`. The capability manifest records
these signatures in `expression_functions`. This name selects a supported
function constructor; it is not raw SQL.

The installed module's capability manifest maps each logical operation
to its Rust API. Arithmetic names such as `expr.add` refer to Python operators
(`Expr.__add__`). Projection/ordering objects retain their state until a
statement consumes a clone. They cannot accidentally become predicates.

Run installed expression tests and Rust parity tests from the repository root:

```sh
target/python-check/bin/python -m unittest discover \
  -s pgorm-python/tests -p 'test_expressions.py' -v
PYO3_PYTHON="$PWD/target/python-dev/bin/python" cargo test \
  --manifest-path pgorm-python/Cargo.toml --target-dir target --locked --lib
```
