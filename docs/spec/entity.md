# Entity system

The entity layer (`src/entity/`) defines the trait family that maps Rust types onto
PostgreSQL tables: entities, models, active models, columns, primary keys, relations,
links, and active enums. This spec captures what the code does today, including its
explicit limitations.

## Entity traits

> [spec:pgorm:def:entity.traits]
> The entity trait family is defined in `src/entity/` and re-exported from
> `pgorm::entity` (`src/entity/mod.rs`). `IdenStatic` (`base_entity.rs`) is the base
> identifier contract: `Iden + Copy + Debug + 'static` plus `as_str(&self) -> &str`.
> `EntityName: IdenStatic + Default` maps an entity to a table. `EntityTrait: EntityName`
> is the abstract entity, carrying five associated types: `Model`
> (`ModelTrait<Entity = Self> + FromQueryResult`), `ActiveModel`
> (`ActiveModelBehavior<Entity = Self>`), `Column` (`ColumnTrait`), `Relation`
> (`RelationTrait`), and `PrimaryKey`
> (`PrimaryKeyTrait + PrimaryKeyToColumn<Column = Self::Column>`).
>
> The trait family is normally produced by the derive macros exported from
> `src/entity/prelude.rs` (`DeriveEntityModel`, `DeriveActiveModel`, `DerivePrimaryKey`,
> `DeriveRelation`, and friends), but every trait can be implemented by hand.

> [spec:pgorm:req:entity.traits.entity-name]
> `EntityName::table_name` is the only required method and MUST return the table's SQL
> name. `schema_name` and `comment` default to `None`; `module_name` defaults to
> `table_name()`. `table_ref` MUST produce a schema-qualified `TableRef`
> (`(Alias::new(schema), entity)`) when `schema_name` returns `Some`, and a bare table
> reference otherwise (`src/entity/base_entity.rs`). All generated SQL that names the
> table goes through `table_ref`, so a `Some` schema name qualifies every statement.

> [spec:pgorm:req:entity.traits.crud+1]
> `EntityTrait` provides the static CRUD surface (`src/entity/base_entity.rs`):
> `find()` returns a fresh `Select<Self>`; `find_by_id(values)` builds on `find()` by
> adding an equality filter per primary-key column, consuming the value tuple in
> primary-key iteration order. `insert(model)` returns `Insert::one`,
> `insert_many(models)` returns `Insert::many`, `update(model)` returns
> `Result<UpdateOne<A>, DbErr>` and `delete(model)` `Result<DeleteOne<A>, DbErr>` —
> both forward the builder guards of `query.build.update` / `query.build.delete`,
> erring on an unset primary key. `update_many()` returns an `UpdateMany`,
> `delete_many()` a `DeleteMany`, and `delete_by_id(values)` a `DeleteMany` filtered
> per primary-key column like `find_by_id`.
>
> `find_by_id` and `delete_by_id` MUST panic with `primary key arity mismatch` when the
> number of supplied values differs from the primary key's arity, in either direction.
> Values are accepted via `Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>`, so
> composite keys are passed as tuples.

> [spec:pgorm:def:entity.traits.column]
> `ColumnTrait: IdenStatic + Iterable + FromStr` (`src/entity/column.rs`) describes one
> column of an entity. `def()` returns the column's `ColumnDef`; `entity_name()` and
> `as_column_ref()` qualify the column with its `EntityName`. The trait exposes an
> expression-building surface wrapping `pgorm_query::Expr`: comparison operators `eq`,
> `ne`, `gt`, `gte`, `lt`, `lte`; range `between` / `not_between`; pattern matching
> `like`, `not_like`, and the sugar `starts_with` (`s%`), `ends_with` (`%s`),
> `contains` (`%s%`); aggregates `max`, `min`, `sum`, `count`; null checks `is_null`,
> `is_not_null`, `if_null`; set membership `is_in` / `is_not_in` and subqueries
> `in_subquery` / `not_in_subquery`; plus `into_expr` and `into_returning_expr`.
> The comparison and set-membership operators pass their values through `save_as`, so
> enum-typed columns compare against properly cast values.
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

> [spec:pgorm:sem:entity.traits.column.enum-cast]
> Enum-typed columns are transparently cast at the SQL boundary
> (`src/entity/column.rs`). On read, `select_as` / `select_enum_as` casts an enum
> column to `text` — or `text[]` when the column type is `Array` of an enum — and
> leaves non-enum columns untouched. On write, `save_as` / `save_enum_as` casts the
> value to the enum's database type name — or `{enum_name}[]` for arrays. As a special
> case under the `with-json` + `postgres-array` features, saving into a `Json` /
> `JsonBinary` column flattens a `Value::Array` of JSON values into a single
> `Value::Json` array value instead of applying an enum cast.

> [spec:pgorm:def:entity.traits.primary-key+1]
> `PrimaryKeyTrait: IdenStatic + Iterable` (`src/entity/primary_key.rs`) defines an
> entity's primary key as an iterable enum of key columns. Its `ValueType` associated
> type is the Rust value form of the whole key and is bound by
> `Sized + Send + Debug + PartialEq + IntoValueTuple + TryFromValueTuple
> + TryGetableMany + TryFromU64 + PrimaryKeyArity`; `auto_increment()` reports whether the key is
> database-generated. `PrimaryKeyToColumn` maps key variants to columns (`into_column`)
> and back (`from_column -> Option<Self>`). `PrimaryKeyArity` exposes a
> `const ARITY: usize`: any single `TryGetable` scalar has arity 1, and tuple impls
> cover composite keys of 1 through 12 components.

> [spec:pgorm:def:entity.traits.model+1]
> `ModelTrait: Clone + Send + Debug` (`src/entity/model.rs`) is the read-side row
> representation. `get(column)` returns the column's `Value`;
> `set(column, value) -> Result<(), DbErr>` writes it, reporting a column this
> model does not carry, or a value whose type does not match the field, as
> `DbErr::Type`. `find_related(R)` returns a `Select<R>` scoped to this instance via
> `Related::find_related().belongs_to(self)`; `find_linked(L)` scopes a multi-hop
> `Linked` join to this instance using the last hop's `r{n}` table alias.
> `delete(self, db)` converts the model through `IntoActiveModel` and delegates to
> `ActiveModelTrait::delete`, so behavior hooks run. `TryIntoModel<M>` is the fallible
> reverse conversion with a blanket identity impl for any model.

> [spec:pgorm:def:entity.traits.from-query-result]
> `FromQueryResult` (`src/entity/model.rs`) instantiates a type from a `QueryResult`
> row given a column-name prefix: `from_query_result(res, pre)`.
> `from_query_result_optional` converts any decode error into `Ok(None)` — the error
> value itself is discarded. `find_by_statement(stmt, values)` builds a
> `SelectorRaw<SelectModel<Self>>` for running raw SQL into typed rows.
> `PartialModelTrait: FromQueryResult` (`src/entity/partial_model.rs`) adds
> `select_cols<S: SelectColumns>(S) -> S`, letting a partial model declare exactly the
> columns it needs on a select.

> [spec:pgorm:def:entity.traits.active-enum]
> `ActiveEnum: Sized + Iterable` (`src/entity/active_enum.rs`) maps a Rust enum onto a
> database value. `Value` is the backing Rust type and must implement `ActiveEnumValue`
> (`Into<Value> + ValueType + Nullable + TryGetable`). `name()` returns the database
> enum's identifier as a `DynIden`; `to_value` / `into_value` convert a variant to its
> database value; `try_from_value` performs the fallible reverse mapping, returning
> `DbErr` for unknown values; `db_type()` returns the column definition used for the
> enum column. `as_enum()` wraps a value expression in a cast to the enum's type name,
> and `values()` enumerates every variant's database value in iterator order. The
> `ValueVec` associated type has no purpose and is documented for removal.

> [spec:pgorm:req:entity.traits.active-enum.limits+1]
> `ActiveEnumValue` is implemented for `String`, `i8`, `i16`, `i32`, `i64` and `u32`
> only (`src/entity/active_enum.rs`). `try_get_vec_by` — reading an array of enum
> values, a Postgres-only capability — MUST NOT panic when it cannot decode: it
> MUST return `TryGetError::DbErr(DbErr::Type(_))` for `u32` (not supported by
> `postgres-array`), and likewise for the other types when the `postgres-array`
> feature is disabled. The blanket `TryFromU64` impl for every `ActiveEnum` MUST
> return `DbErr::ConvertFromU64`, so a primary key containing an active-enum field
> MUST declare `auto_increment = false` to be usable.

## Active model

> [spec:pgorm:req:entity.active-model+1]
> `ActiveModelTrait: Clone + Debug` (`src/entity/active_model.rs`) is the write-side
> row representation whose fields are `ActiveValue`s. Implementations MUST provide
> per-column state access: `get` (immutable), `take` (removes and returns, leaving
> `NotSet`), `set` (stores a `Value` as `Set`, returning `Result<(), DbErr>` so an
> unmatched column or a value of the wrong type for the field comes back as
> `DbErr::Type` instead of unwinding), `not_set` (clears to `NotSet`),
> `is_not_set`, `reset` (per-column `Unchanged` → `Set`), and `default()` (all columns
> `NotSet`). `reset_all` applies `reset` to every column. `get_primary_key_value`
> returns the key as a `ValueTuple` (`One`/`Two`/`Three`/`Many` chosen by
> `PrimaryKeyArity::ARITY`) and MUST return `None` if any key component is `NotSet`.
> `is_changed` returns `true` when any attribute is in the `Set` state.

> [spec:pgorm:def:entity.active-model.active-value]
> `ActiveValue<V: Into<Value>>` (`src/entity/active_model.rs`) is a three-state
> machine over a column value: `Set(V)` (a value actively being written),
> `Unchanged(V)` (a value loaded from the database and not modified), and `NotSet`
> (no value). `Default::default()` is `NotSet`, and the `NotSet` variant is re-exported
> at crate root so `ActiveModel { field: NotSet, .. }` reads naturally. Only `Set`
> values participate in generated `INSERT`/`UPDATE` column lists; `Unchanged` primary
> keys still drive `WHERE` clauses. `PartialEq` compares equal only for identical
> variants with equal payloads.

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

> [spec:pgorm:req:entity.active-model.from-sugar]
> pgorm provides a blanket `impl<V: Into<Value>> From<V> for ActiveValue<V>`
> (`src/entity/active_model.rs`) so any column value converts into `ActiveValue` with
> plain `.into()`, without writing `ActiveValue::Set(...)` — an ergonomic divergence
> from upstream SeaORM. The conversion MUST produce the `Set` state. Additionally,
> `From<ActiveValue<V>> for ActiveValue<Option<V>>` MUST lift a value into a nullable
> column position while preserving the variant (`Set(v)` → `Set(Some(v))`,
> `Unchanged(v)` → `Unchanged(Some(v))`, `NotSet` → `NotSet`).

> [spec:pgorm:req:entity.active-model.persistence]
> `ActiveModelTrait::insert` MUST execute via `Insert::exec_with_returning`, so on
> PostgreSQL the insert and the returned `Model` are a single `INSERT ... RETURNING`
> round trip. `ActiveModelTrait::update` executes `Entity::update(am).exec(db)`
> (an `UPDATE ... RETURNING` statement keyed on the primary key) and likewise returns
> the fresh `Model`. `ActiveModelTrait::delete` deletes by the model's primary key and
> returns the `DeleteResult`. All three are async and generic over any
> `ConnectionTrait` (`src/entity/active_model.rs`).

> [spec:pgorm:req:entity.active-model.save]
> `ActiveModelTrait::save` (`src/entity/active_model.rs`) is the insert-or-update
> decision rule: it MUST iterate every primary-key column and choose `insert` when at
> least one key column is `NotSet`, and `update` when all key columns hold values
> (`Set` or `Unchanged`). The resulting `Model` is converted back through
> `IntoActiveModel` and returned as `Self`. Per its documentation this only works for
> entities with an auto-increment primary key — a fully populated manual key always
> routes to `update`.

> [spec:pgorm:req:entity.active-model.hooks]
> `ActiveModelBehavior: ActiveModelTrait` (`src/entity/active_model.rs`) defines
> lifecycle hooks with pass-through defaults returning `Ok`. Ordering is fixed:
> `insert` MUST call `before_save(self, db, insert: true)` before executing and
> `after_save(model, db, true)` on the returned model; `update` does the same with
> `insert: false`; `delete` MUST call `before_delete(self, db)` before executing and
> `after_delete` (on a clone of the pre-delete active model) after. An `Err` from any
> hook aborts the operation. `new()` defaults to `ActiveModelTrait::default()` and is
> the hook for constructing an active model with default values.

> [spec:pgorm:req:entity.active-model.into]
> `IntoActiveModel<A>` converts a type into an active model and has a blanket identity
> impl for any `ActiveModelTrait`; derived models convert `Model` → `ActiveModel` with
> every field `Unchanged` (used by `save`, `Model::delete`, and `set_from_json`).
> `IntoActiveValue<V>` governs how `DeriveIntoActiveModel` fields become states:
> `Option<V>` MUST map `Some(v)` → `Set(Some(v))` and `None` → `NotSet`;
> `Option<Option<V>>` MUST map `Some(inner)` → `Set(inner)` (allowing an explicit
> `Set(None)` to null a column) and `None` → `NotSet`; the plain scalar impls
> (`bool`, integer and float primitives, `&'static str`, `String`, `Vec<u8>`, and the
> feature-gated `Json`/date-time/`Decimal`/`Uuid` types) MUST produce `Set`
> (`src/entity/active_model.rs`).

> [spec:pgorm:req:entity.active-model.json+1]
> Under the `with-json` feature, `ActiveModelTrait::from_json` builds an active model
> by deserializing the JSON object into the entity's `Model` (errors surface as
> `DbErr`), converting it with `IntoActiveModel`, then normalizing states per column:
> attributes whose key exists in the JSON object become `Set`, and all others MUST be
> `NotSet`. `set_from_json` applies the same conversion in place but MUST NOT alter
> the primary-key values: key values are taken before the overwrite and put back
> afterwards via `set()`, so a `Set` or `Unchanged` key keeps its value but comes
> back in the `Set` state (an `Unchanged` key is upgraded), while `NotSet` keys are
> restored as `NotSet` (`src/entity/active_model.rs`).

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

> [spec:pgorm:def:entity.relation.def]
> `RelationDef` (`src/entity/relation.rs`) is the concrete relation record:
> `rel_type`, `from_tbl` / `to_tbl` (`TableRef`), `from_col` / `to_col`
> (`Identity`, permitting composite keys), `is_owner`, optional `on_delete` /
> `on_update` foreign-key actions (`pgorm_query::ForeignKeyAction`), an optional
> boxed `on_condition` closure receiving the left and right join idens, an optional
> `fk_name`, and a `condition_type` (`All` = AND, `Any` = OR). `rev()` swaps the
> from/to tables and columns, negates `is_owner`, clears `fk_name`, and keeps the
> remaining attributes. `from_alias(alias)` re-points `from_tbl` at a table alias for
> self-join disambiguation; `on_condition(f)` replaces any existing custom condition;
> `condition_type(t)` sets how the ON clauses combine.
>
> `Identity` (`src/entity/identity.rs`) encodes column-set arity as
> `Unary` / `Binary` / `Ternary` / `Many(Vec<DynIden>)`. `IntoIdentity` converts
> `&str` and `String` (via `Alias`), any `IdenStatic`, and tuples of up to 12
> identifiers; `IdentityOf<E>` restricts conversions to columns of entity `E`.

> [spec:pgorm:req:entity.relation.builder]
> `RelationBuilder<E, R>` (`src/entity/relation.rs`) accumulates a `RelationDef`.
> The `belongs_to` path starts with no columns, and callers MUST supply both
> `.from(col)` and `.to(col)` (any `IdentityOf` value, so tuples declare composite
> foreign keys); converting a builder without them panics with
> `Reference column is not set` / `Owner column is not set`. The `has_one` / `has_many`
> path pre-fills both columns from the reversed related definition. Optional
> attributes are set by `on_delete(action)`, `on_update(action)`, `on_condition(f)`,
> `fk_name(name)`, and `condition_type(t)`; `condition_type` defaults to
> `ConditionType::All`. The finished definition is obtained via
> `From<RelationBuilder> for RelationDef`.

> [spec:pgorm:req:entity.relation.linked]
> `Linked` (`src/entity/link.rs`) expresses a multi-hop join: `link()` returns the
> ordered `Vec<RelationDef>` chain from `FromEntity` to `ToEntity`. `find_linked()`
> MUST build the join by iterating the chain in reverse, aliasing each hop's source
> table as `r0`, `r1`, ... and inner-joining it to the previous alias (the innermost
> hop joins the unaliased target table), with each hop's `join_tbl_on_condition`
> augmented by that relation's `on_condition` closure when present.
> `ModelTrait::find_linked` scopes the result to a model instance by filtering on the
> final alias `r{len - 1}` (`src/entity/model.rs`).

> [spec:pgorm:req:entity.relation.fk]
> A `RelationDef` converts into DDL foreign-key forms via
> `From<RelationDef> for ForeignKeyCreateStatement` and `for TableForeignKey`
> (`src/entity/relation.rs`). The conversion maps every `from_col` / `to_col`
> component, applies `on_delete` and `on_update` actions when present, and names the
> constraint from `fk_name` when set; otherwise the name MUST be derived as
> `fk-{from_table}-{from_cols joined with '-'}`. Both conversions unpack the table
> references to bare tables (schema information from `TableRef` variants is reduced
> via `unpack_table_ref`).
