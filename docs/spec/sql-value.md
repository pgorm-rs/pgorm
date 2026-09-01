# SQL Value and Base Type Vocabulary

This section specifies the value container (`pgorm-query/src/value.rs`) and the
base type vocabulary (`pgorm-query/src/types.rs`, plus `ColumnType`/`StringLen`
from `pgorm-query/src/table/column.rs`) that the query builder, DDL builders and
codegen all share. Rules capture current behaviour of the PostgreSQL-only fork,
including panic semantics and quirks inherited from sea-query.

## The Value container

> [spec:pgorm:def:sql.value+1]
> `Value` is the single enum container for all SQL values. Every variant wraps
> an `Option` of its payload; `None` encodes SQL NULL while preserving the type
> tag. Payloads larger than one pointer are boxed so the enum stays small:
> `Bool(Option<bool>)`, `TinyInt(i8)`, `SmallInt(i16)`, `Int(i32)`,
> `BigInt(i64)`, `Unsigned(u32)`, `BigUnsigned(u64)`, `Float(f32)`,
> `Double(f64)`, `String(Box<String>)`,
> `Char(char)`, `Bytes(Box<Vec<u8>>)`, `Json(Box<serde_json::Value>)`,
> `ChronoDate(Box<NaiveDate>)`, `ChronoTime(Box<NaiveTime>)`,
> `ChronoDateTime(Box<NaiveDateTime>)`, `ChronoDateTimeUtc(Box<DateTime<Utc>>)`,
> `ChronoDateTimeLocal(Box<DateTime<Local>>)`,
> `ChronoDateTimeWithTimeZone(Box<DateTime<FixedOffset>>)`, `Uuid(Box<Uuid>)`,
> `Decimal(Box<Decimal>)`, `Array(ArrayType, Option<Box<Vec<Value>>>)`,
> `Vector(Box<pgvector::Vector>)`, `IpNetwork(Box<IpNetwork>)` and
> `MacAddress(Box<MacAddress>)`.
>
> There are no `u8` or `u16` variants. Postgres has no unsigned integer
> types, so the MySQL-era `TinyUnsigned`/`SmallUnsigned` spellings — whose
> binding path panicked at runtime — were removed outright; a `u8` or `u16`
> is now a compile error wherever a `Value` is required. The two surviving
> unsigned variants both have a Postgres meaning: `Unsigned` (u32) is the
> `OID` carrier (see `[spec:pgorm:sem:exec.decode.u32-oid]`) and
> `BigUnsigned` (u64) is how `LIMIT`/`OFFSET` counts reach the builder.
>
> Unlike upstream sea-query, none of these variants are feature-gated in this
> fork: chrono, serde_json, rust_decimal, uuid, ipnetwork, mac_address and
> pgvector are unconditional dependencies of `pgorm-query`, so every variant is
> always compiled in.
>
> `Value` implements `PartialEq` (derived), a blanket `Eq`, and `Hash` — the
> float variants hash via `to_bits()` (with `None` hashed as a zero byte) and
> `Vector` hashes its `f32` slice bitwise, so `Value` is usable as a map key
> even though the float variants keep IEEE `NaN != NaN` equality semantics.
> `Display` renders the value as a Postgres SQL literal by delegating to
> `QueryBuilder.value_to_string`.

> [spec:pgorm:def:sql.value.conversions+1]
> `From<T> for Value` covers the Rust primitives one-to-one: `bool`→`Bool`,
> `i8`→`TinyInt`, `i16`→`SmallInt`, `i32`→`Int`, `i64`→`BigInt`,
> `u32`→`Unsigned`, `u64`→`BigUnsigned`, `f32`→`Float`, `f64`→`Double`,
> `char`→`Char`. `u8` and `u16` have no conversion, no `Nullable` and no
> `ValueType` impl — they are not part of the value vocabulary at all.
> `&str`, `&String`, `String` and `Cow<'_, str>` all convert to `String`
> (owned, boxed); `&[u8]` and `Vec<u8>` convert to `Bytes`. `serde_json::Value`,
> `NaiveDate`, `NaiveTime`, `NaiveDateTime`, `Decimal`, `Uuid`, `IpNetwork`,
> `MacAddress` and `pgvector::Vector` convert to their same-named variants.
>
> `DateTime<Utc>` and `DateTime<Local>` map to `ChronoDateTimeUtc` /
> `ChronoDateTimeLocal` directly. `DateTime<FixedOffset>` is rebuilt via
> `DateTime::from_naive_utc_and_offset(x.naive_utc(), x.offset().fix())` before
> boxing into `ChronoDateTimeWithTimeZone`. The `uuid::fmt` wrapper types
> (`Braced`, `Hyphenated`, `Simple`, `Urn`) convert into the plain `Uuid`
> variant via `into_uuid()`; converting back out re-applies the corresponding
> format accessor.
>
> `From<Option<T>>` exists for any `T: Into<Value> + Nullable`: `Some(v)`
> converts `v`, `None` produces the typed null `T::null()`. There is no
> `From<Vec<u8>> for Value::Array` — `u8` vectors always become `Bytes` (see
> `[spec:pgorm:def:sql.value.array]`).

> [spec:pgorm:def:sql.value.value-type]
> `ValueType` is the extraction/reflection trait implemented by every Rust type
> that maps into `Value`. `try_from(v: Value) -> Result<Self, ValueTypeErr>`
> succeeds only when `v` is the matching variant with a `Some` payload
> (`ValueTypeErr` displays as "Value type mismatch"). `unwrap` and
> `expect(msg)` are `try_from` followed by `Result::unwrap`/`expect`.
> `type_name()` returns the Rust type name, `array_type()` the matching
> `ArrayType` tag, and `column_type()` the default `ColumnType` for schema
> generation (e.g. `String`→`String(StringLen::None)`, `Vec<u8>`→
> `VarBinary(StringLen::None)`, `char`→`Char(None)`, `Decimal`→`Decimal(None)`,
> `DateTime<Utc>`/`Local`/`FixedOffset`→`TimestampWithTimeZone`,
> `IpNetwork`→`Inet`, `MacAddress`→`MacAddr`, `Vector`→`Vector(None)`).
>
> `Nullable` provides `null() -> Value`, the typed-NULL constructor used by the
> `Option` conversions. `ValueType for Option<T>` returns `Ok(None)` when the
> input equals `T::null()` and otherwise delegates to `T::try_from`, so a
> `None` payload of the right variant round-trips to `Option::None` while a
> wrong-variant value still errors.
>
> `Value::unwrap::<T>()` and `Value::expect::<T>(msg)` are inherent
> conveniences delegating to `T::unwrap` / `T::expect`.

> [spec:pgorm:sem:sql.value.accessor-panics]
> The non-`try` accessors panic rather than error. `ValueType::unwrap` (and
> therefore `Value::unwrap`) panics on any variant or nullability mismatch.
> The `is_*`/`as_ref_*` inherent accessor pairs (`is_json`/`as_ref_json`,
> `is_chrono_date`/`as_ref_chrono_date`, the other chrono accessors,
> `is_decimal`/`as_ref_decimal`, `is_uuid`/`as_ref_uuid`,
> `is_array`/`as_ref_array`, `is_ipnetwork`/`as_ref_ipnetwork`,
> `is_mac_address`/`as_ref_mac_address`) return `Option<&T>` (`None` for SQL
> NULL of the right variant) but panic with messages like `not Value::Json`
> when called on a different variant. `chrono_as_naive_utc_in_string` panics
> with `not chrono Value` on non-chrono variants and stringifies the UTC-naive
> form of zoned values. `as_ipaddr` panics on non-`IpNetwork` values and
> returns the network address. `decimal_to_f64` returns
> `Option<f64>` via `to_f64().unwrap()` on the payload.

## Arrays

> [spec:pgorm:def:sql.value.array]
> `ArrayType` is the element-type tag carried by `Value::Array`; its variants
> mirror the scalar `Value` variants (`Bool` through `Bytes`, `Json`, the six
> chrono tags, `Uuid`, `Decimal`, `IpNetwork`, `MacAddress`). There is no
> nested-array tag and no `Vector` tag — `ValueType::array_type()` for
> `pgvector::Vector` is `unimplemented!` and panics.
>
> `Vec<T>` converts to `Value::Array(T::array_type(), ...)` only for `T`
> implementing the `NotU8` marker trait (all supported element types except
> `u8`), because `Vec<u8>` is claimed by the `Bytes` conversion. `Nullable for
> Vec<T>` produces `Array(tag, None)`. `ValueType for Vec<T>` requires the
> stored tag to equal `T::array_type()` and then unwraps every element —
> a mismatched element inside the vector panics rather than erroring.
> `ValueType::column_type()` for `Vec<T>` is
> `ColumnType::Array(Arc::new(T::column_type()))`.

## Value tuples

> [spec:pgorm:def:sql.value.tuple]
> `ValueTuple` represents an ordered tuple of values for composite keys and
> VALUES lists: `One(Value)`, `Two(Value, Value)`, `Three(Value, Value,
> Value)` or `Many(Vec<Value>)`. `IntoValueTuple` is implemented for any
> single `Into<Value>` (producing `One`), for 2- and 3-tuples (producing
> `Two`/`Three` in field order), and for 4- through 12-tuples (producing
> `Many` in field order). `IntoIterator for ValueTuple` yields the values in
> that same positional order.
>
> `FromValueTuple` inverts the mapping and is arity-strict: the scalar impl
> panics unless given `One`, the pair impl unless given `Two`, the triple impl
> unless given `Three`, and the 4..=12 impls unless given `Many` with exactly
> the expected length (panic message `not ValueTuple::Many with length of N`).
> Element extraction uses `ValueType::unwrap`, so a type mismatch at any
> position also panics.
>
> `Values` is a `Vec<Value>` newtype with `iter()` and `IntoIterator`, used to
> carry a statement's collected parameters.

## JSON conversion

> [spec:pgorm:sem:sql.value.to-json]
> `sea_value_to_json_value` (name retained from sea-query) converts a `&Value`
> into a `serde_json::Value`. `None` payloads of the non-chrono variants map
> to `Json::Null`. Booleans and all integer/float variants map to native JSON
> values; `String` and `Char` map to JSON strings; `Json` clones the payload
> through; `Uuid` becomes its hyphenated string; `Decimal` converts via
> `to_f64().unwrap()`; `Bytes` becomes a JSON string via
> `from_utf8(..).unwrap()` and therefore panics on non-UTF-8 payloads;
> `Array` maps recursively to a JSON array; `Vector` becomes a JSON array of
> numbers.
>
> All chrono variants — including their `None` payloads, which are not covered
> by the null arms — plus `IpNetwork(Some)` and `MacAddress(Some)` are
> stringified through `QueryBuilder.value_to_string`. Consequently a chrono
> NULL becomes the JSON string `"NULL"` (not `Json::Null`), and the rendered
> strings include the surrounding single quotes of the SQL literal (e.g.
> `"'2020-01-01'"`). This is current behaviour, inherited and unchanged.

> [spec:pgorm:sem:sql.value.render]
> `QueryBuilder.value_to_string` renders a `Value` as an inline Postgres
> literal (also used by `Display for Value` and by `SqlWriter for String` when
> a statement is built without parameter binding). `None` payloads render as
> `NULL`; booleans as `TRUE`/`FALSE`; numerics and `Decimal` in plain decimal
> form. Strings and chars are single-quoted after `escape_string`, switching
> to the `E'...'` form when the escaped text contains a backslash. `Bytes`
> renders as `'\xHEX...'`. Chrono values render quoted with formats
> `%Y-%m-%d`, `%H:%M:%S`, `%Y-%m-%d %H:%M:%S` and, for the zoned variants,
> `%Y-%m-%d %H:%M:%S %:z`. `Uuid`, `IpNetwork` and `MacAddress` render as
> quoted display strings. `Array` renders as `ARRAY [elem,...]` recursively
> and `Vector` as a quoted bracket literal `'[v1,v2,...]'`.

## Identifier machinery

> [spec:pgorm:def:sql.types]
> `Iden` is the identifier trait (bounded `Send + Sync`): implementors provide
> `unquoted`, and the trait derives `to_string` (unquoted), `quoted(q)` —
> which doubles any embedded quote character — and `prepare`, which writes the
> identifier wrapped in the `Quote` pair. The Postgres `QueryBuilder` uses
> `Quote(b'"', b'"')`, so identifiers render double-quoted with embedded `"`
> doubled. `IdenStatic` adds `as_str() -> &'static str` for `Copy + 'static`
> identifiers.
>
> `DynIden` is `SeaRc<dyn Iden>`, where `SeaRc` is a transparent wrapper over
> `std::sync::Arc` (`RcOrArc` is re-exported as `Arc`). `SeaRc<dyn Iden>`
> equality compares the trait-object vtable pointer and the unquoted string,
> so two idens are equal only when they are the same concrete type rendering
> the same text. `IntoIden` converts any `Iden + 'static` (or an existing
> `DynIden`) into a `DynIden`; `IdenList` is implemented for a single iden and
> for 2- and 3-tuples, yielding `DynIden`s in order.
>
> `Alias` wraps an arbitrary `String` as an identifier; `NullAlias` renders as
> the empty string.

> [spec:pgorm:def:sql.types.column-ref]
> `ColumnRef` has five forms: `Column(DynIden)`, `TableColumn(DynIden,
> DynIden)`, `SchemaTableColumn(DynIden, DynIden, DynIden)`, `Asterisk` and
> `TableAsterisk(DynIden)`. `IntoColumnRef` maps a bare iden to `Column`, a
> 2-tuple to `TableColumn`, a 3-tuple to `SchemaTableColumn`, the `Asterisk`
> unit type to `Asterisk`, and `(iden, Asterisk)` to `TableAsterisk`.

> [spec:pgorm:def:sql.types.table-ref]
> `TableRef` has nine forms: `Table`, `SchemaTable`, `DatabaseSchemaTable`,
> the three alias-carrying counterparts (`TableAlias`, `SchemaTableAlias`,
> `DatabaseSchemaTableAlias`), and three value-producing forms —
> `SubQuery(SelectStatement, alias)`, `ValuesList(Vec<ValueTuple>, alias)` and
> `FunctionCall(FunctionCall, alias)`. `IntoTableRef` maps a bare iden to
> `Table`, a 2-tuple to `SchemaTable` and a 3-tuple to
> `DatabaseSchemaTable`. `TableRef::alias(a)` adds or replaces the alias,
> upgrading plain forms to their alias-carrying counterparts and replacing the
> alias on forms that already carry one.

> [spec:pgorm:def:sql.types.opers]
> `UnOper` has the single variant `Not`. `BinOper` enumerates the binary
> operator vocabulary: logical `And`/`Or`; pattern `Like`/`NotLike` plus
> Postgres `ILike`/`NotILike` and `Escape`; `Is`/`IsNot`; `In`/`NotIn`;
> `Between`/`NotBetween`; comparisons `Equal`, `NotEqual`, `SmallerThan`,
> `GreaterThan`, `SmallerThanOrEqual`, `GreaterThanOrEqual`; arithmetic
> `Add`/`Sub`/`Mul`/`Div`/`Mod`; shifts `LShift`/`RShift`; `As`;
> full-text/containment `Matches`, `Contains`, `Contained`; `Concatenate`,
> `Overlap`; pg_trgm similarity operators (`Similarity`, `WordSimilarity`,
> `StrictWordSimilarity` and their `*Distance` forms); JSON access
> `GetJsonField` (`->`) and `CastJsonField` (`->>`); regex `Regex` (`~`) and
> `RegexCaseInsensitive` (`~*`); pgvector distances `EuclideanDistance`,
> `NegativeInnerProduct`, `CosineDistance`; and an escape hatch
> `Custom(&'static str)`.

## Column type vocabulary

> [spec:pgorm:def:sql.types.column-type+1]
> `ColumnType` (in `pgorm-query/src/table/column.rs`, `#[non_exhaustive]`) is
> the type vocabulary shared by DDL generation, `ValueType::column_type()` and
> codegen: `Char(Option<u32>)`, `String(StringLen)`, `Text`, `Blob`,
> `TinyInteger`, `SmallInteger`, `Integer`, `BigInteger`, `Unsigned`,
> `BigUnsigned`, `Float`, `Double`,
> `Decimal(Option<(u32, u32)>)`, `DateTime`, `Timestamp`,
> `TimestampWithTimeZone`, `Time`, `Date`, `Year`, `Interval(Option<PgInterval>,
> Option<u32>)`, `Binary(u32)`, `VarBinary(StringLen)`, `Bit(Option<u32>)`,
> `VarBit(u32)`, `Boolean`, `Money(Option<(u32, u32)>)`, `Json`, `JsonBinary`,
> `Uuid`, `Custom(DynIden)`, `Enum { name, variants }`,
> `Array(Arc<ColumnType>)`, `Vector(Option<u32>)`, `Cidr`, `Inet`, `MacAddr`
> and `LTree`. `Year` survives from the multi-backend ancestry but is
> unrenderable on Postgres (see `[spec:pgorm:sem:sql.ddl.panics]`). There is
> no `TinyUnsigned` or `SmallUnsigned`: Postgres has no unsigned integer
> types, and the `ColumnDef::tiny_unsigned`/`small_unsigned` builders were
> removed with the variants.
>
> `StringLen` parameterises varchar/varbinary length: `N(u32)`, `Max`, or the
> default `None`. `PgInterval` enumerates the thirteen interval field
> qualifiers (`Year` through `MinuteToSecond`); it implements `Display` as the
> SQL keywords (`YEAR TO MONTH`, ...) and a case-insensitive
> `TryFrom<&str>` inverse.
>
> `ColumnType` equality compares parameters for the parameterised variants,
> compares `Custom` and `Enum` by rendered identifier strings (and variant
> lists), compares `Array` element types recursively, and otherwise compares
> enum discriminants. Convenience constructors: `ColumnType::custom(str)`,
> `ColumnType::string(Option<u32>)` and `ColumnType::var_binary(u32)`.
