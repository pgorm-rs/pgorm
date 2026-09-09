# Native values

`pgorm.Value` owns a `pgorm::pgorm_query::Value` in the extension. Values are
immutable and reusable. `value.value` returns an independent Python value;
mutating a returned list or dictionary does not change the native value.

```python
from decimal import Decimal
from pgorm import TypeName, Value

small = Value(42, "i16")
money = Value(Decimal("19.9900"))
missing_text = Value.null("text")
json_null = Value.json(None)           # a JSON value, not SQL NULL
missing_json = Value.null("json")     # SQL NULL with the JSON tag
empty = Value.array("i32", [])
missing_array = Value.array("i32", None)
mood = TypeName("mood", schema="application")
label = Value("calm", mood)
labels = Value.array(mood, [label, None])
```

`Value(data, kind)` explicitly chooses a Rust variant. `Value(data)` infers
`bool`, `i64`, `f64`, `text`, `bytes`, `decimal`, `uuid`, `date`, `time`, or
naive `datetime` from exact Python types. Inference rejects `None`, containers,
aware datetimes and custom objects. It never chooses an integer type from a
boolean, calls an arbitrary object's string conversion, or promotes an
overflowing integer to another width. Passing an existing `Value` clones it;
supplying a different kind with that value raises `ConstructionError`.

| Kind | Rust variant | Python input/output and limits |
| --- | --- | --- |
| `bool` | `Bool` | `bool` |
| `i8`, `i16`, `i32`, `i64` | `TinyInt`, `SmallInt`, `Int`, `BigInt` | `int` within the chosen signed range |
| `u32`, `u64` | `Unsigned`, `BigUnsigned` | `int` in 0 through 2^width − 1 |
| `f32`, `f64` | `Float`, `Double` | `float`; signed zero, infinities and representable NaN payloads retained |
| `text`, `char` | `String`, `Char` | `str`; `char` is exactly one Unicode scalar |
| `bytes` | `Bytes` | `bytes`, including zero and non-UTF-8 bytes |
| `decimal` | `Decimal` | `decimal.Decimal`, exact 96-bit coefficient and scale 0–28 |
| `uuid` | `Uuid` | `uuid.UUID` |
| `json` | `Json` | JSON-compatible values; see below |
| `date` | `ChronoDate` | `datetime.date`, years 1–9999 |
| `time` | `ChronoTime` | `datetime.time` with no timezone and `fold=0` |
| `datetime` | `ChronoDateTime` | naive `datetime.datetime` with `fold=0` |
| `datetime_utc` | `ChronoDateTimeUtc` | aware `datetime.datetime` with zero UTC offset |
| `datetime_fixed` | `ChronoDateTimeWithTimeZone` | aware `datetime.datetime`, preserving its offset and instant |
| `datetime_local` | `ChronoDateTimeLocal` | aware `datetime.datetime` matching the machine's local timezone at that instant |
| `ipnetwork` | `IpNetwork` | IP/prefix string; native formatting on output, host bits retained |
| `mac_address` | `MacAddress` | exactly six bytes |
| `vector` | `Vector` | list/tuple of exact `f32` values; returns a list |
| `array` | `Array` | `Value.array(element_kind, list_or_tuple_or_none)` |
| `enum` | `String` plus `TypeName` | `Value(label_or_none, type_name)` |

Every scalar supports `Value.null(kind)` or `Value(None, kind)`. These are
typed SQL NULL values. `Value.json(None)` creates JSON null; it has
`is_null == False`. Array type identity survives empty and NULL arrays.
Array items may be compatible native values, Python values or SQL NULLs.
Use `Value.json(None)` as an array item to distinguish it from SQL NULL.
`array.items()` returns tagged values; `array.value` returns plain values.
Nested and heterogeneous arrays are rejected.

Enum labels retain their schema and type name separately from their Rust
string payload. `type_name` and `element_type` expose that identity. Type-name
parts are 1–63 UTF-8 bytes without NUL; case, punctuation and Unicode remain
unchanged. Lowering uses Rust's identifier-only `TypeName` API. Neither type
declaration nor value construction performs DDL.

`f32` conversion rejects a Python float if narrowing and widening changes its
bits. For example, `1.5` is exact but `0.1` is rejected. Applications may
explicitly round with `struct` before constructing a value when that is their
intention. Rust-to-Python conversion also rejects a NaN payload that cannot
survive widening; `snapshot()` remains available to inspect those native bits.
Vector elements follow the same policy.

Decimal conversion never passes through float or the active Decimal context.
Non-finite values, more than 29 coefficient digits and exponents outside
−28 through 28 are rejected; the remaining coefficient must fit Rust's
96-bit limit exactly. Positive exponents expand to scale zero. Representable
negative zero and trailing fractional zeros retain their sign and scale.

JSON accepts `None`, exact `bool`, `int`, finite `float`, `str`, lists, tuples
and dictionaries with string keys. Integers must fit Rust's signed 64-bit or
unsigned 64-bit JSON number representation. Tuples become JSON arrays. Cyclic
objects and nesting deeper than 64 levels are rejected. Decimal, UUID and
date/time values require application-selected JSON encodings. No implicit
string conversion or non-finite JSON number is used.

Temporal values preserve microseconds. Chrono leap seconds, finer Rust
precision and subsecond timezone offsets raise errors. A timezone-aware
input must select a temporal kind. `datetime_fixed` captures its resolved
offset, including a `zoneinfo` daylight-saving fold; output uses a fixed-offset
timezone with `fold=0`, preserving the instant and local clock fields.
Timezone names and transition rules are not stored in Rust's fixed-offset
variant. `datetime_utc` requires zero offset. `datetime_local` retains the Rust
local variant and rejects inputs whose offset/local time disagrees with the
machine timezone. Local output uses the stored resolved offset.

These are Rust value representation limits. PostgreSQL may impose additional
limits when binding a value to a particular database type; the execution API
reports a database/conversion error. A native `u64`, for example, is not a
claim that PostgreSQL has an unsigned 64-bit integer column type.

## Inspection and equality

`kind`, `is_null`, `type_name` and `element_type` expose the native tags.
Equality compares Rust values and enum identities, including the Rust
bitwise float semantics: equal NaN payloads compare equal and positive and
negative zero compare unequal. Native values are not hashable.

`snapshot()` produces a detached JSON-compatible dictionary with `version: 1`,
`type`, `sql_null` and `data`. Arrays recursively retain each element's tag.
Integers and decimals use strings; floats and vector elements use hexadecimal
IEEE bit patterns, preserving infinities, signed zero and NaN payloads without
nonstandard JSON numbers. Bytes use byte lists, UUIDs/IP networks use native
text, and temporal text retains native precision and offsets. This is an
inspection format; it is not SQL or a public deserialization API.

Input errors raise `ConstructionError`; unrepresentable native outputs raise
`DecodeError`. Neither path silently substitutes `None` or truncates values.

Run the installed-wheel conversion suite without PostgreSQL:

```sh
target/python-check/bin/python -m unittest discover \
  -s pgorm-python/tests -p 'test_values.py' -v
```
