# Schema Generation from Entities

`src/schema/` projects compile-time entity definitions (`EntityTrait`) into
`pgorm_query` DDL statements. `Schema` is a stateless helper — `Schema::new()`
takes no backend argument because pgorm is PostgreSQL-only. The statements are
returned to the caller (typically migrations or test setup); nothing here
executes SQL.

## Table projection

> [spec:pgorm:sem:schema.from-entity+2]
> `Schema::create_table_from_entity::<E>()` produces one `TableCreateStatement`
> for `E`: the table ref from `entity.table_ref()`, the entity comment if any,
> and one column per `E::Column` variant projected from `ColumnTrait::def()` —
> the declared `ColumnType` (with `Enum { name, .. }` rewritten to a custom
> type reference naming the Postgres enum), `NOT NULL` unless the column is
> nullable, a unique key for `unique` columns, plus any default value and
> column comment.
>
> Primary-key handling depends on key arity: a column matching a primary-key
> column gains `auto_increment` when `E::PrimaryKey::auto_increment()` is
> true, and the inline `PRIMARY KEY` flag only when the key arity is 1;
> composite keys (arity > 1) instead emit a table-level primary-key index
> named `pk-{table}`. Foreign keys are generated from `E::Relation` entries
> whose `RelationDef` has `is_owner == false` (the belongs-to side); owner-side
> relations produce no constraint.
>
> Comments ride on the create statement (`get_comment()`,
> `ColumnSpec::Comment`) but are inert there — executing it attaches nothing
> (`[spec:pgorm:req:sql.ddl.create-table+6]`). They are a second statement
> stream instead: `Schema::create_comments_from_entity::<E>()` returns the
> `COMMENT ON` statements for the same entity — the entity comment first when
> `E::comment()` is set, then one per column whose `ColumnDef` carries a
> comment, in `E::Column` order — each targeting `entity.table_ref()`, so a
> comment lands on the same qualified name the table projection uses. The Vec
> is empty when no comment is declared. `table_ref()` is a `TableName`, which
> always names a table, so the comment target needs no conversion and the
> stream has no failure mode.

## Secondary indexes

> [spec:pgorm:sem:schema.from-entity.index+1]
> `Schema::create_index_from_entity::<E>()` returns one `IndexCreateStatement`
> per column whose `ColumnDef` has the `indexed` flag, named
> `idx-{table}-{column}` over that single column, and an empty `Vec` when no
> column is indexed. Each statement targets `entity.table_ref()`, the same ref
> the table projection uses, so the index is schema-qualified
> (`ON "{schema}"."{table}"`) exactly when the entity declares a
> `schema_name` and bare (`ON "{table}"`) when it does not. Unique columns are
> not covered here — uniqueness is emitted as a column-level unique key by the
> table projection, not as a separate index statement — and multi-column
> indexes cannot be expressed.

## Postgres enum types

> [spec:pgorm:sem:schema.from-entity.enum+2]
> `Schema::create_enum_from_entity::<E>()` scans `E::Column` and returns one
> `TypeCreateStatement` (`CREATE TYPE {name} AS ENUM ({variants})`) per column
> whose type is `ColumnType::Enum`, preserving declared variant order; a column
> of any other type contributes no statement, so this form cannot fail.
> `Schema::create_enum_from_active_enum::<A>()` builds the same statement from
> `A::db_type()` for a single `ActiveEnum`, and returns `Error::Type` naming the
> enum if the resolved column type is not `ColumnType::Enum` — an `ActiveEnum`
> backed by a plain column type has no database enum to create. Emitting
> duplicates is the caller's problem: two columns sharing one enum type yield
> two identical statements.
