# Entity code generation (pgorm-codegen)

`pgorm-codegen` turns a discovered PostgreSQL schema into Rust entity source
files. The pipeline has two stages: `EntityTransformer::transform` converts a
list of `TableCreateStatement`s into an in-memory `Entity` model, and
`EntityWriter::generate` renders those entities into output files. Everything
below describes what the code emits today; text-level shapes are pinned by the
golden fixtures under `pgorm-codegen/tests/`. Callers with DDL text rather than
a live database reach the same pipeline through `sql_schema`, specified under
[Schema from DDL text](#schema-from-ddl-text).

> [spec:pgorm:def:codegen.entity+2]
> The entity generator is the pipeline
> `EntityTransformer::transform(Vec<TableCreateStatement>) -> EntityWriter`
> followed by `EntityWriter::generate(&EntityWriterContext) -> WriterOutput`.
> An `Entity` carries `table_name`, `schema_name` (the source table's own
> qualifier, `None` when it had none — `codegen.entity.transform`), `columns`
> (name, `ColumnType`, `auto_increment`, `not_null`, `unique`), `relations`,
> `conjunct_relations`,
> and `primary_keys`. A `WriterOutput` is a list of `OutputFile { name, content }`
> — the writer never touches the filesystem; callers (e.g. `pgorm-cli`) write
> the files and run `rustfmt` over them.
>
> `EntityWriterContext` selects all generation options: `expanded_format`,
> `with_serde`, `with_copy_enums`, `date_time_crate`, `schema_name`, `lib`,
> `serde_skip_deserializing_primary_key`, `serde_skip_hidden_column`,
> `model_extra_derives`, `model_extra_attributes`, `enum_extra_derives`,
> `enum_extra_derives`, and `enum_extra_attributes`. Extra derives are appended
> to the generated derive lists; extra attributes are emitted as additional
> `#[...]` lines on the Model struct or enum.
>
> Errors are the two-variant `Error` enum: `StdIoError(io::Error)` and
> `TransformError(String)`.

## Schema discovery → Entity model

> [spec:pgorm:sem:codegen.entity.transform+7]
> `EntityTransformer::transform` builds one `Entity` per input
> `TableCreateStatement`. A table's identity is the `TableIdent` its
> `TableName` spells: the bare name, and the schema qualifying it when the
> form is `SchemaTable`. The schema is kept, not discarded — it becomes the
> entity's `schema_name` and, through it, the `schema_name` the generated
> entity declares (`codegen.entity.context`). Discarding it made every
> generated statement depend on the session's `search_path`: an entity read
> from `tenant_a.item` named only `item`, so its CRUD could reach a different
> schema's `item`, and the schema the DDL bridge had faithfully preserved
> (`codegen.ddl.tables`) was thrown away one stage later. Every statement has
> a name, so the read cannot fail and the
> `TransformError("Table name should not be empty")` that stood in for the
> nameless case is gone with the state it guarded
> (`[spec:pgorm:req:sql.ddl.create-table+6]`), and MUST NOT come back.
>
> Identity is what every lookup keys on: the entity map, the per-target
> counters behind `num_suffix`, the `self_referencing` test, and the inverse
> and conjunct relations synthesised below — `tenant_a.item` and
> `tenant_b.item` are two tables, and a key onto one is not a key onto the
> other. A `Relation` carries its target's identity as `ref_schema` beside
> `ref_table`; only the bare name reaches the generated text, since a module
> path names one table and the collision gate is what makes that so
> (`codegen.entity.collisions`).
>
> A reference — a foreign key's target here, an index's or comment's table in
> the DDL bridge (`codegen.ddl.objects`) — resolves by one rule: a
> schema-qualified reference names exactly that table, and an unqualified one
> names the table written without a qualifier, failing that the one table
> with that bare name. The second clause is `search_path`'s reading of an
> unqualified name in the only case where it has one answer, and it is total
> wherever generation is possible at all: two tables sharing a bare name are
> refused before any reference is resolved (`codegen.entity.collisions`), so
> "the one table with that bare name" cannot be several. A reference that
> resolves to nothing keeps the name it was written with, and is reported by
> the closure check below.
>
> Every failure this gate reports names a table by its identity —
> `` table `tenant_a.item` `` for a qualified table, `` table `item` `` for an
> unqualified one — so a message about one of two same-named tables says
> which. `<table>` below is that identity.
>
> Per column: `auto_increment`, `not_null` and `unique` come from the
> presence of the matching `ColumnSpec` on the column definition; a column
> with no `ColumnType` yields
> ``TransformError("table `<table>` column `<column>`: column type should
> not be empty")``. The table's indexes then widen `unique`, never narrow
> it: a column is unique when its definition carries
> `ColumnSpec::UniqueKey`, or some unique index of the table covers that one
> column and nothing else. The two spellings of one fact therefore agree —
> a `ColumnSpec::UniqueKey` is no longer dropped on this path while the DDL
> bridge keeps it as the index Postgres creates for it
> (`codegen.ddl.tables`). Primary keys are collected
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
> (`codegen.entity.keywords`) and is that one name's alone
> (`codegen.entity.collisions`); that every primary-key name is the name of a
> column of its own table, else
> ``TransformError("table `<table>`: primary key column `<column>` is not a
> column of the table")``.
>
> Once every table has been read, the gate also checks that the schema is
> closed under its own foreign keys: each relation's referenced table is a
> table of this schema, each referenced column a column of that table, and
> each constrained column a column of the table that owns the key. A
> generated file names its target's module and columns, so an unresolved
> reference would otherwise reach the caller as Rust that does not compile —
> a `belongs_to` onto a module nobody generated — while the absent target
> takes no inverse and no conjunct relation with it, silently. The three
> failures are
> ``TransformError("table `<table>`: relation to `<ref table>` names a table
> the schema does not define")``,
> ``TransformError("table `<table>`: relation to `<ref table>` references
> column `<column>`, which `<ref table>` does not have")`` and
> ``TransformError("table `<table>`: relation to `<ref table>` constrains
> column `<column>`, which the table does not have")``. Every foreign key
> names a referenced table, so that read cannot fail and the
> ``TransformError("... referenced table should not be empty")`` that stood in
> for the tableless case is gone with the state it guarded
> (`[spec:pgorm:req:sql.ddl.foreign-key+3]`), and MUST NOT come back: the
> conversion is `From<&TableForeignKey> for Relation`, not a `TryFrom`.
>
> The reference and collision checks run over the tables as read, before
> inverse and conjunct relations are synthesised: those are the gate's own
> work, and these checks are what make the lookups that synthesise them
> total. Identifier validation runs over the final entities, after that
> synthesis, so the derived relations are covered too. The `format_ident!`
> and column-lookup sites downstream of the gate are therefore internal
> invariants, not caller-reachable failures.
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
> Entities are held in a `BTreeMap` keyed by identity, ordered by bare name
> before schema, so all outputs that iterate entities (entity files,
> `mod.rs`, `prelude.rs`) are ordered alphabetically by table name whatever
> the schemas sort like. Before writing, each entity's `relations` are sorted
> by referenced table name and its `conjunct_relations` by target name.

> [spec:pgorm:sem:codegen.entity.transform.inverse+1]
> For every non-self-referencing relation with `num_suffix == 0`, the
> transformer adds an inverse relation to the referenced entity, pointing
> back at the FK-owning table with empty column lists. The inverse type is
> `HasOne` when the FK is unique and `HasMany` otherwise, and a key is unique
> in any of three ways: every one of its columns is a unique column of the
> owning table; some unique index of the owning table covers exactly the
> key's columns, compared as sets; or the key's column set is exactly the
> owning table's full primary-key set. The middle case is the one a
> column-at-a-time reading cannot see — a composite `UNIQUE` over the key's
> columns constrains the key as a whole while leaving every column of it free
> on its own — and it is exact in both directions: a unique index over a
> superset of the key constrains the key not at all, and is not read as
> uniqueness. Self-referencing relations and
> suffixed relations produce no inverse (the suffixed case would emit a
> `Relation` variant with no usable `Related` impl). An inverse relation is
> dropped when the target entity already has any relation to that table.

> [spec:pgorm:sem:codegen.entity.transform.conjunct+1]
> A table is treated as a many-to-many junction when it is nothing but the
> join: exactly two of its relations are `BelongsTo` relations carrying
> columns of their own, and those two column sets together are exactly the
> table's primary key. Each of the two referenced entities then receives a
> `ConjunctRelation { via: junction_table, to: other_ref_table }`. Both
> conditions are load-bearing. Only the table's own foreign keys are legs:
> the inverse relations synthesised above belong to other tables' keys, so a
> table that is referenced is no more a junction for it, and a junction that
> is referenced is no less one — the classification does not depend on the
> order the transformer works in. And a table keyed by something of its own,
> with two foreign keys beside that key, joins nothing: its primary key is
> not the two keys' columns, so the pair is two references and not a join.
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

> [spec:pgorm:sem:codegen.entity.context+2]
> `EntityWriterContext::new` takes one `EntityWriterOptions` struct — every
> generation option is a named field, and its `Default` is the no-flags
> shape (compact format, no serde, chrono, `mod.rs`) — and returns
> `Result<EntityWriterContext, Error>`.
>
> The `schema_name` option is a **default, not an override**: an entity's
> schema name is the source table's own qualifier when it has one
> (`codegen.entity.transform`), and the option only when it has none. This is
> the direction that cannot lie. The qualifier is a fact the schema states
> about where the table is, and an option that overrode it would generate an
> entity naming a schema the table is demonstrably not in — the very failure
> discarding the qualifier used to produce. The option's job is the
> complementary one: naming the schema that discovery ran against, which is
> precisely the schema an unqualified name belongs to. So the two never
> disagree, and generating a mixed-schema DDL with `schema_name` set gives
> each table the schema it is actually in. `EntityWriter::gen_schema_name`
> takes the entity and the option and applies that precedence, so no
> generator can be written that forgets it; "a schema name is configured", in
> `codegen.entity.compact`, `codegen.entity.compact.model` and
> `codegen.entity.expanded`, means the result of it.
>
> `new` normalizes the option lists at construction:
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

> [spec:pgorm:def:codegen.entity.compact+1]
> The compact format (default, `expanded_format == false`) emits per entity,
> in order: the imports (`use pgorm::entity::prelude::*;`, serde imports,
> and one `use super::pgorm_active_enums::<EnumName>;` per distinct enum
> used by the entity's columns); a single `Model` struct deriving
> `Clone, Debug, PartialEq, DeriveEntityModel` (plus `Eq`, serde derives,
> and extra derives) annotated with
> `#[pgorm(schema_name = "...", table_name = "...")]` (`schema_name` only
> when the entity has one — its source table's qualifier, else the configured
> default, per `codegen.entity.context`); a `Relation` enum deriving
> `Copy, Clone, Debug, EnumIter, DeriveRelation` whose variants carry
> `#[pgorm(...)]` relation attributes (an entity with no relations emits the
> empty `pub enum Relation {}`); the `Related` impls; and
> `impl ActiveModelBehavior for ActiveModel {}`. `DeriveEntityModel`
> expands the Entity/Column/PrimaryKey machinery that the expanded format
> spells out.

> [spec:pgorm:def:codegen.entity.expanded+1]
> The expanded format (`expanded_format == true`) emits per entity, in
> order: the same imports; `#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
> pub struct Entity;`; `impl EntityName for Entity` containing
> `fn schema_name(&self) -> Option<&str>` returning `Some("...")` only when
> the entity has a schema name — its source table's qualifier, else the
> configured default, per `codegen.entity.context` — and
> `fn table_name(&self) -> &str` returning
> the table name literal, which is the bare name in either case; a `Model`
> struct deriving
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

> [spec:pgorm:sem:codegen.entity.compact.model+1]
> `gen_compact_model_struct` emits the compact `Model` as one block: the
> derive attribute `Clone, Debug, PartialEq, DeriveEntityModel` followed by
> the `Eq` slot, the serde fragment (`codegen.entity.serde.derives`), and
> `model_extra_derives`; the struct attribute
> `#[pgorm(schema_name = "...", table_name = "...")]` (the `schema_name =`
> part present only when the entity has a schema name, and carrying the
> entity's, not the option's, when the two differ —
> `codegen.entity.context`); then
> `model_extra_attributes` as further attribute lines. Fields follow the
> entity's column order; each field is the keyword-escaped snake_case
> column name (`codegen.entity.keywords`) typed per `codegen.entity.types`,
> preceded first by its assembled `#[pgorm(...)]` attribute
> (`codegen.entity.compact.attrs`, omitted when no parts apply) and then by
> its serde attribute (`codegen.entity.serde.skip`). Primary-key membership
> is decided by matching the raw DB column name against the entity's
> `primary_keys` list.

> [spec:pgorm:sem:codegen.entity.expanded.blocks+1]
> `gen_expanded_code_blocks` assembles an entity's expanded file as an
> ordered `Vec<TokenStream>`, one block per section, each produced by a
> dedicated generator: a single import block (`gen_import` extended with
> `gen_import_active_enum`); `gen_entity_struct`; `gen_impl_entity_name`;
> `gen_model_struct`; `gen_column_enum`; `gen_primary_key_enum`;
> `gen_impl_primary_key`; `gen_relation_enum`; `gen_impl_column_trait`;
> `gen_impl_relation_trait`; then zero or more `gen_impl_related` blocks,
> zero or more `gen_impl_conjunct_related` blocks, then
> `gen_impl_active_model_behavior` last. `write_entities` prepends the generated-file
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
> (`[spec:pgorm:sem:macros.derive.entity-model.column-def+4]`).
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

> [spec:pgorm:req:codegen.entity.collisions+1]
> Every identity the writer derives from a DB name MUST be one name's alone,
> and the `transform` gate refuses a schema in which two names claim one. The
> namespaces checked are: the module name a table derives — and with it the
> file name, which is the same string — and the type name a table derives,
> both across the schema; the field name a column derives, within its table;
> the type name an enum derives, across the schema; and the variant name a
> value derives, within its enum. Keyword escaping
> (`codegen.entity.keywords`) is applied before the comparison, so the rescue
> cannot itself introduce a collision.
>
> Two tables that share a bare name across schemas — `tenant_a.item` and
> `tenant_b.item`, or `tenant_a.item` and a bare `item`, whose sameness is
> `search_path`'s business and not the generator's — are the case where both
> DB names are right and the collision is the generator's, not the schema's:
> they each need a module, they cannot be told apart by the only name a
> module has, and a run has one output directory to put them in.
> They are refused first, before any table is read, because the schemas that
> tell them apart are exactly what every later lookup by bare name would have
> to guess at (`codegen.entity.transform`) — the refusal is what makes the
> unqualified-reference rule total. The message names both identities and the
> way out:
> ``TransformError("tables `tenant_a.item` and `tenant_b.item` both generate
> the module name `item`: same-named tables in different schemas are different
> tables, and need one generation run each")``. One identity passed twice is a
> different mistake and says so: ``TransformError("table `<table>` is declared
> twice")``, the wording the DDL bridge already uses for it
> (`codegen.ddl.unsupported`).
>
> Every other refusal names both DB names — as identities, so a qualified
> table is named with its schema — and the identifier they share:
> ``TransformError("tables `<first>` and `<second>` both generate the module
> name `<derived>`")``, and in the same shape
> ``tables `<first>` and `<second>` both generate the type name `<derived>` ``,
> ``table `<table>` columns `<first>` and `<second>` both generate the field
> name `<derived>` ``,
> ``enums `<first>` and `<second>` both generate the type name `<derived>` ``
> and ``enum `<enum>` values `<first>` and `<second>` both generate the
> variant name `<derived>` ``. `<first>` is whichever name the gate reached
> first, in the order the schema is held in — alphabetical by table name for
> tables and enums, declaration order for columns and enum values.
>
> Case conversion is many-to-one — `CakeFilling`, `cake filling` and
> `cake_filling` all derive `cake_filling` — so without this check a schema
> holding two such names generates one file over the other and declares the
> module twice, and the duplicate definition lands in the caller's build
> rather than in ours. This is the same contract as
> `codegen.entity.keywords`, on the other axis: that rule is about a name
> having a Rust form at all, this one about that form being unshared.

## Schema from DDL text

`pgorm_codegen::sql_schema` reads a `schema.sql` — DDL text, with no database to
inspect — using libpg_query, the PostgreSQL server's own parser, and bridges the
parse tree into the statements the transformer already consumes. `pg_query` is a
plain dependency of `pgorm-codegen` rather than an optional one: the crate is a
build-time tool nothing links into a running application, so the cost of
compiling the C parser falls on people generating entities and on nobody else.

> [spec:pgorm:def:codegen.ddl+2]
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
> `pgorm-query` and rendered through their `Display`
> (`sql.ddl.create-table`, `sql.ddl.type-enum`) MUST, when parsed back, generate
> the same entity files as the statements themselves — with no asymmetry left
> to document. There was one: a column carrying `ColumnSpec::UniqueKey`, which
> the bridge preserves as the unique index Postgres creates for it
> (`codegen.ddl.tables`) while `transform` discarded it on the statement path,
> so the round trip gained a `unique` the statement path dropped. `transform`
> now reads that spec (`codegen.entity.transform`) and the two paths agree.

> [spec:pgorm:req:codegen.ddl.unsupported+1]
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
> downstream, so a duplicate would otherwise overwrite in silence. A foreign
> key naming a table the file never creates, or a column that table does not
> have, is refused too, but by the transform gate `entities_from_sql` runs
> (`codegen.entity.transform`) rather than here: the bridge resolves one
> statement against the file, and the gate holds every table at once, which is
> where the answer to a foreign key is known.
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

> [spec:pgorm:sem:codegen.ddl.tables+2]
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
> unique index on the table, which is what Postgres itself creates for it and
> so the truthful form to bridge it as; `codegen.entity.transform` reads that
> index and a `ColumnSpec::UniqueKey` alike, so the fact survives either
> spelling. A primary-key column is `NOT NULL`
> whether or not the DDL spells it, which is Postgres' own rule: the entity
> model reads nullability off the column alone, so an unstated `NOT NULL` would
> otherwise generate an `Option` primary key.
>
> Table-level `PRIMARY KEY` and `UNIQUE` constraints become the table's
> primary-key and unique indexes, keeping the constraint name and
> `NULLS NOT DISTINCT`; a table-level `FOREIGN KEY` becomes a foreign key with
> its columns, referenced table and referenced columns, and both forms keep the
> constraint name. A foreign key whose two column lists differ in length is a
> named rejection rather than a truncated key — the pairs are what the bridged
> statement is built from (`[spec:pgorm:req:sql.ddl.foreign-key+3]`), and
> Postgres itself rejects the mismatch at parse analysis.
> Referential actions map `RESTRICT`, `CASCADE`, `SET NULL` and
> `SET DEFAULT` onto `ForeignKeyAction`. `NO ACTION` reads as no action
> declared: the grammar fills that same code in when a foreign key declares
> nothing, the two are indistinguishable in the parse tree, and it is
> Postgres' default — so the generated relation carries an `on_update` or
> `on_delete` exactly where the schema chose something other than the default.

> [spec:pgorm:sem:codegen.ddl.objects+1]
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
> `COMMENT ON COLUMN` a `ColumnSpec::Comment` on the named column. Neither
> comment reaches the generated entities, which have no comment surface — they
> ride on the statements so a caller reading `parse_schema`'s output still has
> them.
>
> Tables are held by identity — schema and bare name, the same key the
> transformer holds them under (`codegen.entity.transform`) — and an index or
> comment attaches by resolving its own target against them, by that rule: a
> qualified target names exactly that table, an unqualified one the table
> written without a qualifier, failing that the one table with that bare
> name. So a schema-qualified `CREATE INDEX` or `COMMENT ON` reaches its own
> table rather than whichever table happened to share the bare name, and a
> `CREATE INDEX` a qualifier makes generated for a table the file never
> creates is ``unresolved("no CREATE TABLE for table `tenant_b.item`")``
> rather than a fact quietly folded into `tenant_a.item`. The unqualified
> case is the only one that can be answered by more than one table, and it is
> named rather than guessed at:
> ``TransformError("statement <n>: `item` names more than one table")``. Two
> tables with one bare name cannot be generated at all
> (`codegen.entity.collisions`), so this is reachable only from
> `parse_schema` read on its own. `is declared twice` likewise compares
> identities: two `CREATE TABLE`s for one bare name in different schemas are
> two tables, and the file is read.
