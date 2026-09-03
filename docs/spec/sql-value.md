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
> `[spec:pgorm:def:sql.value.array+2]`).

> [spec:pgorm:def:sql.value.value-type+2]
> `ValueType` is the extraction/reflection trait implemented by every Rust type
> that maps into `Value`. `try_from(v: Value) -> Result<Self, ValueTypeErr>`
> succeeds only when `v` is the matching variant with a `Some` payload
> (`ValueTypeErr` displays as "Value type mismatch") and is the trait's only
> extraction entry point: there is no panicking `unwrap`/`expect` pair, on the
> trait or as an inherent convenience on `Value`.
> `type_name()` returns the Rust type name, `array_type()` the matching
> `ArrayType` tag, and `column_type()` the default `ColumnType` for schema
> generation (e.g. `String`→`String(StringLen::None)`, `Vec<u8>`→`Bytea`,
> `char`→`Char(None)`, `Decimal`→`Decimal(None)`, `i8`/`i16`→`SmallInteger`,
> `u32`/`u64`→`BigInteger` — the `int8` those values bind as —
> `NaiveDateTime`→`Timestamp`,
> `DateTime<Utc>`/`Local`/`FixedOffset`→`TimestampWithTimeZone`,
> `IpNetwork`→`Inet`, `MacAddress`→`MacAddr`, `Vector`→`Vector(None)`). The
> `ColumnType` a Rust type maps to MUST be one Postgres actually has: no
> mapping may name a width or signedness the server will not honour.
>
> `Nullable` provides `null() -> Value`, the typed-NULL constructor used by the
> `Option` conversions. `ValueType for Option<T>` returns `Ok(None)` when the
> input equals `T::null()` and otherwise delegates to `T::try_from`, so a
> `None` payload of the right variant round-trips to `Option::None` while a
> wrong-variant value still errors.

> [spec:pgorm:sem:sql.value.accessor-panics+1]
> The inherent accessors on `Value` MUST NOT panic. (The rule keeps the id it
> was given when they did: every `as_ref_*` panicked with a message like
> `not Value::Json` on a variant other than its own.)
>
> The `is_*`/`as_ref_*` pairs are `is_json`/`as_ref_json`,
> `is_chrono_date`/`as_ref_chrono_date` and the other five chrono accessors,
> `is_decimal`/`as_ref_decimal`, `is_uuid`/`as_ref_uuid`,
> `is_array`/`as_ref_array`, `is_ipnetwork`/`as_ref_ipnetwork` and
> `is_mac_address`/`as_ref_mac_address`. Each `as_ref_*` returns `Option<&T>`
> borrowed from the payload, and its `None` is ambiguous by design: it means
> either SQL NULL of the accessor's own variant or a value of some other
> variant entirely, and the return value cannot distinguish them. The `is_*`
> predicate of the pair is the discriminator for a caller that needs the
> distinction — `is_array` is true for an `Array` of any element tag — and
> `ValueType::try_from` is the typed extraction that reports a mismatch as
> `ValueTypeErr`.
>
> `chrono_as_naive_utc_in_string` stringifies any non-NULL chrono variant, in
> the UTC-naive form for the three zoned ones, and returns `None` for both a
> NULL chrono variant and a non-chrono one. `as_ipaddr` returns the network
> address of a non-NULL `IpNetwork` and `None` otherwise. `decimal_to_f64`
> returns the payload of a non-NULL `Decimal` through `to_f64`, and `None` if
> the value is not a `Decimal`, is NULL, or has no `f64` representation.

## Arrays

> [spec:pgorm:def:sql.value.array+2]
> `ArrayType` is the element-type tag carried by `Value::Array`; its variants
> mirror the scalar `Value` variants (`Bool` through `Bytes`, `Json`, the six
> chrono tags, `Uuid`, `Decimal`, `IpNetwork`, `MacAddress`, `Vector`). There is
> no nested-array tag. `ValueType::array_type()` is total — `pgvector::Vector`
> answers `ArrayType::Vector` rather than panicking — but `Vector` does not
> implement `NotU8`, so `Vec<Vector>` has no `From`/`ValueType` impl and the tag
> reaches no `Value` through the generic array conversions.
>
> `Vec<T>` converts to `Value::Array(T::array_type(), ...)` only for `T`
> implementing the `NotU8` marker trait (all supported element types except
> `u8`), because `Vec<u8>` is claimed by the `Bytes` conversion. `Nullable for
> Vec<T>` produces `Array(tag, None)`. `ValueType for Vec<T>` requires the
> stored tag to equal `T::array_type()` and then converts every element through
> `T::try_from`, so a mismatched element inside the vector surfaces as
> `ValueTypeErr` rather than panicking.
> `ValueType::column_type()` for `Vec<T>` is
> `ColumnType::Array(Arc::new(T::column_type()))`.

## Value tuples

> [spec:pgorm:def:sql.value.tuple+1]
> `ValueTuple` represents an ordered tuple of values for composite keys and
> VALUES lists: `One(Value)`, `Two(Value, Value)`, `Three(Value, Value,
> Value)` or `Many(Vec<Value>)`. `IntoValueTuple` is implemented for any
> single `Into<Value>` (producing `One`), for 2- and 3-tuples (producing
> `Two`/`Three` in field order), and for 4- through 12-tuples (producing
> `Many` in field order). `IntoIterator for ValueTuple` yields the values in
> that same positional order. `ValueTuple::shape` projects a tuple onto its
> arity alone as a `ValueTupleShape` — `One`, `Two`, `Three`, or `Many(len)` —
> which displays as `ValueTuple::One` and, for the last, `ValueTuple::Many
> with length of N`.
>
> `TryFromValueTuple` inverts the mapping, is arity-strict, and is fallible:
> it returns `Result<Self, ValueTupleErr>` and never panics. The scalar impl
> requires `One`, the pair impl `Two`, the triple impl `Three`, and the 4..=12
> impls `Many` with exactly the expected length; anything else is
> `ValueTupleErr::Arity { expected, actual }`, naming both shapes. Element
> extraction goes through `ValueType::try_from`, so a type mismatch at any
> position is `ValueTupleErr::Element { position, expected }`, naming the
> zero-based position and the `ValueType::type_name` required there. The
> conversion is short-circuiting: the leftmost failing position is the one
> reported. `ValueTupleErr` implements `std::error::Error`; `Arity` displays
> as `expected {expected}, received {actual}` and `Element` as `value at
> position {position} is not a valid {expected}`, the type name backquoted.
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

> [spec:pgorm:def:sql.types+2]
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
> `DynIden`) into a `DynIden`, and also accepts `&str` and `String`, wrapping
> them in `Alias` so a string-spelled identifier escapes like any other;
> `IdenList` is implemented for a single iden and for 2- and 3-tuples,
> yielding `DynIden`s in order.
>
> `Alias` wraps an arbitrary `String` as an identifier. There is no empty-name
> identifier type: PostgreSQL rejects a zero-length delimited identifier, so
> the `NullAlias` that rendered one — and only ever served as a placeholder
> inside `ColumnDef::take` — is gone
> (`[dec:pgorm:invalid-states-unrepresentable]`).

> [spec:pgorm:def:sql.types.column-ref]
> `ColumnRef` has five forms: `Column(DynIden)`, `TableColumn(DynIden,
> DynIden)`, `SchemaTableColumn(DynIden, DynIden, DynIden)`, `Asterisk` and
> `TableAsterisk(DynIden)`. `IntoColumnRef` maps a bare iden to `Column`, a
> 2-tuple to `TableColumn`, a 3-tuple to `SchemaTableColumn`, the `Asterisk`
> unit type to `Asterisk`, and `(iden, Asterisk)` to `TableAsterisk`.

> [spec:pgorm:def:sql.types.table-ref+2]
> Table references are split by position, so that a reference which names no
> table cannot reach a statement that needs one. There are three positions,
> and each takes the widest type its position admits: DDL targets a name,
> DML targets a name with an optional alias, and a `FROM` clause or join
> additionally admits the value-producing forms.
>
> `TableName` is the DDL-position reference and has exactly two forms:
> `Table(DynIden)` and `SchemaTable(DynIden, DynIden)`. There is no
> database-qualified form — Postgres rejects a cross-database reference at
> execution, so the shape is not offered. `IntoTableName` maps a bare iden to
> `Table` and a 2-tuple to `SchemaTable`, and accepts a `TableName`
> unchanged. `TableName::table()` returns the table iden;
> `TableName::schema()` returns the schema iden when the name carries one.
>
> `NamedTable` is the DML-position reference: a struct of a `name: TableName`
> and an `alias: Option<DynIden>`, which is exactly what PostgreSQL's write
> statements target. `IntoNamedTable` accepts a `NamedTable` unchanged and
> widens anything `IntoTableName` accepts — a bare iden, a 2-tuple, a
> `TableName` — to the unaliased form, so a DML target spelled as a bare name
> needs no ceremony; `From<TableName> for NamedTable` is the same widening as
> a value conversion. `NamedTable::alias(a)` binds or replaces the alias, and
> `NamedTable::qualifier()` returns the identifier a column of the table is
> qualified by — the bound alias when there is one, otherwise the table iden.
>
> `FromItem` is the query-position reference: `Table(NamedTable)` — the DML
> reference reused, so aliasing is expressed in one place — plus the three
> value-producing forms `SubQuery(SelectStatement, alias)`,
> `ValuesList(Vec<ValueTuple>, alias)` and `FunctionCall(FunctionCall,
> alias)`, each carrying a mandatory alias. `IntoFromItem` accepts a
> `FromItem` unchanged and widens anything `IntoNamedTable` accepts;
> `From<NamedTable> for FromItem` and `From<TableName> for FromItem` are the
> same widening as value conversions. `FromItem::alias(a)` binds or replaces
> the alias on any form. `FromItem::table_name()` returns the name for the
> named form and `None` for the value-producing forms;
> `FromItem::qualifier()` returns the identifier a column of the item is
> qualified by, delegating to `NamedTable::qualifier()` for the named form.
>
> Because every DDL target takes `TableName`, every DML target takes
> `NamedTable` and every query position takes `FromItem`, a subquery, values
> list or function call in a DDL or DML position is a type error rather than a
> render-time panic or a statement PostgreSQL rejects; an alias in a DDL
> position is a type error for the same reason
> (`[spec:pgorm:sem:sql.ddl.panics+2]`).

> [spec:pgorm:def:sql.types.opers+1]
> `UnOper` has the single variant `Not`. `BinOper` enumerates the binary
> operator vocabulary: logical `And`/`Or`; pattern `Like`/`NotLike` plus
> Postgres `ILike`/`NotILike`; `Is`/`IsNot`; `In`/`NotIn`;
> `Between`/`NotBetween`; comparisons `Equal`, `NotEqual`, `SmallerThan`,
> `GreaterThan`, `SmallerThanOrEqual`, `GreaterThanOrEqual`; arithmetic
> `Add`/`Sub`/`Mul`/`Div`/`Mod`; shifts `LShift`/`RShift`; `As`;
> full-text/containment `Matches`, `Contains`, `Contained`; `Concatenate`,
> `Overlap`; pg_trgm similarity operators (`Similarity`, `WordSimilarity`,
> `StrictWordSimilarity` and their `*Distance` forms); JSON access
> `GetJsonField` (`->`) and `CastJsonField` (`->>`); regex `Regex` (`~`) and
> `RegexCaseInsensitive` (`~*`); pgvector distances `EuclideanDistance`,
> `NegativeInnerProduct`, `CosineDistance`; and an escape hatch
> `Custom(&'static str)`. There is no `Escape` operator: `ESCAPE` is
> grammatical only as the tail of a `LIKE` pattern, so it belongs to
> `SimpleExpr::LikePattern` and cannot be applied to two arbitrary operands
> (`[dec:pgorm:invalid-states-unrepresentable]`).

## Column type vocabulary

> [spec:pgorm:def:sql.types.column-type+3]
> `ColumnType` (in `pgorm-query/src/table/column.rs`, `#[non_exhaustive]`) is
> the type vocabulary shared by DDL generation, `ValueType::column_type()` and
> codegen, and every variant MUST name a type Postgres has: `Char(Option<u32>)`,
> `String(StringLen)`, `Text`, `Bytea`, `SmallInteger`, `Integer`,
> `BigInteger`, `Float`, `Double`, `Decimal(Option<(u32, u32)>)`, `Timestamp`,
> `TimestampWithTimeZone`, `Time`, `Date`, `Interval(IntervalSpec)`,
> `Bit(Option<u32>)`, `VarBit(u32)`, `Boolean`, `Money`,
> `Json`, `JsonBinary`, `Uuid`, `Custom(DynIden)`, `Enum { name, variants }`,
> `Array(Arc<ColumnType>)`, `Vector(Option<u32>)`, `Cidr`, `Inet`, `MacAddr`
> and `LTree`. `ColumnType::serial_spelling` reports the serial form of the
> integer trio and `None` for everything else
> (`[spec:pgorm:req:sql.ddl.column-def+3]`).
>
> The vocabulary carries no MySQL-era spelling and no variant that renders
> something other than what it names, and MUST NOT reacquire one. `Year` had
> no Postgres spelling at all. `TinyInteger`, like the already-removed
> `TinyUnsigned`/`SmallUnsigned`, was a second name for `smallint`;
> `Unsigned`/`BigUnsigned` rendered plain `integer`/`bigint`, claiming a
> signedness Postgres does not have; `Blob`, `Binary(u32)` and
> `VarBinary(StringLen)` were three names for `bytea` with their lengths
> silently discarded, collapsed into the single `Bytea`; `DateTime` and
> `Timestamp` were two names for the same `timestamp`, collapsed onto the
> Postgres spelling; and `Money`'s precision/scale pair produced a
> `money(p, s)` type modifier Postgres rejects when it resolves it. The
> `ColumnDef` builders went with them — there is no `tiny_integer`,
> `unsigned`, `big_unsigned`, `year`, `blob`, `binary`, `binary_len`,
> `var_binary`, `date_time` or `money_len`, and no `ColumnType::var_binary`
> constructor; `bytea()`, `timestamp()` and `money()` are the surviving
> spellings.
>
> `StringLen` parameterises varchar length: `N(u32)`, `Max`, or the
> default `None`. `IntervalSpec` is the interval tail: `Any(Option<
> IntervalPrecision>)` for the unqualified `interval`/`interval(p)`, and
> `Fields(PgInterval)` for the qualified forms. `PgInterval` enumerates the
> thirteen interval field qualifiers (`Year` through `MinuteToSecond`); the
> four second-bearing ones (`Second`, `DayToSecond`, `HourToSecond`,
> `MinuteToSecond`) carry an `Option<IntervalPrecision>` because PostgreSQL
> takes a precision only where the trailing field is `SECOND`, so
> `interval HOUR(3)` does not construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). `IntervalPrecision` is the
> closed set PostgreSQL accepts, `P0` through `P6`, with `new(digits)`
> returning `None` outside it and `digits()`/`Display` spelling it back.
> `PgInterval` implements `Display` as the SQL keywords with the precision
> appended (`YEAR TO MONTH`, `SECOND(3)`, ...) and a case-insensitive
> `TryFrom<&str>` inverse over the bare keywords.
>
> `ColumnType` equality compares parameters for the parameterised variants,
> compares `Custom` and `Enum` by rendered identifier strings (and variant
> lists), compares `Array` element types recursively, and otherwise compares
> enum discriminants. Convenience constructors: `ColumnType::custom(str)`,
> `ColumnType::string(Option<u32>)` and `ColumnType::var_binary(u32)`.
