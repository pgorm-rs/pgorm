# pgorm derive macros

This spec covers the procedural macro suite: the `pgorm-macros` crate (entity/model
derives, active enums, relations, partial models, the vendored strum `EnumIter`, and the
`#[pgorm_macros::test]` harness attribute), the two pgorm-query proc-macro crates
(`pgorm-query-derive` for `Iden`/`IdenStatic`, `pgorm-query-attr` for `#[enum_def]`), and
`pgorm-sql-macro` (the compile-time-checked `sql!` literal). Rules are maintenance-scope:
they describe what the macros generate and reject today, including known limitations.

## The macro suite

> [spec:pgorm:def:macros.derive]
> The `pgorm-macros` crate exposes the ORM's derive macros, all gated behind the crate's
> `derive` feature except `EnumIter`, which is gated behind `strum`: `DeriveEntity`,
> `DeriveEntityModel`, `DerivePrimaryKey`, `DeriveColumn`, `DeriveCustomColumn`,
> `DeriveModel`, `DeriveActiveModel`, `DeriveIntoActiveModel`, `DeriveActiveModelBehavior`,
> `DeriveActiveEnum`, `FromQueryResult`, `DeriveRelation`, `DeriveRelatedEntity`,
> `DeriveMigrationName`, `FromJsonQueryResult`, `DerivePartialModel`, `DeriveValueType`,
> `DeriveDisplay`, `DeriveIden`, and `EnumIter`, plus the `#[pgorm_macros::test]`
> attribute macro. Every entity-side derive reads its configuration from the shared
> `#[pgorm(...)]` helper attribute (`EnumIter` uses `#[strum(...)]`).
>
> Two auxiliary macros are defined in the same crate: `DeriveMigrationName` implements
> `pgorm_migration::MigrationName` by taking the file stem of `file!()`, and
> `DeriveRelatedEntity` generates a `seaography::RelationBuilder` impl but expands to an
> empty token stream unless the `seaography` feature is enabled (each variant requires an
> `entity` attribute and takes an optional `def`; relation names are the lowerCamelCase
> variant name).
>
> Separately, `pgorm-query-derive` provides `Iden` and `IdenStatic` derives using the
> `iden` and `method` helper attributes, and `pgorm-query-attr` provides the `#[enum_def]`
> attribute macro.

## DeriveEntityModel

> [spec:pgorm:sem:macros.derive.entity-model]
> `DeriveEntityModel` is the composite derive: applied to a `Model` struct, it expands
> the entity-model generation and then re-runs `DeriveModel` and `DeriveActiveModel` on
> the same input, so one derive yields the full entity module. The entity-model portion
> generates: (1) a `Column` enum deriving `Copy, Clone, Debug, EnumIter, DeriveColumn`
> with one variant per non-ignored field; (2) an `impl ColumnTrait for Column` with
> `type EntityName = Entity`, a `def()` match arm per column, and `select_as`/`save_as`
> overrides that `cast_as` an alias for columns carrying `select_as`/`save_as` attributes
> and otherwise fall back to `ColumnTrait::select_enum_as`/`save_enum_as`; (3) only when
> `table_name` is given, a `pub struct Entity;` deriving
> `Copy, Clone, Default, Debug, DeriveEntity` together with a hand-rolled `EntityName`
> impl returning the `table_name`, optional `schema_name`, and optional `comment`; and
> (4) a `PrimaryKey` enum deriving `Copy, Clone, Debug, EnumIter, DerivePrimaryKey` with
> a `PrimaryKeyTrait` impl (see `[spec:pgorm:sem:macros.derive.entity-model.primary-key]`).

> [spec:pgorm:req:macros.derive.entity-model.reject+1]
> `DeriveEntityModel` input MUST be a struct named exactly `Model`; any other identifier
> is the compile error "DeriveEntityModel requires the struct to be named `Model`",
> spanned at the offending identifier rather than raised as a macro panic. The
> entity MUST have at least one `#[pgorm(primary_key)]` field: with none, the generated
> `PrimaryKey` enum is empty and `DerivePrimaryKey` emits the compile error "Entity must
> have a primary key column. See <https://github.com/pgorm-rs/pgorm/issues/485> for
> details."

> [spec:pgorm:syn:macros.derive.entity-model.attrs]
> Struct-level `#[pgorm(...)]` keys recognised by `DeriveEntityModel`: `table_name = Lit`,
> `schema_name = Lit`, `comment = Lit`, bare `table_iden`, and `rename_all = "style"`
> (case styles per the strum-derived list: `camelCase`, `PascalCase`, `kebab-case`,
> `snake_case`, `SCREAMING_SNAKE_CASE`, `SCREAMING-KEBAB-CASE`, `lowercase`, `UPPERCASE`,
> `title_case`, `mixed_case`; unknown styles are a compile error). `table_iden` (only
> effective together with `table_name`) adds a `Table` variant to `Column`, marked
> `#[strum(disabled)]` so `EnumIter` skips it, whose `def()` arm panics with "Table
> cannot be used as a column".
>
> Field-level keys: bare `primary_key`, `nullable`, `indexed`, `unique`, `ignore`;
> `auto_increment = bool`; `column_type = "ColumnType expr"`; `column_name = "string"`;
> `enum_name = "Ident"`; `default_value = Lit`; `default_expr = "expr"`; `comment = Lit`;
> `select_as = "sql type"`; `save_as = "sql type"`. String-typed keys reject non-string
> literals with an `Invalid <key> ...` error. Unrecognised keys at both levels are
> silently skipped (their value expression is parsed only to advance the stream), so
> typos in attribute names are not diagnosed.

> [spec:pgorm:sem:macros.derive.entity-model.casing+1]
> Column-variant naming: the field identifier is stripped of a leading `r#`, converted
> to UpperCamelCase, and then keyword-escaped — any of the 49 reserved Rust keywords
> becomes a raw identifier (`r#type`), while `crate`/`Self`/`self` get a trailing
> underscore. An `enum_name` attribute replaces the computed variant name entirely (and
> is itself keyword-escaped).
>
> Case conversion drops characters that are neither alphanumeric nor word boundaries, so
> the derived name can fail to spell an identifier: the legal field names `__` and `_1`
> derive `""` and `"1"`. Each MUST be a compile error naming the field and the derived
> spelling, spanned at the field identifier, rather than a panic inside `Ident::new`.
> Validity is judged on the keyword-escaped spelling, so `SELF` — which derives `Self`
> and escapes to `Self_` — stays legal. An `enum_name` literal that does not spell an
> identifier is likewise a compile error, spanned at the literal. The same rules govern
> the variant names `DeriveModel` and `DeriveActiveModel` compute.
>
> The SQL column name is attached as `#[pgorm(column_name = "...")]` on the generated
> variant only when needed: an explicit `column_name` always wins; with `rename_all`,
> the UpperCamelCase variant name is converted per the case style; otherwise, when the
> field name does not survive a UpperCamelCase-then-snake_case round trip (e.g. `_id`,
> `id_` or names with double underscores), the snake_case of the original name is pinned.
> Fields that are already clean snake_case get no attribute, and their SQL name falls out
> of `DeriveColumn`'s default (snake_case of the variant).

> [spec:pgorm:sem:macros.derive.entity-model.column-def+3]
> Each `def()` arm builds `ColumnTypeTrait::def(<column type>)`. The column type is the
> parsed `column_type` attribute if present; otherwise it is inferred by matching the
> field's Rust type structurally — against the `syn::Type`, never against a
> whitespace-stripped rendering of it: `char`→`Char(None)`,
> `String`/`&str`→`string(None)`,
> `i8`/`i16`→`SmallInteger`, `i32`→`Integer`,
> `i64`/`u32`/`u64`→`BigInteger`,
> `f32`→`Float`, `f64`→`Double`, `bool`→`Boolean`,
> `Date`/`NaiveDate`→`Date`, `Time`/`NaiveTime`→`Time`, `DateTime`/`NaiveDateTime`→
> `Timestamp`, `DateTimeUtc`/`DateTimeLocal`/`DateTimeWithTimeZone`→
> `TimestampWithTimeZone`, `Uuid`→`Uuid`, `Json`→`Json`, `Decimal`→`Decimal(None)`,
> `Vec<u8>`→`Bytea`. Every row names a `ColumnType` Postgres has, so a
> narrower Rust integer widens to the Postgres type that holds it rather than
> naming a type the server lacks — `i8` to `smallint`, `u32` and `u64` to the
> `int8` their `Value` binds as. A named row matches only a bare,
> single-segment path carrying no generic arguments, so a qualified spelling
> (`std::string::String`) is not in the table; `&str` matches only a shared reference
> with neither lifetime nor `mut`, and `Vec<u8>` only that exact one-argument spelling.
> Any other type is assumed to be an
> `ActiveEnum`-style value and resolves at compile time via
> `<T as ValueType>::column_type()`, reproducing the type exactly as written: a
> lifetime, `dyn`, or `impl` inside it MUST NOT fuse with the token beside it
> (`&'a str` becoming `&'astr`), and the fallback MUST NOT be able to fail to re-parse.
> `u8` and `u16` are deliberately absent from
> the table: they fall through to that `ValueType` path and, since neither
> implements `ValueType` (see `[spec:pgorm:def:sql.value.conversions+1]`), a `u8`
> or `u16` field is a compile error rather than a column type that panics when
> bound. An `Option<T>` wrapper — a bare, single-segment `Option` with exactly one type
> argument — is unwrapped one level to `T` and forces `nullable`. Modifier calls are
> then chained in order: `.nullable()`, `.indexed()`, `.unique()`,
> `.default_value(lit)`, `.comment(lit)`, `.default(expr)` (from `default_expr`).
>
> The parallel `ArrayType` table that `DeriveValueType` reads
> (`[spec:pgorm:sem:macros.derive.value-type+1]`) is matched the same way and carries the
> same rows mapped to their `ArrayType` counterparts, except `Vec<u8>`, which it does not
> carry at all.

> [spec:pgorm:sem:macros.derive.entity-model.primary-key]
> Every `primary_key` field contributes a variant to the generated `PrimaryKey` enum and
> its type to `PrimaryKeyTrait::ValueType` — a bare type for a single key, a tuple for
> composite keys. `auto_increment()` returns true only when there is exactly one primary
> key column and no field set `auto_increment = false` (the flag defaults to true and is
> shared: a `false` on any field flips it globally).
>
> `DerivePrimaryKey` itself (enums only; other inputs are a compile error) generates
> `Iden` delegating to `IdenStatic::as_str`, an `IdenStatic` impl mapping each variant
> to its snake_case name (or a `column_name` attribute override), and a
> `PrimaryKeyToColumn` impl whose `into_column`/`from_column` map variants to the
> same-named variants of a sibling type hard-coded as `Column`.

> [spec:pgorm:sem:macros.derive.entity]
> `DeriveEntity` wires up the entity unit struct. It always generates
> `impl EntityTrait` with associated types `Model`, `ActiveModel`, `Column`,
> `PrimaryKey`, `Relation` — each identifier defaulting to those names but overridable
> via struct-level `#[pgorm(model = Ident, active_model = Ident, column = Ident,
> primary_key = Ident, relation = Ident)]` — plus `Iden` and `IdenStatic` impls that
> render `EntityName::table_name`. An `EntityName` impl (with optional `schema_name`)
> is generated only when a `table_name` attribute is present; the `DeriveEntityModel`
> path omits it because the composite writes its own `EntityName` impl.

## Column, Model and ActiveModel derives

> [spec:pgorm:sem:macros.derive.column]
> `DeriveColumn` (enums only; other inputs are a compile error) generates: an inherent
> `default_as_str` returning the snake_case of the variant name or a
> `#[pgorm(column_name = "...")]` override; a `FromStr` impl accepting either the
> snake_case or the lowerCamelCase spelling of each variant and returning
> `ColumnFromStrErr(input)` otherwise; an `Iden` impl writing `IdenStatic::as_str`; and
> an `IdenStatic` impl whose `as_str` is `default_as_str`. `DeriveCustomColumn`
> generates the same minus the `IdenStatic` impl, leaving `as_str` to the user (who may
> delegate to `default_as_str`) — this is the escape hatch for non-snake-case column
> names. Note that neither derive adds `EnumIter`; callers derive it alongside.

> [spec:pgorm:sem:macros.derive.model+1]
> `DeriveModel` (named-field structs only; otherwise a compile error) generates two
> impls. `FromQueryResult`: each field is read with
> `row.try_get(pre, Column::<Variant>.as_str())`, where the variant name follows the
> same trim/UpperCamelCase/keyword-escape/`enum_name` rules as
> `[spec:pgorm:sem:macros.derive.entity-model.casing+1]`; `#[pgorm(ignore)]` fields are
> filled with `Default::default()` instead. `ModelTrait`: `get` clones the field and
> converts it `.into()` a `Value`; `set` converts through `ValueType::try_from` and
> assigns, answering `DbErr::Type("value does not match the type of {Model} field
> {field}")` when the value is of another type. Ignored fields have no match arm, so
> `get` on them panics with "field does not exist on {Model}" and `set` returns that
> same text as `DbErr::Type`. The
> target entity defaults to `Entity`, overridable via `#[pgorm(entity = Ident)]`.

> [spec:pgorm:sem:macros.derive.active-model+1]
> `DeriveActiveModel` (named-field structs only) generates a
> `pub struct ActiveModel` — name and entity are hard-coded as `ActiveModel`/`Entity` —
> with one `pub field: ActiveValue<T>` per non-ignored field, plus: `Default`
> delegating to `ActiveModelBehavior::new()`; `From<Model>` mapping every field through
> `ActiveValue::unchanged`; an `IntoActiveModel` impl for `Model`; and
> `ActiveModelTrait` with `take`/`get` (unmatched columns yield `not_set`),
> `set` (`DbErr::Type("This ActiveModel does not have this field")` on an unmatched
> column, and `DbErr::Type("value does not match the type of ActiveModel field
> {field}")` when the value is of another type),
> `not_set` (silently ignores unmatched), `is_not_set` (panics on unmatched),
> `default` (all fields `not_set`), and `reset`. It also generates
> `TryFrom<ActiveModel> for Model` and `TryIntoModel`: each non-ignored field is
> taken out of its `ActiveValue` directly — no round trip through `Value` — and one
> left `NotSet` fails with `DbErr::AttrNotSet(field)`; ignored fields are rebuilt with
> `Default::default()`.
>
> `DeriveActiveModelBehavior` unconditionally emits
> `impl ActiveModelBehavior for ActiveModel {}`, ignoring the input's identifier and
> shape. `DeriveIntoActiveModel` (named-field structs only) generates an
> `IntoActiveModel<A>` impl — `A` defaults to `ActiveModel`, overridable with
> `#[pgorm(active_model = Ident)]` — that converts each field via
> `IntoActiveValue::into_active_value(..).into()` and fills the remainder with
> `..Default::default()`, enabling partial "form" structs.

## Active enums

> [spec:pgorm:syn:macros.derive.active-enum]
> `DeriveActiveEnum` applies to enums only ("you can only derive ActiveEnum on enums").
> Container attributes: `rs_type = "Type"` and `db_type = "ColumnType expr"` are
> mandatory — missing either produces the compile error "Missing macro attribute
> `rs_type`"/"`db_type`" — while `enum_name = "string"` (defaulting to the
> UpperCamelCase of the enum name) and `rename_all = "style"` are optional. The special
> spelling `db_type = "Enum"` expands to
> `Enum { name: Self::name(), variants: Self::iden_values() }`. Variant attributes:
> `string_value = "s"`, `num_value = int`, `rename = "style"`, and `display_value`
> (accepted only as a placeholder for `DeriveDisplay`). Unknown keys at either level are
> rejected with "Unknown attribute parameter found" — unlike the entity derives, which
> skip them. String-flavoured markers (`string_value`, `rename`, `rename_all`) and
> `num_value` are mutually exclusive across the whole enum; mixing them is the compile
> error "All enum variants should specify the same `*_value` macro attribute...". A
> variant with no attribute falls back to its integer discriminant, including negative
> literals written with unary minus (other unary operators are rejected); a variant with
> neither attribute nor usable discriminant is a compile error.

> [spec:pgorm:sem:macros.derive.active-enum.expansion]
> The expansion generates: a unit struct `{Enum}Enum` with an `Iden` impl writing the
> `enum_name`; when any variant has a string value (explicit or via rename), an enum
> `{Enum}Variant` deriving `EnumIter` with an `Iden` impl writing the raw string values
> and an inherent `iden_values()` returning them as `DynIden`s; an `ActiveEnum` impl
> with `Value = rs_type`, `ValueVec = Vec<rs_type>`, `name()` returning the
> `{Enum}Enum` iden, `to_value()` matching variants to their values, `try_from_value()`
> matching back (comparing `v.as_ref()` for strings) and failing with
> `DbErr::Type("unexpected value for {Enum} enum: ...")`, and `db_type()` building a
> `ColumnDef` from the `db_type` tokens; plus `TryGetable`, `TryGetableArray`,
> `Into<Value>`, `ValueType`, `Nullable`, and the `NotU8` marker impl.
>
> `{Enum}Variant` identifiers are produced by an escaping PascalCase conversion:
> characters outside Unicode UAX#31 identifier classes, plus `_` and space, are replaced
> by their `{:#X}` hex notation (`_`→`0x5F`, space→`0x20`); the empty string becomes
> `__Empty`; a leading digit gets an `_` prefix. Known limitation: the conversion is
> case-preserving but collision-blind — string values differing only in case map to the
> same identifier and fail to compile.
>
> `DeriveDisplay` (enums only) is the companion derive: it generates a `std::fmt::Display`
> impl writing each variant's identifier text, or the `#[pgorm(display_value = "...")]`
> override; `string_value`/`num_value`/`rename` are parsed and ignored, and unknown keys
> are rejected.

## Relations

> [spec:pgorm:syn:macros.derive.relation+1]
> `DeriveRelation` applies to enums only. Each variant requires exactly one of
> `belongs_to = "path::to::Entity"`, `has_one = "..."`, or `has_many = "..."` (checked
> in that order; none present is the error "Missing one of 'has_one', 'has_many' or
> 'belongs_to'"); the value is a string parsed as a token path. `belongs_to` variants
> additionally require `from = "Column::X"` and `to = "Column::Y"` (errors "Missing
> attribute 'from'"/"Missing attribute 'to'"); on `has_one`/`has_many` both are
> optional, but the two MUST be given together or not at all — one without the
> other raises the same missing-attribute error, since a column can only be added
> to a relation paired with its counterpart. Both values are parsed as
> expressions and a tuple on either side is taken apart per column, so a
> composite key expands to `.columns(from0, to0).and_columns(from1, to1)...`.
> Sides of unequal length are rejected at expansion, where the arity is still
> known, with "'from' names N column(s) and 'to' names M; a relation joins its
> columns in pairs". Further optional keys chain builder calls: `on_update`/`on_delete` (a
> `ForeignKeyAction` variant name), `on_condition` (an expression wrapped in an
> `IntoCondition` closure), `fk_name` (string), and `condition_type` (case-insensitive
> `"all"` or `"any"`; anything else is "Condition type must be one of `all` or `any`").
> Non-string literal values are rejected with "attribute must be a string". The
> expansion is an `impl RelationTrait` whose `def()` matches each variant to
> `Entity::belongs_to/has_one/has_many(target)` plus the paired column calls, the
> chained modifiers and a
> catch-all arm panicking "No RelationDef for {Relation}"; the entity identifier
> defaults to `Entity`, overridable via container `#[pgorm(entity = Ident)]`.

## Projection and value derives

> [spec:pgorm:sem:macros.derive.partial-model+1]
> `DerivePartialModel` accepts only non-generic named-field structs (generics and other
> shapes are compile errors). The container attribute `entity = "Type"` names the source
> entity; it is required unless every field carries `from_expr`, and its absence
> otherwise produces "you need specific which entity you are using". Per field:
> no attribute selects `Column::{UpperCamelCase(field)}`; `from_col = "name"` selects
> `Column::{UpperCamelCase(name)}` aliased to the field name; `from_expr = "expr"`
> selects the parsed expression aliased to the field name. The expansion implements
> `PartialModelTrait::select_cols` by chaining `SelectColumns::select_column` /
> `select_column_as` calls; the chain's type is `S::Projected`, so a struct with no
> fields — whose chain is empty and whose body is therefore the unprojected `S` — does
> not compile. Limitation: although supplying both `from_col` and
> `from_expr` is a documented compile error, the parser overwrites both trackers on
> every meta item in a `#[pgorm(...)]` list, so only the last recognised key of the last
> attribute takes effect and the both-keys guard is unreachable in practice.

> [spec:pgorm:sem:macros.derive.from-query-result]
> `FromQueryResult` (named-field structs only; generics supported) implements
> `from_query_result(row, pre)` by `row.try_get(pre, "<field>")` for each field, using
> the un-rawed field identifier as the column name; fields marked `#[pgorm(skip)]` are
> filled with `Default::default()`. The skip flag is recomputed for every meta item in
> an attribute list, so `skip` is only honoured as the last (or sole) item.
>
> `FromJsonQueryResult` performs no input validation at all (only the identifier is
> used). It generates the `TryGetableFromJson` marker; `From<T> for Value` serialising
> through `serde_json::to_value` — a serialisation failure silently becomes
> `Value::Json(None)`; a `ValueType` impl accepting only `Value::Json(Some(_))` (column
> type and array type are `Json`); `Nullable` returning `Value::Json(None)`; and the
> `NotU8` marker.

> [spec:pgorm:sem:macros.derive.value-type+1]
> `DeriveValueType` targets newtype tuple structs; it reads only the first unnamed
> field. Any other input shape is a compile error spanned at the struct identifier —
> "DeriveValueType can only be derived on a tuple struct", or "DeriveValueType requires
> the tuple struct to hold a value field" for an empty one — not a macro panic. Optional
> attributes `column_type = "..."` and
> `array_type = "..."` override the inferred `ColumnType`/`ArrayType`, which otherwise
> use the same Rust-type tables (and `Option<T>` unwrapping) as
> `[spec:pgorm:sem:macros.derive.entity-model.column-def+3]`, falling back to
> `<T as ValueType>::column_type()`/`array_type()`. Attribute errors propagate: a
> non-string value for either key, and any other key in the `#[pgorm(...)]` list, is a
> spanned compile error rather than being silently ignored. The expansion implements
> `From<T> for Value` (through `self.0`), `TryGetable`, and `ValueType` delegating to
> the inner type with `type_name()` = the struct name; no `Nullable` impl is generated.

## Iden derives and query helpers

> [spec:pgorm:sem:macros.derive.iden+1]
> `DeriveIden` (in `pgorm-macros`) supports enums and unit structs; anything else is
> the compile error "you can only derive DeriveIden on unit struct or enum", and an
> empty enum expands to nothing. A unit struct renders the snake_case of its type name,
> overridable with container `#[pgorm(iden = "...")]`. An enum renders each variant's
> snake_case name, except the special variant `Table`, which renders the snake_case of
> the enum's own name; per-variant `#[pgorm(iden = "...")]` substitutes a literal
> string. The `Iden::prepare` override (which wraps the name in quote characters) is
> emitted only when every rendered name is a "valid iden" (first char `_` or ASCII
> alphabetic, rest `_` or ASCII alphanumeric); otherwise the trait default handles
> quoting. A malformed variant-level attribute is the parser's own spanned compile
> error, propagated out of the derive rather than panicking the macro.

> [spec:pgorm:sem:macros.derive.iden.query]
> `pgorm-query-derive` defines `Iden` and `IdenStatic` derives over enums and unit
> structs (helper attributes `iden` and `method`; empty enums expand to nothing; other
> shapes are a compile error). The container name defaults to the snake_case type name
> and may only be renamed with `#[iden = "name"]` — list forms at container level are
> rejected. Variant forms: `#[iden = "name"]` and `#[iden(rename = "name")]` substitute
> a literal; `#[method = "name"]` and `#[iden(method = "name")]` render via
> `self.name()`; `#[iden(flatten)]` delegates to the single field of a one-field
> variant (multi-field or unit variants are "Must have a single field is supported for
> flattenning"); the variant `Table` renders the container name; everything else
> snake_cases the variant. `prepare` is emitted only when all names are statically
> valid idens (`method`/`flatten` disqualify). `IdenStatic` additionally generates
> `as_str() -> &'static str` and `AsRef<str>` from the same naming rules. Limitation:
> `TryFrom<Meta> for IdenAttr` ends in a bare `todo!()` for any attribute path other
> than `iden`/`method` (pgorm-query-derive/src/iden_attr.rs:91); it is unreachable
> today only because `find_attr` pre-filters to those two paths, and any new call path
> would panic rather than error.

> [spec:pgorm:sem:macros.derive.enum-def]
> `#[enum_def]` (in `pgorm-query-attr`) applies to named-field structs — anything else
> panics with "#[enum_def] can only be used on structs" — and re-emits the input
> unchanged followed by a generated
> `pub enum {prefix}{Struct}{suffix}` (defaults: empty prefix, suffix `Iden`, so
> `StructIden`) deriving `Debug, Clone, Copy, PartialEq, Eq, Hash`, with a `Table`
> variant plus one PascalCase variant per field. Its `Iden::unquoted` writes
> `stringify!` of the table name identifier for `Table` — defaulting to the snake_case
> struct name, overridable with `table_name = "..."` (which must itself be a valid
> identifier, since it is re-parsed as one) — and `stringify!` of the original field
> identifier for each field variant. The `crate_name = "..."` argument (default
> `pgorm_query`) rewrites the `Iden` trait path but not the hard-coded
> `pgorm_query::Write` argument type in the generated method.

## Iteration and test harness

> [spec:pgorm:sem:macros.derive.enum-iter]
> `EnumIter` is a vendored strum derive: for an enum `E` it generates an `EIter` struct
> and implements `strum::IntoEnumIterator`, with `Iterator`, `DoubleEndedIterator`,
> `ExactSizeIterator`, `FusedIterator`, `Clone`, and `Debug` on the iterator. Variants
> carrying data are constructed with `Default::default()` for every field; variants
> marked `#[strum(disabled)]` are skipped entirely (this is how the `table_iden`
> `Table` column variant is excluded from iteration). Enums with lifetime parameters
> are rejected ("This macro doesn't support enums with lifetimes."); type parameters
> are supported via a `PhantomData` marker.

> [spec:pgorm:sem:macros.derive.test-attr+1]
> `#[pgorm_macros::test]` is a hidden test-harness attribute: it rewrites an async test
> body into a `#[test]` fn that initialises a `tracing_subscriber` at DEBUG level with a
> test writer and drives the body through `crate::block_on!`. Caller attributes are
> passed through to the generated fn verbatim. The expansion adds no cfg gating of its
> own, so wrapped tests compile unconditionally.

## Compile-time SQL validation

> [spec:pgorm:def:macros.sql+1]
> `pgorm-sql-macro` is a proc-macro crate exporting exactly one function-like macro,
> `sql!`. It takes one string literal and nothing else. While the calling crate is being
> compiled, the literal's text is handed to `pg_query::parse` — the Rust binding to
> libpg_query, the PostgreSQL server's own parser, pinned to the same `6.2.0` the render
> oracle uses and for the same reason (`[spec:pgorm:req:sql.render.oracle]`). A literal
> the grammar accepts expands to itself, byte for byte, as a `&'static str` usable in
> const position; there is no wrapper type, no runtime check and no allocation, so the
> macro is invisible in the generated code.
>
> The macro lives in its own crate rather than in `pgorm-macros` because a proc macro is
> the wrong place for a C dependency the entity derives have no use for. The root crate
> re-exports it as `pgorm::sql` behind the off-by-default `sql-macro` feature. That
> feature no longer decides whether libpg_query is compiled — `pg_query` is a plain
> dependency of `pgorm` itself, because `[spec:pgorm:sem:exec.paginator.raw+1]` parses
> raw statements at runtime — only whether the macro is in scope. Its call sites are the
> escape hatches that take SQL as text — `SelectorRaw::from_statement`,
> `ConnectionTrait::query_raw` / `execute_raw` / `batch_execute`, and migration bodies.

> [spec:pgorm:req:macros.sql.reject]
> `sql!` MUST refuse, at compile time, both an input that is not a single string literal
> and a literal the PostgreSQL grammar rejects. It MUST NOT panic on any input: a
> proc-macro panic surfaces as an unattributed compiler abort rather than a diagnostic
> the caller can act on, which is the same objection `[dec:pgorm:no-panic]` raises
> against panicking on a caller's mistake at runtime.
>
> Non-literal input is refused by `syn`'s own parse: bare tokens and non-string literals
> give `expected string literal` spanned at the offending token, and a literal followed by
> anything else gives `unexpected token` spanned at the trailing argument. The macro
> deliberately takes no bound parameters; `$1` placeholders are written in the SQL and
> their values supplied at the call site.
>
> A grammar rejection becomes `syn::Error::new(literal.span(), ..)` whose message is
> `PostgreSQL rejected this SQL: ` followed by the parser's own diagnostic, unwrapped from
> the `pg_query::Error` variant that would otherwise prefix it with a stage name. Input
> the parser cannot even be handed — a literal carrying an interior NUL byte, which fails
> the `CString` conversion — takes the same path and reports the conversion failure, so
> no input reaches an abort.

> [spec:pgorm:sem:macros.sql.script]
> A literal holding a `;`-separated script is validated whole: a single `pg_query::parse`
> call walks every statement in the string and reports the first one the grammar refuses,
> so a well-formed opening statement does not excuse a malformed later one. No separate
> split pass is involved — `pg_query::split_with_parser` rejects exactly what `parse`
> rejects, so a split-then-parse pipeline would carry a stage that can never fire.
>
> A literal that yields no statements at all — empty, whitespace-only, comment-only, or a
> bare `;` — is accepted, because PostgreSQL accepts it: the server answers an empty query
> string with an empty-query response. `sql!` reports on the grammar and nothing else, so
> it does not add a "must contain a statement" rule of its own.

> [spec:pgorm:req:macros.sql.ceiling]
> Two limits are part of the contract and MUST be stated on the macro rather than implied
> away.
>
> The check is syntax only. libpg_query carries no catalog, so unknown tables, unknown
> columns and misapplied type modifiers (`money(12, 2)`) all pass — the same ceiling the
> render oracle declares. `sql!` rules out typos in SQL, not mistakes about the schema: a
> literal it accepts can still fail against a live server, and the integration suite
> remains the semantic oracle.
>
> The error spans the whole literal, not the offending byte inside it. Narrowing to a
> sub-literal span needs `proc_macro::Literal::subspan`, which is nightly-only —
> `proc-macro2` compiles its call away and returns `None` on stable — so the position
> rides in the message instead: the offending line is reprinted with a caret under the
> column the parser named. libpg_query surfaces only the message and not a cursor offset,
> so that column is recovered by locating the token the message quotes, using the same
> last-occurrence heuristic as the render oracle; a message naming no token, such as
> `syntax error at end of input`, gets no caret.
