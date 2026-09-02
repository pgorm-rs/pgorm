# Entity code generation (pgorm-codegen)

`pgorm-codegen` turns a discovered PostgreSQL schema into Rust entity source
files. The pipeline has two stages: `EntityTransformer::transform` converts a
list of `TableCreateStatement`s into an in-memory `Entity` model, and
`EntityWriter::generate` renders those entities into output files. Everything
below describes what the code emits today; text-level shapes are pinned by the
golden fixtures under `pgorm-codegen/tests/`.

> [spec:pgorm:def:codegen.entity]
> The entity generator is the pipeline
> `EntityTransformer::transform(Vec<TableCreateStatement>) -> EntityWriter`
> followed by `EntityWriter::generate(&EntityWriterContext) -> WriterOutput`.
> An `Entity` carries `table_name`, `columns` (name, `ColumnType`,
> `auto_increment`, `not_null`, `unique`), `relations`, `conjunct_relations`,
> and `primary_keys`. A `WriterOutput` is a list of `OutputFile { name, content }`
> — the writer never touches the filesystem; callers (e.g. `pgorm-cli`) write
> the files and run `rustfmt` over them.
>
> `EntityWriterContext` selects all generation options: `expanded_format`,
> `with_serde`, `with_copy_enums`, `date_time_crate`, `schema_name`, `lib`,
> `serde_skip_deserializing_primary_key`, `serde_skip_hidden_column`,
> `model_extra_derives`, `model_extra_attributes`, `enum_extra_derives`,
> `enum_extra_attributes`, and `seaography`. Extra derives are appended to the
> generated derive lists; extra attributes are emitted as additional
> `#[...]` lines on the Model struct or enum.
>
> Errors are the two-variant `Error` enum: `StdIoError(io::Error)` and
> `TransformError(String)`.

## Schema discovery → Entity model

> [spec:pgorm:sem:codegen.entity.transform+1]
> `EntityTransformer::transform` builds one `Entity` per input
> `TableCreateStatement`. The table name is unpacked from any `TableRef`
> variant carrying a table iden (`Table`, `SchemaTable`,
> `DatabaseSchemaTable`, and their alias forms); a statement with no table
> name yields `Error::TransformError("Table name should not be empty")`.
>
> Per column: `auto_increment` and `not_null` come from the presence of the
> matching `ColumnSpec` on the column definition; a column with no
> `ColumnType` panics with `"ColumnType should not be empty"`. `unique` does
> not: the transformer overwrites the value the `From<&ColumnDef> for
> Column` conversion derived, assigning `unique` solely from the table's
> indexes — true exactly when some unique index of the table covers that one
> column and nothing else. A `ColumnSpec::UniqueKey` on the column
> definition is therefore discarded on this path; it takes effect only when
> a `ColumnDef` is converted through the public `From<&ColumnDef> for
> Column` impl directly, outside `transform`. Primary keys are collected
> from `ColumnSpec::PrimaryKey` markers and extended with the column names
> of any table-level primary-key index. Every column whose (possibly
> array-inner) type is `ColumnType::Enum` registers an `ActiveEnum` in a
> `BTreeMap` keyed by enum name, deduplicating across tables.
>
> Foreign keys become `BelongsTo` relations on the owning table, keeping the
> FK's columns, referenced columns, `on_update`, and `on_delete` actions. A
> relation whose referenced table equals its own table is flagged
> `self_referencing`. When several FKs of one table reference the same
> target table, each such relation receives a distinct 1-based `num_suffix`
> — but the numbering runs in reverse declaration order: the per-target
> counter is seeded with the number of FKs to that target and decremented as
> suffixes are handed out, so for N FKs to one target the first-declared
> receives suffix N and the last-declared receives 1 (`fruit_id1` becomes
> `Fruit2` and `fruit_id2` becomes `Fruit1`). A single FK to a target keeps
> suffix 0.
>
> Entities are held in a `BTreeMap` keyed by table name, so all outputs that
> iterate entities (entity files, `mod.rs`, `prelude.rs`) are ordered
> alphabetically by table name. Before writing, each entity's `relations`
> are sorted by referenced table name and its `conjunct_relations` by target
> name.

> [spec:pgorm:sem:codegen.entity.transform.inverse]
> For every non-self-referencing relation with `num_suffix == 0`, the
> transformer adds an inverse relation to the referenced entity, pointing
> back at the FK-owning table with empty column lists. The inverse type is
> `HasOne` when the FK is unique — every FK column is a unique column of the
> owning table, or the FK column set is exactly the owning table's full
> primary-key set — and `HasMany` otherwise. Self-referencing relations and
> suffixed relations produce no inverse (the suffixed case would emit a
> `Relation` variant with no usable `Related` impl). An inverse relation is
> dropped when the target entity already has any relation to that table.

> [spec:pgorm:sem:codegen.entity.transform.conjunct]
> A table with exactly 2 relations and exactly 2 primary-key columns is
> treated as a many-to-many junction: each of the two referenced entities
> receives a `ConjunctRelation { via: junction_table, to: other_ref_table }`.
> When an entity accumulates more than one conjunct relation to the same
> target (duplicated many-to-many paths), all conjunct relations to that
> target are removed — ambiguity is resolved by generating nothing. When a
> conjunct relation targets the same table as an ordinary relation, that
> relation's `impl_related` flag is cleared so only the conjunct
> (`via`-based) `Related` impl is generated.

## Output files

> [spec:pgorm:req:codegen.entity.files]
> `EntityWriter::generate` MUST produce: one `<table_name_snake_case>.rs`
> per entity; an index file named `lib.rs` when `lib` is true and `mod.rs`
> otherwise; `prelude.rs`; and `pgorm_active_enums.rs` if and only if at
> least one enum was discovered. Every generated file MUST start with the
> header line
> ``//! `pgorm` Entity, @generated by pgorm-codegen <version>``
> where `<version>` is the pgorm-codegen crate version.
>
> The index file MUST contain, in order: `pub mod prelude;`, one
> `pub mod <table_name_snake_case>;` per entity (keyword-escaped, in
> alphabetical order), and `pub mod pgorm_active_enums;` last when enums
> exist. `prelude.rs` MUST contain one
> `pub use super::<table_name_snake_case>::Entity as <TableNameCamelCase>;`
> per entity. Entity-file code blocks are joined with blank lines; content
> is unformatted `TokenStream` text (callers are expected to run rustfmt).

> [spec:pgorm:sem:codegen.entity.context]
> `EntityWriterContext::new` normalizes the option lists at construction:
> `model_extra_derives` / `enum_extra_derives` pass through `bonus_derive`,
> which parses each string as a `TokenStream` and folds them into a single
> leading-comma fragment (`, A, B`); `model_extra_attributes` /
> `enum_extra_attributes` pass through `bonus_attributes`, which wraps each
> parsed string in its own `#[...]` attribute line. Parsing is
> `.parse().unwrap()`, so an extra derive or attribute that is not valid
> Rust token text panics at context construction, before any file is
> generated.
>
> `date_time_crate` is threaded by reference from the context through
> `write_entities` into both block generators: it selects Model field types
> (`get_column_rs_types`) in both formats, the expanded
> `PrimaryKeyTrait::ValueType` (`get_primary_key_rs_type`), and the
> per-column `tracing::info!` lines (`Column::get_info`) that
> `write_entities` logs while generating each file. The type mapping itself
> is `codegen.entity.types.datetime`; the compact format has no further
> date-time surface (its `PrimaryKeyTrait` comes from `DeriveEntityModel`).

## Compact and expanded formats

> [spec:pgorm:def:codegen.entity.compact]
> The compact format (default, `expanded_format == false`) emits per entity,
> in order: the imports (`use pgorm::entity::prelude::*;`, serde imports,
> and one `use super::pgorm_active_enums::<EnumName>;` per distinct enum
> used by the entity's columns); a single `Model` struct deriving
> `Clone, Debug, PartialEq, DeriveEntityModel` (plus `Eq`, serde derives,
> and extra derives) annotated with
> `#[pgorm(schema_name = "...", table_name = "...")]` (`schema_name` only
> when configured); a `Relation` enum deriving
> `Copy, Clone, Debug, EnumIter, DeriveRelation` whose variants carry
> `#[pgorm(...)]` relation attributes (an entity with no relations emits the
> empty `pub enum Relation {}`); the `Related` impls; and
> `impl ActiveModelBehavior for ActiveModel {}`. `DeriveEntityModel`
> expands the Entity/Column/PrimaryKey machinery that the expanded format
> spells out.

> [spec:pgorm:def:codegen.entity.expanded]
> The expanded format (`expanded_format == true`) emits per entity, in
> order: the same imports; `#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
> pub struct Entity;`; `impl EntityName for Entity` containing
> `fn schema_name(&self) -> Option<&str>` returning `Some("...")` only when
> a schema name is configured, and `fn table_name(&self) -> &str` returning
> the table name literal; a `Model` struct deriving
> `Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel` (plus `Eq`,
> serde, extras) with no `#[pgorm(...)]` struct attribute; a `Column` enum
> deriving `Copy, Clone, Debug, EnumIter, DeriveColumn` (variants of
> non-snake-case columns carry `#[pgorm(column_name = "...")]`); a
> `PrimaryKey` enum deriving `Copy, Clone, Debug, EnumIter,
> DerivePrimaryKey`; `impl PrimaryKeyTrait`; a `Relation` enum deriving
> `Copy, Clone, Debug, EnumIter` (no `DeriveRelation`); `impl ColumnTrait
> for Column` whose `def()` matches every column to a `ColumnType`
> expression chain (`ColumnType::X.def()` + `.null()` when nullable +
> `.unique()` when unique, and `<EnumName>::db_type()` for enum columns);
> `impl RelationTrait for Relation` whose `def()` matches every variant to a
> `RelationDef` expression — or has the body `panic!("No RelationDef")` when
> there are no relations; the `Related` impls; and
> `impl ActiveModelBehavior for ActiveModel {}`.

> [spec:pgorm:sem:codegen.entity.compact.attrs]
> In the compact Model, each field's `#[pgorm(...)]` attribute assembles
> parts in this fixed order: `column_name = "..."` when the DB column name
> is not already snake_case; `primary_key` when the column is in the primary
> key, followed by `auto_increment = false` when that PK column is not
> auto-increment; `column_type = "..."` for exactly the types whose default
> mapping is ambiguous — `Float`, `Double`, `Decimal(Some((p, s)))`,
> `Money(Some(p, s))`, `Text`, `JsonBinary`, `custom("...")`, `Binary(n)`,
> `VarBinary(StringLen::...)`, `Blob` — with `nullable` appended (only
> alongside a `column_type`) when the column is nullable; and `unique` when
> the column is unique. Fields needing none of these carry no `#[pgorm]`
> attribute.

> [spec:pgorm:sem:codegen.entity.compact.model]
> `gen_compact_model_struct` emits the compact `Model` as one block: the
> derive attribute `Clone, Debug, PartialEq, DeriveEntityModel` followed by
> the `Eq` slot, the serde fragment (`codegen.entity.serde.derives`), and
> `model_extra_derives`; the struct attribute
> `#[pgorm(schema_name = "...", table_name = "...")]` (the `schema_name =`
> part present only when a schema name is configured); then
> `model_extra_attributes` as further attribute lines. Fields follow the
> entity's column order; each field is the keyword-escaped snake_case
> column name (`codegen.entity.keywords`) typed per `codegen.entity.types`,
> preceded first by its assembled `#[pgorm(...)]` attribute
> (`codegen.entity.compact.attrs`, omitted when no parts apply) and then by
> its serde attribute (`codegen.entity.serde.skip`). Primary-key membership
> is decided by matching the raw DB column name against the entity's
> `primary_keys` list.

> [spec:pgorm:sem:codegen.entity.expanded.blocks]
> `gen_expanded_code_blocks` assembles an entity's expanded file as an
> ordered `Vec<TokenStream>`, one block per section, each produced by a
> dedicated generator: a single import block (`gen_import` extended with
> `gen_import_active_enum`); `gen_entity_struct`; `gen_impl_entity_name`;
> `gen_model_struct`; `gen_column_enum`; `gen_primary_key_enum`;
> `gen_impl_primary_key`; `gen_relation_enum`; `gen_impl_column_trait`;
> `gen_impl_relation_trait`; then zero or more `gen_impl_related` blocks,
> zero or more `gen_impl_conjunct_related` blocks,
> `gen_impl_active_model_behavior`, and — only with the `seaography` flag —
> `gen_related_entity` last. `write_entities` prepends the generated-file
> header and joins the stringified blocks with blank lines, so each section
> is one contiguous block in the output (rendered shapes pinned by
> `tests/expanded/`).
>
> The expanded Model block (`gen_model_struct`) derives
> `Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel`, then the `Eq`
> slot, the serde fragment, and `model_extra_derives`;
> `model_extra_attributes` render as attribute lines between the derive and
> the struct, and fields carry only their serde attributes — the
> `#[pgorm(...)]` field metadata of the compact format has no expanded
> counterpart because `Column` / `PrimaryKey` are spelled out explicitly.

> [spec:pgorm:sem:codegen.entity.imports]
> `gen_import` builds the per-file import block: always
> `use pgorm::entity::prelude::*;`, followed by the serde import selected
> by `WithSerde` — `use serde::Serialize;` for `Serialize`,
> `use serde::Deserialize;` for `Deserialize`,
> `use serde::{Deserialize, Serialize};` for `Both`, nothing for `None`.
> For entity files, `gen_import_active_enum` extends the block with one
> `use super::pgorm_active_enums::<EnumName>;` per distinct enum among the
> entity's column types (looked through `Array` via `get_inner_col_type`),
> deduplicated in first-use column order. The same `gen_import` block —
> without enum imports — heads `pgorm_active_enums.rs`
> (`codegen.entity.enums`); in every file the imports sit directly below
> the `write_doc_comment` header required by `codegen.entity.files`.

## Type mapping

> [spec:pgorm:sem:codegen.entity.types+1]
> Model field types come from `Column::get_rs_type`: a non-null column maps
> to `T`, a nullable column to `Option<T>`, where `T` is:
>
> | ColumnType | Rust type |
> |---|---|
> | `Char(_)`, `String(_)`, `Text`, `Custom(_)` | `String` |
> | `TinyInteger` / `SmallInteger` / `Integer` / `BigInteger` | `i8` / `i16` / `i32` / `i64` |
> | `Unsigned` / `BigUnsigned` | `u32` / `u64` |
> | `Float` / `Double` | `f32` / `f64` |
> | `Json`, `JsonBinary` | `Json` |
> | `Decimal(_)`, `Money(_)` | `Decimal` |
> | `Uuid` | `Uuid` |
> | `Binary(_)`, `VarBinary(_)`, `Blob` | `Vec<u8>` |
> | `Boolean` | `bool` |
> | `Enum { name, .. }` | UpperCamelCase of `name` |
> | `Array(inner)` | `Vec<T(inner)>` (recursive) |
> | `Date`, `Time`, `DateTime`, `Timestamp`, `TimestampWithTimeZone` | per `codegen.entity.types.datetime` |
>
> The named types resolve through `pgorm::entity::prelude::*`.
>
> The `Eq` derive is added to the Model derive list only when no column's
> type is `Float` or `Double`, checked recursively through `Array` element
> types; a single float column suppresses `Eq` for the whole Model.

> [spec:pgorm:sem:codegen.entity.types.datetime]
> `DateTimeCrate` selects the date/time field types:
>
> | ColumnType | `Chrono` | `Time` |
> |---|---|---|
> | `Date` | `Date` | `TimeDate` |
> | `Time` | `Time` | `TimeTime` |
> | `DateTime` | `DateTime` | `TimeDateTime` |
> | `Timestamp` | `DateTimeUtc` | `TimeDateTime` |
> | `TimestampWithTimeZone` | `DateTimeWithTimeZone` | `TimeDateTimeWithTimeZone` |
>
> Limitation: only `Chrono` is usable in practice. The `TimeDate`-family
> aliases in `pgorm::entity::prelude` are gated behind a `with-time` cargo
> feature that pgorm's `Cargo.toml` does not define (only `with-chrono` is a
> default feature), and pgorm's `tokio-postgres` dependency is built with
> `with-chrono-0_4` only — so code generated with `DateTimeCrate::Time` does
> not compile against pgorm as shipped.

> [spec:pgorm:req:codegen.entity.types.unsupported]
> Column types outside the mapping table are not supported: `get_rs_type`
> panics via `unimplemented!("column type {other:?} is not supported by
> codegen")`, and the expanded-format `ColumnDef` writer (`Column::get_def`)
> likewise hits an `unimplemented!()` wildcard. Codegen MUST NOT be expected
> to degrade gracefully on such types — the generation run aborts by panic
> rather than emitting placeholder code.

## Serde

> [spec:pgorm:def:codegen.entity.serde]
> `WithSerde` has four variants parsed from the strings `none`,
> `serialize`, `deserialize`, `both` (anything else is a
> `TransformError("Unsupported enum variant '...'")`). It contributes both
> an import and extra Model/enum derives: `Serialize` adds
> `use serde::Serialize;` and the `Serialize` derive; `Deserialize` the
> mirror image; `Both` adds `use serde::{Deserialize, Serialize};` and both
> derives; `None` adds nothing. The same derives are appended to generated
> active enums, and the same import is emitted at the top of
> `pgorm_active_enums.rs`.

> [spec:pgorm:sem:codegen.entity.serde.skip]
> Two flags add field-level serde attributes, and both are effective only
> when serde output is enabled: `serde_skip_hidden_column` (any of
> `Serialize`/`Deserialize`/`Both`) puts `#[serde(skip)]` on every column
> whose DB name starts with `_`; `serde_skip_deserializing_primary_key`
> (`Deserialize` or `Both` only) puts `#[serde(skip_deserializing)]` on
> primary-key fields. The hidden-column check wins: a hidden primary key
> gets `#[serde(skip)]`, not `#[serde(skip_deserializing)]`. With
> `WithSerde::None` both flags are inert.

> [spec:pgorm:sem:codegen.entity.serde.derives]
> `WithSerde::extra_derive` is the single producer of the serde derive
> fragment: an empty stream for `None`, otherwise a leading-comma fragment
> (`, Serialize`, `, Deserialize`, or `, Serialize, Deserialize` for
> `Both`). Model writers in both formats splice it at a fixed slot — after
> the base derives and the conditional `Eq`, before `model_extra_derives` —
> so a serde-enabled compact Model derives exactly
> `Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize`
> (pinned by `tests/compact_with_serde/cake_both.rs`). The same fragment is
> appended to generated active enums after their base derives and optional
> `Copy` (`codegen.entity.enums`). The fragment and the serde import
> (`codegen.entity.imports`) always travel together, both derived from the
> one `WithSerde` value in the context.

## Primary keys

> [spec:pgorm:sem:codegen.entity.pk]
> In the expanded format, `impl PrimaryKeyTrait for PrimaryKey` sets
> `type ValueType` to the single PK column's Rust type, or to a tuple
> `(T1, T2, ...)` of the column types for composite keys, and
> `fn auto_increment() -> bool` returns true when any column of the table
> is auto-increment (the check is over all columns, not just PK columns).
> In the compact format the same facts surface as the `primary_key` /
> `auto_increment = false` field attributes described in
> `codegen.entity.compact.attrs`.

## Relations

> [spec:pgorm:sem:codegen.entity.relations]
> `Relation` enum variants are named by the UpperCamelCase of the referenced
> table, with two adjustments: a self-referencing relation is named
> `SelfRef`, and a nonzero `num_suffix` is appended (`Fruit1`, `Fruit2`,
> `SelfRef1`, ...). In the compact format each variant carries a `#[pgorm]`
> attribute: FK-owning relations emit
> `#[pgorm(belongs_to = "<Entity>", from = "Column::<Src>", to = "<Entity path>Column::<Ref>", on_update = "<Action>", on_delete = "<Action>")]`
> where `<Entity>` is `super::<module>::Entity` (or plain `Entity` when
> self-referencing), multi-column FKs render `from`/`to` as parenthesized
> tuples `"(Column::A, Column::B)"`, and `on_update`/`on_delete` appear only
> when the FK declared an action (`Restrict`, `Cascade`, `SetNull`,
> `NoAction`, `SetDefault`). Inverse relations emit
> `#[pgorm(has_one = "...")]` or `#[pgorm(has_many = "...")]` with no
> `from`/`to`. In the expanded format the same information renders as
> `RelationTrait::def()` match arms:
> `Entity::has_many(super::fruit::Entity).into()` and
> `Entity::belongs_to(...).from(...).to(...).into()`.

> [spec:pgorm:sem:codegen.entity.relations.related]
> For each relation that is not self-referencing, has `num_suffix == 0`, and
> still has `impl_related` set, both formats emit
> `impl Related<super::<module>::Entity> for Entity` with
> `fn to() -> RelationDef { Relation::<Variant>.def() }`. Self-referencing
> and suffixed relations get `Relation` variants but no `Related` impl
> (`Related` can only be implemented once per target entity), and
> conjunct-shadowed relations are replaced by the `via` impl of
> `codegen.entity.relations.via`.

> [spec:pgorm:sem:codegen.entity.relations.via]
> Each conjunct relation emits a many-to-many `Related` impl:
>
> `impl Related<super::<to>::Entity> for Entity` with
> `fn to() -> RelationDef { super::<via>::Relation::<To>.def() }` and
> `fn via() -> Option<RelationDef> { Some(super::<via>::Relation::<Self>.def().rev()) }`
>
> where `<via>` is the junction module, `<To>` the target's variant in the
> junction's `Relation` enum, and `<Self>` the current entity's variant
> there. These impls are emitted after the plain `Related` impls in both
> formats.

## Active enums

> [spec:pgorm:sem:codegen.entity.enums]
> All discovered database enums are generated into a single
> `pgorm_active_enums.rs`, in alphabetical order by enum name. Each becomes
> `#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]`
> (plus `Copy` when `with_copy_enums`, then serde derives, then
> `enum_extra_derives`) with
> `#[pgorm(rs_type = "String", db_type = "Enum", enum_name = "<db name>")]`
> and `enum_extra_attributes` as further attribute lines. The Rust enum
> name is the UpperCamelCase of the DB enum name; each variant carries
> `#[pgorm(string_value = "<db value>")]`. Variant naming: values starting
> with a digit get an underscore prefix (`3D` → `_3D`); values whose
> UpperCamelCase form is empty (punctuation-only, etc.) fall back to
> per-character encoding — ASCII chars as `U<hex, 4 digits>` and multi-byte
> chars kept verbatim (`/` → `U002F`, `你好` → `你好`) — with a warning
> printed to stdout; everything else is plain UpperCamelCase. Entity files
> using an enum column import it via
> `use super::pgorm_active_enums::<EnumName>;` (deduplicated per file), and
> the expanded `ColumnTrait::def()` renders the column as
> `<EnumName>::db_type()`.

## Identifier hygiene

> [spec:pgorm:sem:codegen.entity.keywords]
> Generated identifiers derived from DB names (module names, Model field
> names, table idents) pass through `escape_rust_keyword`: 49 strict/reserved
> Rust keywords are emitted as raw identifiers (`type` → `r#type`,
> `typeof` → `r#typeof`), and the three keywords that cannot be raw
> identifiers — `crate`, `self`, `Self` — get a trailing underscore
> (`crate` → `crate_`, `self` → `self_`). Field names are the snake_case of
> the column name; when that differs from the raw DB name, the DB name is
> preserved via `column_name` attributes (compact Model fields and expanded
> `Column` variants).

## Seaography support

> [spec:pgorm:sem:codegen.entity.seaography]
> With the `seaography` flag, both formats append a final
> `#[derive(Copy, Clone, Debug, EnumIter, DeriveRelatedEntity)] pub enum RelatedEntity`
> whose variants are, in order: every `Relation` variant name; a
> `<Name>Reverse` variant for every self-referencing relation; and the
> UpperCamelCase target of every conjunct relation. Each variant carries
> `#[pgorm(entity = "<Entity path>")]`, and relations that lack a `Related`
> impl (self-referencing, conjunct-shadowed, or suffixed) additionally carry
> `def = "Relation::<Variant>.def()"` — with `.def().rev()` for the
> `Reverse` variants — so Seaography can resolve them without `Related`.
