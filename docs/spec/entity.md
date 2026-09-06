# Entity system

The entity layer (`src/entity/`) defines the trait family that maps Rust types onto
PostgreSQL tables: entities, models, active models, columns, primary keys, relations,
links, and active enums. This spec captures what the code does today, including its
explicit limitations.

## Entity traits

> [spec:pgorm:def:entity.traits+1]
> The entity trait family is defined in `src/entity/` and re-exported from
> `pgorm::entity` (`src/entity/mod.rs`). `IdenStr` (`base_entity.rs`) is the base
> identifier contract: `Iden + Copy + Debug + 'static` plus `as_str(&self) -> &str`.
> It MUST NOT be named `IdenStatic`: `pgorm_query::IdenStatic` is a different
> trait with the same method name and an incompatible signature
> (`-> &'static str`, `[spec:pgorm:def:sql.types+2]`), and the two are reachable
> together, so sharing the name made implementing the wrong one an unreadable
> unsatisfied-bound error. They cannot be one trait: an entity's name is
> borrowed from `self` through `EntityName::table_name`, not `'static`.
> `EntityName: IdenStr + Default` maps an entity to a table. `EntityTrait: EntityName`
> is the abstract entity, carrying five associated types: `Model`
> (`ModelTrait<Entity = Self> + FromQueryResult`), `ActiveModel`
> (`ActiveModelBehavior<Entity = Self>`), `Column` (`ColumnTrait`), `Relation`
> (`RelationTrait`), and `PrimaryKey`
> (`PrimaryKeyTrait + PrimaryKeyToColumn<Column = Self::Column>`).
>
> The trait family is normally produced by the derive macros exported from
> `src/entity/prelude.rs` (`DeriveEntityModel`, `DeriveActiveModel`, `DerivePrimaryKey`,
> `DeriveRelation`, and friends), but every trait can be implemented by hand.

> [spec:pgorm:req:entity.traits.entity-name+1]
> `EntityName::table_name` is the only required method and MUST return the table's SQL
> name. `schema_name` and `comment` default to `None`; `module_name` defaults to
> `table_name()`. `table_ref` MUST produce a schema-qualified `TableName`
> (`(Alias::new(schema), entity)`) when `schema_name` returns `Some`, and a bare
> `TableName::Table` otherwise (`src/entity/base_entity.rs`). It is the DDL-position
> type, so schema projection targets take it directly; query positions widen it to a
> `FromItem` through `IntoFromItem`. All generated SQL that names the table goes
> through `table_ref`, so a `Some` schema name qualifies every statement.

> [spec:pgorm:req:entity.traits.crud+3]
> `EntityTrait` provides the static *read* surface (`src/entity/base_entity.rs`):
> `find()` returns a fresh `Select<Self>`; `find_by_id(values)` builds on `find()` by
> adding an equality filter per primary-key column, consuming the value tuple in
> primary-key iteration order. `delete_by_id(values)` returns a `DeleteMany`
> filtered per primary-key column like `find_by_id`; it stays because the
> per-column filter loop is real work, not a forward.
>
> `EntityTrait` MUST NOT carry `insert`, `insert_many`, `update`, `update_many`,
> `delete`, or `delete_many`. Each was a one-line forward to a builder
> constructor — `Insert::one`, `Insert::many`, `Update::one`, `Update::many`,
> `Delete::one`, `Delete::many` — and a second spelling of a constructor is a
> second thing to learn, a second place for docs to drift, and a second name
> in every reader's search. This follows the precedent of
> `entity.active-model.save`: where two spellings mean one thing, the surface
> keeps the one that says what it does. `Insert`, `TryInsert`, `Update`, and
> `Delete` are exported from `entity::prelude` so the surviving spelling is
> reachable wherever the deleted one was.
>
> `find_by_id` and `delete_by_id` MUST panic with `primary key arity mismatch` when the
> number of supplied values differs from the primary key's arity, in either direction.
> Values are accepted via `Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>`, so
> composite keys are passed as tuples.

> [spec:pgorm:def:entity.traits.column+4]
> `ColumnTrait: IdenStr + Iterable + FromStr` (`src/entity/column.rs`) describes one
> column of an entity. `def()` returns the column's `ColumnDef`; `entity_name()` and
> `as_column_ref()` qualify the column with its `EntityName`. The trait exposes an
> expression-building surface wrapping `pgorm_query::Expr`: comparison operators `eq`,
> `ne`, `gt`, `gte`, `lt`, `lte`; range `between` / `not_between`; pattern matching
> `like`, `not_like`, and the sugar `starts_with` (`s%`), `ends_with` (`%s`),
> `contains` (`%s%`); aggregates `max`, `min`, `sum`, `count`; null checks `is_null`,
> `is_not_null`, `if_null`; set membership `is_in` / `is_not_in`, its
> array-parameter counterpart `eq_any` / `ne_all`
> (`[spec:pgorm:req:sql.ast.expr.eq-any]`), and subqueries
> `in_subquery` / `not_in_subquery`; plus `into_expr` and `into_returning_expr`.
> The comparison and set-membership operators pass their values through `save_as`, so
> enum-typed columns compare against properly cast values; `eq_any` / `ne_all` pass
> their single array value through `save_array_as` instead, the whole array being one
> operand.
>
> The comparison operators above take a *value*, bound by `Into<Value>`. Comparing
> against another column, or against a computed expression, is a separate named
> family: `eq_col`, `ne_col`, `gt_col`, `gte_col`, `lt_col` and `lte_col` take any
> `ColumnTrait` and render `"a"."x" <op> "b"."y"`, each side qualified by its own
> entity; `eq_expr` takes any `Into<SimpleExpr>` for the computed case. `eq` and its
> siblings MUST NOT be widened to admit an expression operand: their operand goes
> through `save_as`, which is what casts an enum value, and a widened bound would drop
> that cast silently. The `_col` and `_expr` forms deliberately do not apply `save_as` —
> a column or an expression is already typed on the server side, and an enum column
> compared against another column of the same enum type needs no cast.
>
> `json_key()` names the key the column occupies in a JSON object, which is a
> different namespace from the SQL name `IdenStr::as_str` gives: the `with-json`
> conversions of `[spec:pgorm:req:entity.active-model.json+3]` deserialize through the
> entity's `Model`, so the key is the model field's name as `serde` spells it.
> `DeriveEntityModel` MUST emit it from the field the column was derived from and the
> `serde` renames declared on that field and its struct; the trait's default falls back
> to the SQL name, which is what a hand-written `Column` — having no field to read —
> can offer and what the two namespaces agree on when nothing is renamed.
>
> `ColumnType` is a re-export of `pgorm_query::ColumnType`; the crate's own `ColumnType`
> enum was dropped and `ColumnTypeTrait` (`def()`, `get_enum_name()`) bridges a
> `ColumnType` or existing `ColumnDef` into a `ColumnDef`.

> [spec:pgorm:req:entity.traits.column-def]
> `ColumnDef` (`src/entity/column.rs`) carries a column's definition attributes:
> `col_type: ColumnType`, `null`, `unique`, `indexed`, `default: Option<SimpleExpr>`,
> and `comment: Option<String>`. `ColumnTypeTrait::def()` MUST initialise a definition
> as non-null, non-unique, non-indexed, with no default and no comment. Builder methods
> flip individual attributes: `unique()`, `indexed()`, `null()` / `nullable()` (aliases),
> `comment(v)`, `default_value(T: Into<Value>)`, and `default(T: Into<SimpleExpr>)`
> (the latter accepting arbitrary expressions). `get_column_type()` and `is_null()`
> expose the type and nullability for introspection.

> [spec:pgorm:sem:entity.traits.column.enum-cast+1]
> Enum-typed columns are transparently cast at the SQL boundary
> (`src/entity/column.rs`). On read, `select_as` / `select_enum_as` casts an enum
> column to `text` — or `text[]` when the column type is `Array` of an enum — and
> leaves non-enum columns untouched. On write, `save_as` / `save_enum_as` casts the
> value to the enum's database type name — or `{enum_name}[]` for arrays. As a special
> case under the `with-json` + `postgres-array` features, saving into a `Json` /
> `JsonBinary` column flattens a `Value::Array` of JSON values into a single
> `Value::Json` array value instead of applying an enum cast.
>
> `save_array_as` is the array counterpart of `save_as`, for the operands that are one
> array value rather than one value per element: it casts to `{enum_name}[]` when the
> column's type is an enum or an array of one, and leaves every other column untouched.
> It does not take the JSON-flattening path — flattening an array into a scalar would
> destroy the operand `= ANY` needs. A column that overrides `save_as` with a cast of
> its own, as `#[pgorm(save_as = "…")]` generates, MUST override this too if its array
> comparisons are to carry the matching cast; the default knows only the enum case.

> [spec:pgorm:def:entity.traits.primary-key+2]
> `PrimaryKeyTrait: IdenStr + Iterable` (`src/entity/primary_key.rs`) defines an
> entity's primary key as an iterable enum of key columns. Its `ValueType` associated
> type is the Rust value form of the whole key and is bound by
> `Sized + Send + Debug + PartialEq + IntoValueTuple + TryFromValueTuple
> + TryGetableMany + TryFromU64 + PrimaryKeyArity`; `auto_increment()` reports whether the key is
> database-generated. `PrimaryKeyToColumn` maps key variants to columns (`into_column`)
> and back (`from_column -> Option<Self>`). `PrimaryKeyArity` exposes a
> `const ARITY: usize`: any single `TryGetable` scalar has arity 1, and tuple impls
> cover composite keys of 1 through 12 components.

> [spec:pgorm:def:entity.traits.model+3]
> `ModelTrait: Clone + Send + Debug` (`src/entity/model.rs`) is the read-side row
> representation. `get(column)` returns the column's `Value`;
> `set(column, value) -> Result<(), Error>` writes it, reporting a column this
> model does not carry, or a value whose type does not match the field, as
> `Error::Type`. `find_related(R)` returns a `Select<R>` scoped to this instance via
> `Related::find_related().belongs_to(self)`; `find_linked(L)` scopes a multi-hop
> `Linked` join to this instance using the last hop's `r{n}` table alias.
> `into_active(self)` converts the model into
> `<Self::Entity as EntityTrait>::ActiveModel` by delegating to `IntoActiveModel`;
> because the destination is fixed by the entity rather than by a type parameter, the
> call needs no annotation where `into_active_model()` does.
> `delete(self, db)` converts the model through `IntoActiveModel` and delegates to
> `ActiveModelTrait::delete`, so behavior hooks run. `TryIntoModel<M>` is the fallible
> reverse conversion with a blanket identity impl for any model.

> [spec:pgorm:def:entity.traits.from-query-result+4]
> `FromQueryResult` (`src/entity/model.rs`) instantiates a type from a `QueryResult`
> row given a column-name prefix: `from_query_result(res, pre)`.
> `from_query_result_optional` reads a row that may not carry the type at all —
> the related side of an outer join — answering `Ok(None)` for an absent row and
> propagating every other decode failure, against the witness `exec.decode.absent`
> defines. `find_by_statement(stmt, values)` builds a
> `SelectorRaw<SelectModel<Self>>` for running raw SQL into typed rows.
> `expected_columns` reports the columns `from_query_result` reads, so a statement can
> be checked against the type before a row exists (`exec.verify`) and so the optional
> decode knows which columns witness an absent row; it defaults to `None`, the answer
> of an implementation that does not report them.
> `PartialModelTrait: FromQueryResult` (`src/entity/partial_model.rs`) adds
> `select_cols<S: QuerySelect>(S) -> S::Projected`, letting a partial model declare
> exactly the columns it needs on a select. The return type is the *projected* state,
> not `S`, so an implementation that selects no column cannot typecheck: a field-less
> `DerivePartialModel` is a compile error rather than a query with an empty
> projection (`query.build.modifiers`).

> [spec:pgorm:def:entity.traits.active-enum+1]
> `ActiveEnum: Sized + Iterable` (`src/entity/active_enum.rs`) maps a Rust enum onto a
> database value. `Value` is the backing Rust type and must implement `ActiveEnumValue`
> (`Into<Value> + ValueType + Nullable + TryGetable`). `name()` returns the database
> enum's identifier as a `DynIden`; `to_value` / `into_value` convert a variant to its
> database value; `try_from_value` performs the fallible reverse mapping, returning
> `Error` for unknown values; `db_type()` returns the column definition used for the
> enum column. `as_enum()` wraps a value expression in a cast to the enum's type name,
> and `values()` enumerates every variant's database value in iterator order. The
> `ValueVec` associated type has no purpose and is documented for removal.

> [spec:pgorm:req:entity.traits.active-enum.limits+2]
> `ActiveEnumValue` is implemented for `String`, `i8`, `i16`, `i32`, `i64` and `u32`
> only (`src/entity/active_enum.rs`). `try_get_vec_by` — reading an array of enum
> values, a Postgres-only capability — MUST NOT panic when it cannot decode: it
> MUST return `TryGetError::Db(Error::Type(_))` for `u32` (not supported by
> `postgres-array`), and likewise for the other types when the `postgres-array`
> feature is disabled. The blanket `TryFromU64` impl for every `ActiveEnum` MUST
> return `Error::ConvertFromU64`, so a primary key containing an active-enum field
> MUST declare `auto_increment = false` to be usable.

## Active model

> [spec:pgorm:req:entity.active-model+2]
> `ActiveModelTrait: Clone + Debug` (`src/entity/active_model.rs`) is the write-side
> row representation whose fields are `ActiveValue`s. Implementations MUST provide
> per-column state access: `get` (immutable), `take` (removes and returns, leaving
> `NotSet`), `set` (stores a `Value` as `Set`, returning `Result<(), Error>` so an
> unmatched column or a value of the wrong type for the field comes back as
> `Error::Type` instead of unwinding), `not_set` (clears to `NotSet`),
> `is_not_set`, `reset` (per-column `Unchanged` → `Set`), and `default()` (all columns
> `NotSet`). `reset_all` applies `reset` to every column. `get_primary_key_value`
> returns the key as a `ValueTuple` (`One`/`Two`/`Three`/`Many` chosen by
> `PrimaryKeyArity::ARITY`) and MUST return `None` if any key component is `NotSet`.
> `is_changed` returns `true` when any attribute is in the `Set` state.

> [spec:pgorm:def:entity.active-model.active-value+1]
> `ActiveValue<V: Into<Value>>` (`src/entity/active_model.rs`) is a three-state
> machine over a column value: `Set(V)` (a value actively being written),
> `Unchanged(V)` (a value loaded from the database and not modified), and `NotSet`
> (no value). `Default::default()` is `NotSet`, and the `NotSet` variant is re-exported
> at crate root so `ActiveModel { field: NotSet, .. }` reads naturally. Only `Set`
> values participate in generated `INSERT`/`UPDATE` column lists; `Unchanged` primary
> keys still drive `WHERE` clauses. `PartialEq` compares equal only for identical
> variants with equal payloads.
>
> The `Set` state has three spellings, and documentation and examples MUST use the
> free `set(value)` function (`[spec:pgorm:req:entity.active-model.from-sugar+1]`) for
> construction: it is the only one that converts into the column type. The `Set(v)`
> variant and the `ActiveValue::set(v)` associated constructor both pin `v` to `V`
> exactly, and remain the spelling where the variant itself is the subject — pattern
> matching, where no function call can appear, and the cases where `V` is not
> inferable from context.

> [spec:pgorm:sem:entity.active-model.active-value.ops]
> `ActiveValue` accessors (`src/entity/active_model.rs`): the constructors `set`,
> `unchanged`, `not_set` and the predicates `is_set`, `is_unchanged`, `is_not_set`
> mirror the three variants. `take(&mut self)` returns `Some(value)` for `Set` or
> `Unchanged` and leaves `NotSet` behind. `unwrap(self)` and `as_ref(&self)` return
> the inner value and panic on `NotSet`; `try_as_ref` is the non-panicking form
> returning `Option<&V>`. `into_value` yields `Option<Value>`; `into_wrapped_value`
> converts to `ActiveValue<Value>` preserving the variant. `reset` promotes
> `Unchanged` to `Set` and leaves `NotSet` untouched. `set_if_not_equals(value)`
> assigns `Set(value)` unless the current state is `Unchanged` with an equal payload,
> in which case it does nothing — making `is_changed` reflect actual differences.

> [spec:pgorm:req:entity.active-model.from-sugar+1]
> pgorm provides a blanket `impl<V: Into<Value>> From<V> for ActiveValue<V>`
> (`src/entity/active_model.rs`) so any column value converts into `ActiveValue` with
> plain `.into()`, without writing `ActiveValue::Set(...)` — an ergonomic divergence
> from upstream SeaORM. The conversion MUST produce the `Set` state. Additionally,
> `From<ActiveValue<V>> for ActiveValue<Option<V>>` MUST lift a value into a nullable
> column position while preserving the variant (`Set(v)` → `Set(Some(v))`,
> `Unchanged(v)` → `Unchanged(Some(v))`, `NotSet` → `NotSet`).
>
> The blanket pins the source type to the field type exactly, so a borrowed value has
> no route into an owning column: `&str` reaches only `ActiveValue<&str>`, never
> `ActiveValue<String>`. Three targeted conversions close that gap —
> `From<&str> for ActiveValue<String>`, `From<&str> for ActiveValue<Option<String>>`,
> and `From<&[u8]> for ActiveValue<Vec<u8>>`. Each names a target the blanket does not
> produce for the same source, so they cohere with it, and each MUST produce the `Set`
> state.
>
> The free function `pub fn set<V: Into<Value>, T: Into<V>>(value: T) -> ActiveValue<V>`
> is the `Set`-shaped counterpart of those conversions: it applies `Into<V>` at the
> call site, so `set("Apple")` reaches an `ActiveValue<String>` and no ActiveModel
> field needs a `.to_owned()`. Neither the `Set` variant nor the `ActiveValue::set`
> associated constructor can offer this — a variant admits no conversion, and the
> associated form is bound by the `V` of the impl block. `set` is exported at the crate
> root and is the spelling documentation uses
> (`[spec:pgorm:def:entity.active-model.active-value+1]`).

> [spec:pgorm:req:entity.active-model.persistence+2]
> `ActiveModelTrait::insert` MUST execute via `Insert::exec_returning_model`, so on
> PostgreSQL the insert and the returned `Model` are a single `INSERT ... RETURNING`
> round trip. `ActiveModelTrait::update` executes
> `Update::one(am).exec_returning_model(db)`
> (an `UPDATE ... RETURNING` statement keyed on the primary key) and likewise returns
> the fresh `Model`. `ActiveModelTrait::delete` deletes by the model's primary key and
> returns the rows-affected count as `u64`. All three are async and generic over any
> `ConnectionTrait` (`src/entity/active_model.rs`). `insert` and `update` are the
> trait's only write entry points, and which of them runs is the caller's stated
> choice rather than a property of the model
> (`[spec:pgorm:req:entity.active-model.save+1]`). Both accept a primary key in
> either the `Set` or the `Unchanged` state.

> [spec:pgorm:req:entity.active-model.save+1]
> `ActiveModelTrait` MUST NOT carry a `save` operation that infers insert-versus-update
> from the primary-key state. The inherited inference — `insert` when at least one key
> column is `NotSet`, `update` otherwise — reads a distinction the input does not carry:
> `Set` and `Unchanged` are both "holds a value", so an entity with a manually assigned
> key, whose caller must populate that key to name the row at all, could never reach
> `insert` through it, and creating such a row was possible only by bypassing the
> ActiveModel API. Intent that is unrepresentable in the input is stated by the caller
> instead: `insert` and `update` (`[spec:pgorm:req:entity.active-model.persistence+1]`)
> are separate entry points, and insert-or-update in a single statement is expressed
> explicitly through `Insert::on_conflict`. The removal is deliberate and `save` MUST
> NOT be reintroduced under this or another name (`src/entity/active_model.rs`).

> [spec:pgorm:req:entity.active-model.hooks+1]
> `ActiveModelBehavior: ActiveModelTrait` (`src/entity/active_model.rs`) defines
> lifecycle hooks with pass-through defaults returning `Ok`. Ordering is fixed:
> `insert` MUST call `before_save(self, db, insert: true)` before executing and
> `after_save(model, db, true)` on the returned model; `update` does the same with
> `insert: false`; `delete` MUST call `before_delete(self, db)` before executing and
> `after_delete` (on a clone of the pre-delete active model) after. `new()` defaults to
> `ActiveModelTrait::default()` and is the hook for constructing an active model with
> default values.
>
> An `Err` from a *before* hook aborts the operation: it returns before the statement
> is built, so nothing reaches the database. An `Err` from an *after* hook is returned
> to the caller but MUST NOT be read as an abort — the statement has already executed
> on the connection, and the row stays written or stays deleted. The three entry points
> run their hooks and their statement on the `&C` they were handed and MUST NOT open a
> transaction of their own: a write the caller asked for on a plain connection is not
> silently widened into one, and the caller who wants the hook and the write to stand
> or fall together passes a `DatabaseTransaction` as that `&C` and lets the `Err`
> reach the rollback. The Rust docs on `ActiveModelBehavior`, `insert`, `update` and
> `delete` MUST state this asymmetry rather than promise an abort the code does not
> perform.

> [spec:pgorm:req:entity.active-model.into+1]
> `IntoActiveModel<A>` converts a type into an active model and has a blanket identity
> impl for any `ActiveModelTrait`; derived models convert `Model` → `ActiveModel` with
> every field `Unchanged` (used by `Model::delete`, `set_from_json`, and by callers
> turning a returned `Model` back into an active model to update it).
> `IntoActiveValue<V>` governs how `DeriveIntoActiveModel` fields become states:
> `Option<V>` MUST map `Some(v)` → `Set(Some(v))` and `None` → `NotSet`;
> `Option<Option<V>>` MUST map `Some(inner)` → `Set(inner)` (allowing an explicit
> `Set(None)` to null a column) and `None` → `NotSet`; the plain scalar impls
> (`bool`, integer and float primitives, `&'static str`, `String`, `Vec<u8>`, and the
> feature-gated `Json`/date-time/`Decimal`/`Uuid` types) MUST produce `Set`
> (`src/entity/active_model.rs`).

> [spec:pgorm:req:entity.active-model.json+3]
> Under the `with-json` feature, `ActiveModelTrait::from_json` builds an active model
> by deserializing the JSON object into the entity's `Model` (errors surface as
> `Error`), converting it with `IntoActiveModel`, then normalizing states per column:
> attributes whose key exists in the JSON object become `Set`, and all others MUST be
> `NotSet` (`src/entity/active_model.rs`).
>
> Presence detection and deserialization MUST read the same key namespace, and that
> namespace is `serde`'s: the key a column occupies is the model field's name as
> `serde` spells it, read from `ColumnTrait::json_key`
> (`[spec:pgorm:def:entity.traits.column+4]`) — never the SQL column name, which the
> deserializer never sees. `DeriveEntityModel` computes each column's key from the
> field it derived that column from, applying `#[serde(rename = "..")]` and
> `#[serde(rename_all = "..")]` (the deserialize half where the split form is used);
> `#[pgorm(column_name)]` and `#[pgorm(rename_all)]` name the SQL column and MUST NOT
> take part. A model `serde` serializes therefore round-trips back through `from_json`
> however either side is renamed, where reading SQL names would have silently dropped
> every renamed column to `NotSet`. A hand-written `Column` has no field to read and
> keeps the SQL name as its key — the name the two namespaces agree on when nothing is
> renamed.
>
> `set_from_json` applies the same conversion in place, and MUST leave the active
> model untouched when it fails: the JSON is converted into a whole new active model
> first, and a conversion that returned `Err` MUST NOT have written anything to
> `self` — in particular MUST NOT have cleared the primary key, which would leave a
> caller holding a model that can no longer name its row. On success it MUST NOT alter
> the primary-key values either: they are read off `self` and put onto the replacement
> via `set()`, so a `Set` or `Unchanged` key keeps its value but comes back in the
> `Set` state (an `Unchanged` key is upgraded), while `NotSet` keys stay `NotSet`.

## Relations

> [spec:pgorm:req:entity.relation+1]
> Relations are declared per entity through `RelationTrait: Iterable + Debug`, whose
> `def(&self) -> RelationDef` maps each variant of the entity's `Relation` enum to a
> definition (`src/entity/relation.rs`). `RelationType` has exactly two variants:
> `HasOne` and `HasMany` — belongs-to is expressed as ownership direction, not a third
> type. `EntityTrait::belongs_to(related)` MUST start a builder with `HasOne` and
> `is_owner = false`; `EntityTrait::has_one(R)` and `has_many(R)` require
> `R: Related<Self>` and derive their builder from the reversed related entity's
> definition (`R::to().rev()`) with `is_owner = true` — but the builder keeps only
> the tables and columns of that reversed definition: its `on_delete`/`on_update`
> actions, condition hooks and `fk_name` are discarded (`RelationBuilder::from_rel`),
> so FK actions declared on the belongs-to side do not surface on the reversed
> relation. The `Related<R>` trait exposes
> `to()`, `via()` (defaulting to `None`; `Some` denotes a junction-table hop), and
> `find_related()`, which MUST inner-join `to()` (and `via()` when present, joined in
> reverse) onto a fresh `Select<R>`.

> [spec:pgorm:def:entity.relation.def+4]
> `RelationDef` (`src/entity/relation.rs`) is the concrete relation record:
> `rel_type`, `from_tbl` / `to_tbl` (`FromItem`, since a relation is joined into a
> query and may be re-aliased), `columns` (`ColumnPairs`),
> `is_owner`, optional `on_delete` /
> `on_update` foreign-key actions (`pgorm_query::ForeignKeyAction`), an optional
> boxed `on_condition` closure receiving the left and right join idens, an optional
> `fk_name`, and a `condition_type` (`All` = AND, `Any` = OR). `rev()` swaps the
> from/to tables and columns, negates `is_owner`, clears `fk_name`, and keeps the
> remaining attributes. `from_alias(alias)` re-points `from_tbl` at a table alias for
> self-join disambiguation; `on_condition(f)` replaces any existing custom condition;
> `condition_type(t)` sets how the ON clauses combine.
>
> `ColumnPairs` (`src/entity/identity.rs`) is the column set a relation joins on,
> held as a first `(from, to)` pair plus any further pairs. The two sides of a
> relation are therefore one value, not two: the only constructor,
> `ColumnPairs::new(from, to)`, takes a pair, `and(from, to)` / `push(from, to)`
> extend by a pair, and `rev()` swaps within each pair. A set of join columns is
> consequently non-empty and equal-sided by construction — the arities cannot
> disagree, so no consumer has to reconcile them and none can silently drop a
> column. `arity()` reports the number of pairs; `from_identity()` /
> `to_identity()` project one side as an `Identity` for consumers that key on a
> single side.
>
> `Identity` (`src/entity/identity.rs`) encodes column-set arity as
> `Unary` / `Binary` / `Ternary` / `Many(Vec<DynIden>)`. `IntoIdentity` converts
> `&str` and `String` (via `Alias`), any `IdenStr`, and tuples of up to 12
> identifiers; `IdentityOf<E>`, a subtrait of `IntoIdentity`, restricts
> conversions to columns of entity `E`.
>
> Each `IntoIdentity` impl also names a `ValueType`: the tuple of `Value` of the
> same length as the columns it produces, or `ValueTuple` for `Identity` itself,
> whose arity is only known at runtime. `IntoBoundary<K>` is the matching
> relation on the value side, implemented exactly for the tuples whose length `K`
> describes — plus, for `K = ValueTuple`, every `IntoValueTuple`. A consumer that
> pairs a column set with values (`[spec:pgorm:sem:exec.cursor.keyset+3]`)
> therefore gets the arity agreement from the type system rather than by
> checking it, and the `Identity` case is the only one left to check.

> [spec:pgorm:req:entity.relation.builder+1]
> `RelationBuilder<E, R, C>` (`src/entity/relation.rs`) accumulates a
> `RelationDef`, with `C` tracking whether the join columns have been supplied.
> The `belongs_to` path starts at `C = NoColumns` and MUST be given a column pair
> through `.columns(from, to)`, which takes one `E::Column` and one `R::Column`
> and moves the builder to `C = ColumnPairs`; a composite key adds each further
> pair with `.and_columns(from, to)`. Columns are therefore always supplied in
> pairs, and the two sides of a relation can never be given different numbers of
> columns. The `has_one` / `has_many` path starts at `C = ColumnPairs`, pre-filled
> from the reversed related definition, where `.columns(from, to)` instead
> replaces the pre-filled set with the given pair. Optional
> attributes are set by `on_delete(action)`, `on_update(action)`, `on_condition(f)`,
> `fk_name(name)`, and `condition_type(t)` in either state; `condition_type`
> defaults to `ConditionType::All`. The finished definition is obtained via
> `From<RelationBuilder<E, R, ColumnPairs>> for RelationDef`; there is no such
> conversion from `NoColumns`, so a relation missing its columns is a compile
> error rather than a panic.

> [spec:pgorm:req:entity.relation.linked+2]
> `Linked` (`src/entity/link.rs`) expresses a multi-hop join: `link()` returns the
> ordered `Vec<RelationDef>` chain from `FromEntity` to `ToEntity`. `find_linked()`
> MUST build the join by iterating the chain in reverse, aliasing each hop's source
> table as `r0`, `r1`, ... and inner-joining it to the previous alias (the innermost
> hop joins the unaliased target table), with each hop's `join_tbl_on_condition`
> augmented by that relation's `on_condition` closure when present.
> `ModelTrait::find_linked` scopes the result to a model instance by filtering on the
> final alias `r{len - 1}` (`src/entity/model.rs`).
>
> Those aliases are a type, `LinkedAlias`, whose `hop(i)` renders `r{i}` — not a
> `format!` repeated at each site that needs one. The last rung is derived by the
> provided method `Linked::last_hop_alias`, which every builder walking a chain
> MUST use and which is public precisely so that callers do too: the name is
> otherwise an internal that a caller can only reproduce by hardcoding `r4`, and
> a chain that later gains a hop rebinds that string to a different table without
> any diagnostic. The method is named for the mechanism rather than for one of
> its readings because the two ladders run in opposite directions: walking
> forwards from `FromEntity` (`find_also_linked` / `find_with_linked`) the last
> rung is the joined target, and walking backwards from `ToEntity`
> (`find_linked`) it is the source table — which is exactly why it is what
> scopes `ModelTrait::find_linked`. A chain whose `link()` is empty yields
> `hop(0)`; the derivation saturates rather than underflowing, and the resulting
> query names a table the (join-less) statement does not have, which the server
> reports.
>
> A chain a `Related` implementation already describes MUST NOT have to be
> restated as a hand-written `Linked`: `RelatedLink<E, R>` is that chain, its
> `link()` returning `[E::to()]` for a direct relation and `[via, E::to()]` for
> a junction-mediated one — the order `Linked` expects. It is a zero-sized
> `Copy` witness carrying no data, written `RelatedLink::to(target_entity)`,
> which names only the target: the source entity is inferred from the position
> the witness is used in. Hand-written `Linked` impls remain the way to express
> what a `Related` impl does not — chains of more than one relation, and hops
> carrying a bespoke `on_condition`.
>
> The linked form differs from the `Related` one (`find_also_related`) in
> aliasing the joined table, so `RelatedLink::to(Entity)` on an entity related
> to itself is well-formed where the related form would name one table twice.

> [spec:pgorm:req:entity.relation.fk+3]
> A `RelationDef` converts into DDL foreign-key forms via
> `From<RelationDef> for ForeignKeyCreateStatement` and `for TableForeignKey`
> (`src/entity/relation.rs`). The conversion maps every pair in `columns` to a
> constrained column and its referenced column,
> applies `on_delete` and `on_update` actions when present, and names the
> constraint from `fk_name` when set; otherwise the name MUST be derived as
> `fk-{from_table}-{from_cols joined with '-'}`. Both conversions unpack the table
> references to bare tables (the schema of a `FromItem`'s `TableName`, and any bound
> alias, are reduced away by `unpack_table_ref`).
>
> Both conversions are total and MUST stay so: a `ColumnPairs` is non-empty and
> balanced by construction and a foreign key is built from exactly that, so the
> first pair goes to the constructor and the rest are appended
> (`[spec:pgorm:req:sql.ddl.foreign-key+3]`). There is no unpaired or empty
> column set on either side of the conversion for it to fail on.

## Prelude

> [spec:pgorm:def:entity.prelude+3]
> `pgorm::entity::prelude` (`src/entity/prelude.rs`) is the glob a file that
> talks to the database imports instead of naming what it needs one item at a
> time. Membership is chosen from what code actually writes, and is public API:
> removing a name from it breaks callers that never mentioned it.
>
> It carries the entity trait family and the derives that produce it
> (`entity.traits`); the query-builder traits whose methods are otherwise
> unreachable — `QueryFilter`, `QuerySelect`, `QueryOrder`, `QueryTrait`,
> `PaginatorTrait`, `CursorTrait`, `LoaderTrait`; the active-model vocabulary
> `ActiveValue` with its `Set` / `Unchanged` / `NotSet` variants,
> `IntoActiveModel`, `IntoActiveValue`, `TryIntoModel`, and the free `set`
> constructor (`entity.active-model.from-sugar`); the CRUD entry points
> `Select`, `Insert`, `Update`, `Delete`; the decode surface `FromQueryResult`,
> `QueryResult`, `DecodeSelect`, `DecodeRaw` (`exec.crud.selector-entry`); the
> connection types `DatabasePool`, `DatabaseConnection`, `DatabaseTransaction`,
> `ConnectionTrait`, `TransactionTrait`; `Iterable`, `Condition`, `JoinType`,
> `Value`, the `error` module's contents, and the handful of `pgorm_query`
> names an entity definition needs (`Expr`, `DynIden`, `SharedIden`, `StringLen`,
> `ForeignKeyAction`, `Arc`).
>
> `Order` — `pgorm_query`'s `ASC`/`DESC` enum — is deliberately NOT a member.
> `order` is an ordinary table name, so an entity aliased `Order` is ordinary
> too, and a module globbing both it and this prelude makes every mention of
> `Order` an E0659 ambiguity; `order_by_asc` / `order_by_desc` cover the common
> case and the enum is one import away. The same hazard is live for members
> that stay — `ActiveEnum` collides with an entity of that name — so a file
> hitting it disambiguates with an explicit import, which is why the hazard is
> stated here rather than resolved by shrinking the prelude to names no schema
> could reuse.
>
> It also carries the relation vocabulary an entity definition and its call
> sites write: `Related`, `Linked`, `RelationDef`, `RelationTrait`, and the
> `RelatedLink` witness that spares a `Related` chain from being restated as a
> hand-written `Linked` (`[spec:pgorm:req:entity.relation.linked+2]`).
>
> The alias vocabulary `alias` and `AliasName` are members
> (`[spec:pgorm:sem:query.build.alias]`) on the same grounds as `Expr`: a name
> the query introduces is written where the query is written, and a token is
> only cheaper than `Alias::new` when it is already in scope. `LinkedAlias` is
> NOT a member — it is reached as the return of a `Linked` method, so a caller
> never has to name the type.
>
> `IdenStr` is a member, and it is the reason the base identifier contract is
> NOT named `IdenStatic` (`[spec:pgorm:def:entity.traits+1]`):
> `pgorm_query::IdenStatic` is a different trait with the same method and an
> incompatible signature, and a prelude that globbed one of them into every
> file made the collision reachable from anywhere. Under the distinct name the
> two coexist, and the hazard is one the rename retires rather than one a
> reader has to remember.
