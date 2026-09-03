# Entity code generation (pgorm-codegen)

`pgorm-codegen` turns a discovered PostgreSQL schema into Rust entity source
files. The pipeline has two stages: `EntityTransformer::transform` converts a
list of `TableCreateStatement`s into an in-memory `Entity` model, and
`EntityWriter::generate` renders those entities into output files. Everything
below describes what the code emits today; text-level shapes are pinned by the
golden fixtures under `pgorm-codegen/tests/`. Callers with DDL text rather than
a live database reach the same pipeline through `sql_schema`, specified under
[Schema from DDL text](#schema-from-ddl-text).

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

> [spec:pgorm:sem:codegen.entity.transform+3]
> `EntityTransformer::transform` builds one `Entity` per input
> `TableCreateStatement`. The table name is unpacked from either `TableName`
> form (`Table` and `SchemaTable`, the schema discarded); a statement with no
> table name yields `Error::TransformError("Table name should not be empty")`.
>
> Per column: `auto_increment` and `not_null` come from the presence of the
> matching `ColumnSpec` on the column definition; a column with no
> `ColumnType` yields
> ``TransformError("table `<table>` column `<column>`: column type should
> not be empty")``. `unique` does not: the transformer overwrites the value
> the `TryFrom<&ColumnDef> for Column` conversion derived, assigning
> `unique` solely from the table's
> indexes — true exactly when some unique index of the table covers that one
> column and nothing else. A `ColumnSpec::UniqueKey` on the column
> definition is therefore discarded on this path; it takes effect only when
> a `ColumnDef` is converted through the public `TryFrom<&ColumnDef> for
> Column` impl directly, outside `transform`. Primary keys are collected
> from `ColumnSpec::PrimaryKey` markers and extended with the column names
> of any table-level primary-key index. Every column whose (possibly
> array-inner) type is `ColumnType::Enum` registers an `ActiveEnum` in a
> `BTreeMap` keyed by enum name, deduplicating across tables.
>
> `transform` is the pipeline's validation gate: every failure a caller's
> schema can cause MUST come back from this one call as a `TransformError`,
> leaving the writer that follows infallible on its output. Besides the
> table name and the column types (`codegen.entity.types.unsupported`), the
> gate checks that every identifier the writer will derive from a DB name —
> table, column, primary key, relation target, conjunct relation, enum name
> and enum value, in each of the snake_case, camel-case and keyword-escaped
> forms the generators use — has a legal Rust form
> (`codegen.entity.keywords`); that every primary-key name is the name of a
> column of its own table, else
> ``TransformError("table `<table>`: primary key column `<column>` is not a
> column of the table")``; and that every foreign key names a referenced
> table, else ``TransformError("table `<table>` foreign key on `<columns>`:
> referenced table should not be empty")``. Validation runs over the final
> entities, after inverse and conjunct relations are synthesised, so the
> derived relations are covered too. The `format_ident!` and column-lookup
> sites downstream of the gate are therefore internal invariants, not
> caller-reachable failures.
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

> [spec:pgorm:sem:codegen.entity.context+1]
> `EntityWriterContext::new` takes one `EntityWriterOptions` struct — every
> generation option is a named field, and its `Default` is the no-flags
> shape (compact format, no serde, chrono, `mod.rs`) — and returns
> `Result<EntityWriterContext, Error>`. It normalizes the option lists at
> construction:
> `model_extra_derives` / `enum_extra_derives` pass through `bonus_derive`,
> which parses each string as a `TokenStream` and folds them into a single
> leading-comma fragment (`, A, B`); `model_extra_attributes` /
> `enum_extra_attributes` pass through `bonus_attributes`, which wraps each
> parsed string in its own `#[...]` attribute line. An extra derive or
> attribute that is not valid Rust token text is returned as
> ``TransformError("`<option>` entry `<string>` is not valid Rust token
> text")`` — naming the option field it came from — before any file is
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

> [spec:pgorm:sem:codegen.entity.compact.attrs+1]
> In the compact Model, each field's `#[pgorm(...)]` attribute assembles
> parts in this fixed order: `column_name = "..."` when the DB column name
> is not already snake_case; `primary_key` when the column is in the primary
> key, followed by `auto_increment = false` when that PK column is not
> auto-increment; `column_type = "..."` for exactly the types whose default
> mapping is ambiguous — `Float`, `Double`, `Decimal(Some((p, s)))`,
> `Money`, `Text`, `JsonBinary`, `custom("...")`, `Bytea` — with
> `nullable` appended (only
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

> [spec:pgorm:sem:codegen.entity.types+2]
> Model field types come from `Column::get_rs_type`: a non-null column maps
> to `T`, a nullable column to `Option<T>`, where `T` is:
>
> | ColumnType | Rust type |
> |---|---|
> | `Char(_)`, `String(_)`, `Text`, `Custom(_)` | `String` |
> | `SmallInteger` / `Integer` / `BigInteger` | `i16` / `i32` / `i64` |
> | `Float` / `Double` | `f32` / `f64` |
> | `Json`, `JsonBinary` | `Json` |
> | `Decimal(_)`, `Money` | `Decimal` |
> | `Uuid` | `Uuid` |
> | `Bytea` | `Vec<u8>` |
> | `Boolean` | `bool` |
> | `Enum { name, .. }` | UpperCamelCase of `name` |
> | `Array(inner)` | `Vec<T(inner)>` (recursive) |
> | `Date`, `Time`, `Timestamp`, `TimestampWithTimeZone` | per `codegen.entity.types.datetime+1` |
>
> No row produces an unsigned Rust integer: Postgres has no unsigned integer
> type, so a generated field MUST NOT claim one.
>
> The named types resolve through `pgorm::entity::prelude::*`.
>
> The `Eq` derive is added to the Model derive list only when no column's
> type is `Float` or `Double`, checked recursively through `Array` element
> types; a single float column suppresses `Eq` for the whole Model.

> [spec:pgorm:sem:codegen.entity.types.datetime+1]
> `DateTimeCrate` selects the date/time field types:
>
> | ColumnType | `Chrono` | `Time` |
> |---|---|---|
> | `Date` | `Date` | `TimeDate` |
> | `Time` | `Time` | `TimeTime` |
> | `Timestamp` | `DateTime` | `TimeDateTime` |
> | `TimestampWithTimeZone` | `DateTimeWithTimeZone` | `TimeDateTimeWithTimeZone` |
>
> `Timestamp` MUST map to the time-zone-naive type (`chrono::NaiveDateTime`,
> re-exported from the prelude as `DateTime`), because that is what Postgres
> `timestamp` is; mapping it to `DateTimeUtc` claimed a time zone the column
> does not carry, and disagreed with the inference table's
> `NaiveDateTime`→`Timestamp` direction
> (`[spec:pgorm:sem:macros.derive.entity-model.column-def+3]`).
>
> Limitation: only `Chrono` is usable in practice. The `TimeDate`-family
> aliases in `pgorm::entity::prelude` are gated behind a `with-time` cargo
> feature that pgorm's `Cargo.toml` does not define (only `with-chrono` is a
> default feature), and pgorm's `tokio-postgres` dependency is built with
> `with-chrono-0_4` only — so code generated with `DateTimeCrate::Time` does
> not compile against pgorm as shipped.

> [spec:pgorm:req:codegen.entity.types.unsupported+1]
> Column types outside the mapping table are not supported, and support is
> decided when a `Column` is built rather than when it is rendered. Both
> `TryFrom<&ColumnDef> for Column` and — through it —
> `EntityTransformer::transform` MUST reject an unsupported type with
> ``TransformError("table `<table>` column `<column>`: column type <type> is
> not supported by codegen")``, where `<type>` is the `ColumnType`'s `Debug`
> form; outside `transform` the message names the column alone. `Array`
> element types are checked recursively, so an array of an unsupported
> element type is itself unsupported.
>
> Codegen MUST NOT be expected to degrade gracefully on such a type: no
> placeholder code is emitted and no file is generated, the whole run fails.
> It MUST NOT abort by panic either — the failure is a value the caller
> handles. Because no `Column` can hold an unsupported type, the wildcard
> arms of `Column::get_rs_type` and `Column::get_def` are unreachable and
> stand as internal invariants.

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

> [spec:pgorm:sem:codegen.entity.relations+1]
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
> `Entity::belongs_to(...).columns(<src>, <ref>).into()`, with each further
> column of a composite FK appended as `.and_columns(<src>, <ref>)`. A relation
> whose constrained and referenced column lists differ in length is rejected by
> `Relation::validate`, so no generated relation can name a column on one side
> without its counterpart on the other.

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

> [spec:pgorm:sem:codegen.entity.enums+1]
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
> printed to stdout as `transform` validates the enum; everything else is
> plain UpperCamelCase. A value whose derived variant name is still not a
> legal Rust identifier (a multi-byte character that cannot start one, a
> digit-led value carrying a space) is a `TransformError` per
> `codegen.entity.keywords`, not a panic. Entity files
> using an enum column import it via
> `use super::pgorm_active_enums::<EnumName>;` (deduplicated per file), and
> the expanded `ColumnTrait::def()` renders the column as
> `<EnumName>::db_type()`.

## Identifier hygiene

> [spec:pgorm:sem:codegen.entity.keywords+1]
> Generated identifiers derived from DB names (module names, Model field
> names, table idents) pass through `escape_rust_keyword`: 49 strict/reserved
> Rust keywords are emitted as raw identifiers (`type` → `r#type`,
> `typeof` → `r#typeof`), and the three keywords that cannot be raw
> identifiers — `crate`, `self`, `Self` — get a trailing underscore
> (`crate` → `crate_`, `self` → `self_`). Field names are the snake_case of
> the column name; when that differs from the raw DB name, the DB name is
> preserved via `column_name` attributes (compact Model fields and expanded
> `Column` variants).
>
> Keyword escaping is the only rescue on offer. A DB name whose derived form
> is still not a legal Rust identifier — empty after case conversion, all
> digits, or carrying characters an identifier cannot hold (`1`, `-`,
> punctuation-only) — is not mangled into something else: the shared
> `safe_ident` helper rejects it, and every such derivation is put through
> that helper at the `transform` gate, so the failure is
> ``TransformError("<what>: `<derived>` is not a valid Rust identifier")``
> where `<what>` names the table, column, primary key, relation, conjunct
> relation, enum or enum value it came from. The `format_ident!` call sites
> in the writer are downstream of that check and cannot panic on validated
> input.

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

## Schema from DDL text

`pgorm_codegen::sql_schema` reads a `schema.sql` — DDL text, with no database to
inspect — using libpg_query, the PostgreSQL server's own parser, and bridges the
parse tree into the statements the transformer already consumes. `pg_query` is a
plain dependency of `pgorm-codegen` rather than an optional one: the crate is a
build-time tool nothing links into a running application, so the cost of
compiling the C parser falls on people generating entities and on nobody else.

> [spec:pgorm:def:codegen.ddl]
> `sql_schema::parse_schema(&str) -> Result<Vec<TableCreateStatement>, Error>`
> parses DDL text with `pg_query::parse` and returns one statement per
> `CREATE TABLE`, in file order, with every other statement it read folded into
> the table that statement describes.
> `sql_schema::entities_from_sql(&str, EntityWriterOptions) -> Result<WriterOutput, Error>`
> runs the whole pipeline: it builds the `EntityWriterContext` first — so an
> unusable option is reported before the schema is read — then `parse_schema`,
> `EntityTransformer::transform` and `EntityWriter::generate`.
>
> Both report failure as `Error::TransformError`, the channel the transform gate
> already uses, and neither panics on any input: text the grammar rejects comes
> back as ``TransformError("schema SQL did not parse: <parser message>")``,
> carrying libpg_query's own message. The bridge validates only what it must to
> build statements; whether the result can be generated at all stays the
> transform gate's decision (`codegen.entity.transform`), so a column type
> `pgorm-query` can spell but `codegen.entity.types.unsupported` cannot render
> is refused there, by name, rather than here.
>
> The bridge is the inverse of the DDL builder: statements built with
> `pgorm-query` and rendered through `SchemaStatementBuilder`
> (`sql.ddl.create-table`, `sql.ddl.type-enum`) MUST, when parsed back, generate
> the same entity files as the statements themselves. One documented asymmetry:
> a column carrying `ColumnSpec::UniqueKey`, which `transform` discards on the
> statement path (`codegen.entity.transform`) but which the bridge preserves as
> the unique index Postgres creates for it (`codegen.ddl.tables`), so the
> round trip yields a `unique` the statement path drops.

> [spec:pgorm:req:codegen.ddl.unsupported]
> The supported subset is what the entity model can hold: `CREATE TABLE` with
> its columns, `NULL`/`NOT NULL`, primary-key, unique and foreign-key
> constraints; `CREATE TYPE ... AS ENUM`; `CREATE INDEX`; and `COMMENT ON TABLE`
> / `COMMENT ON COLUMN`. Everything else in the file MUST be reported — never
> skipped, never quietly reinterpreted. A construct outside the subset is
> ``TransformError("unsupported DDL: <what> at statement <n>")``; a construct
> inside it that this schema cannot resolve is
> ``TransformError("statement <n>: <problem>")``. `<n>` is the statement's
> 1-based position in the text, and `<what>` names the construct the way its
> author wrote it — `ALTER TABLE`, `CREATE TRIGGER`,
> ``a PARTITION BY clause on table `t` ``,
> ``a DEFAULT clause on column `t`.`c` `` — with `an unrecognised statement` as
> the fallback for a statement kind the namer does not know.
>
> Named rejections MUST cover at least: every statement other than the four
> above; `INHERITS`, `PARTITION BY`, `PARTITION OF`, `OF <type>`, `LIKE`, `WITH`
> storage options, `TABLESPACE`, `USING <access method>`, `ON COMMIT`,
> catalog-qualified table names and temporary or unlogged tables; column
> `DEFAULT`, `CHECK`, `GENERATED`,
> identity, `COLLATE`, `STORAGE` and `COMPRESSION` clauses; table-level `CHECK`
> and `EXCLUDE` constraints, deferrable and `NO INHERIT` constraints, `INCLUDE`
> columns, constraint index and storage options, and `MATCH` clauses;
> `REFERENCES` without a referenced column list, which no catalog is present to
> resolve; index `WHERE`, `INCLUDE`, `CONCURRENTLY`, `COLLATE`, operator
> classes, `NULLS FIRST`/`NULLS LAST`, expression columns, tablespaces and
> storage options; and `COMMENT ON` any object other than a table or a column.
> Type spellings outside the vocabulary are named the same way
> (`codegen.ddl.types`).
>
> Unresolved references are named as well: an index or comment naming a table
> the file never creates, a column comment naming a column its table does not
> have, and a table or enum type declared twice — both are keyed by name
> downstream, so a duplicate would otherwise overwrite in silence.
>
> One construct is accepted without being carried, and only this one: a
> non-unique `CREATE INDEX`. It states no fact the entity model holds —
> `codegen.entity.transform` reads unique and primary-key indexes and nothing
> else — and `pgorm-query` renders an index embedded in a `CREATE TABLE` as a
> constraint (`sql.ddl.create-table`), so carrying one would emit DDL Postgres
> rejects. Its table must still exist.

> [spec:pgorm:sem:codegen.ddl.types+2]
> Column types map back through the `ColumnType` → Postgres spelling contract of
> `sql.ddl.column-types`, read over the names the grammar produces: keyword
> spellings arrive qualified as `pg_catalog.<name>`, everything else bare, and
> both forms are accepted. `bpchar` → `Char`, `varchar` → `String`,
> `text` → `Text`, `int2`/`int4`/`int8` →
> `SmallInteger`/`Integer`/`BigInteger`, `float4`/`float8` → `Float`/`Double`,
> `numeric` → `Decimal`, `timestamp` → `Timestamp`,
> `timestamptz` → `TimestampWithTimeZone`,
> `time` → `Time`, `date` → `Date`, `interval` → `Interval(Any(None))`,
> `bool` → `Boolean`, `money` → `Money`, `bytea` → `Bytea`, `bit` → `Bit`,
> `varbit` → `VarBit`, `json` → `Json`, `jsonb` → `JsonBinary`, `uuid` → `Uuid`,
> `inet`/`cidr`/`macaddr`/`ltree` → `Inet`/`Cidr`/`MacAddr`/`LTree`,
> `vector` → `Vector`. `serial`, `bigserial` and `smallserial` (and
> `serial4`/`serial8`/`serial2`) are `Integer`/`BigInteger`/`SmallInteger` plus
> the auto-increment fact the renderer spells as the serial family. A name the
> file declared as an enum type resolves to `ColumnType::Enum` carrying that
> type's values, and an unqualified name is looked up as an enum before the
> table above. A type argument the vocabulary can hold is kept
> (`varchar(255)`, `numeric(10, 2)`, `bit(8)`, `vector(3)`), a single-argument
> `numeric(p)` reading as `Decimal(Some((p, 0)))` — its own meaning. An array
> bound wraps the element type in `Array`; only one unsized `[]` is accepted.
>
> The map is close to a bijection because the forward contract
> (`[spec:pgorm:req:sql.ddl.column-types+3]`) no longer spells one Postgres
> type under several names: `bytea`, `timestamp`, `smallint` and `money` each
> have exactly one `ColumnType` to come back to, so `Bytea`, `Timestamp`,
> `SmallInteger` and `Money` are recovered rather than chosen from a set. Where
> the contract is still many-to-one the reverse takes the faithful branch and
> the collapse is stated rather than hidden: `varchar` reads as
> `String(StringLen::None)`, since `StringLen::Max` renders the same bare
> `varchar` and Postgres has no `varchar(max)` to have written it. Every other
> `ColumnType` in the vocabulary has a reverse. `ColumnType::Custom`
> is deliberately not produced: a type name outside the table above is an error,
> not a `String` column that quietly means something else. `varbit` without a
> length, a modifier the vocabulary cannot hold (`timestamp(3)`), a sized or
> multi-dimensional array, and a non-integer type modifier are all named
> rejections per `codegen.ddl.unsupported`.

> [spec:pgorm:sem:codegen.ddl.tables]
> A `CREATE TABLE` becomes a `TableCreateStatement` carrying the `TableName`
> its name spells — `Table`, or `SchemaTable` when it is schema-qualified;
> a catalog-qualified `db.schema.table` names a cross-database reference
> Postgres does not implement and `TableName` cannot hold, so it is a named
> rejection rather than a name quietly shortened to its last two parts. The
> statement also carries its `IF NOT EXISTS` flag and one `ColumnDef` per
> column definition, in declaration order. Column constraints set the matching
> `ColumnSpec` —
> `NOT NULL`, `NULL`, `PRIMARY KEY` — and a column-level `REFERENCES` becomes a
> foreign key on that one column. A column-level `UNIQUE` becomes a one-column
> unique index on the table, which is both what Postgres creates for it and the
> only form the entity model reads: `codegen.entity.transform` assigns `unique`
> from the table's indexes and discards a `ColumnSpec::UniqueKey`, so setting
> that spec instead would drop the fact. A primary-key column is `NOT NULL`
> whether or not the DDL spells it, which is Postgres' own rule: the entity
> model reads nullability off the column alone, so an unstated `NOT NULL` would
> otherwise generate an `Option` primary key.
>
> Table-level `PRIMARY KEY` and `UNIQUE` constraints become the table's
> primary-key and unique indexes, keeping the constraint name and
> `NULLS NOT DISTINCT`; a table-level `FOREIGN KEY` becomes a foreign key with
> its columns, referenced table and referenced columns, and both forms keep the
> constraint name. Referential actions map `RESTRICT`, `CASCADE`, `SET NULL` and
> `SET DEFAULT` onto `ForeignKeyAction`. `NO ACTION` reads as no action
> declared: the grammar fills that same code in when a foreign key declares
> nothing, the two are indistinguishable in the parse tree, and it is
> Postgres' default — so the generated relation carries an `on_update` or
> `on_delete` exactly where the schema chose something other than the default.

> [spec:pgorm:sem:codegen.ddl.objects]
> Statements are resolved against each other rather than in file order: a
> `CREATE TYPE ... AS ENUM` may follow the table whose column names it, and a
> `CREATE INDEX` or `COMMENT ON` may precede its table. An enum type contributes
> its name and values to every column typed with it (`codegen.ddl.types`), which
> is where `transform` discovers enums; an enum type no column names contributes
> nothing and is returned as no statement of its own.
>
> A unique `CREATE INDEX` is folded into its table's indexes, keeping its name,
> columns, `ASC`/`DESC` ordering, `NULLS NOT DISTINCT`, `IF NOT EXISTS` and
> access method (`btree` is the default, `hash` → `IndexType::Hash`,
> `gin` → `IndexType::FullText`, anything else `IndexType::Custom`);
> `codegen.entity.transform` then reads a single-column unique index as that
> column's `unique` flag. `COMMENT ON TABLE` becomes the statement's comment and
> `COMMENT ON COLUMN` a `ColumnSpec::Comment` on the named column; both accept a
> schema-qualified target and both attach by table name, the same key the
> transformer holds tables under. Neither comment reaches the generated
> entities, which have no comment surface — they ride on the statements so a
> caller reading `parse_schema`'s output still has them.
