# DDL Statement Builders

This section specifies the schema (DDL) statement builders in `pgorm-query`:
table create/alter/drop/rename/truncate (`pgorm-query/src/table/`), index and
foreign key statements (`pgorm-query/src/index/`,
`pgorm-query/src/foreign_key/`), `CREATE TYPE ... AS ENUM` and extension
statements (`pgorm-query/src/extension.rs`), and the rendering contract
implemented by the Postgres `QueryBuilder`
(`pgorm-query/src/backend/query_builder.rs`). All rules describe current
behaviour, including panics and leftovers from the multi-backend ancestry.

> [spec:pgorm:req:sql.ddl+1]
> The DDL surface MUST be reachable through the entry-point helpers: `Table`
> (`create`/`alter`/`drop`/`rename`/`truncate`), `Index` (`create`/`drop`),
> `ForeignKey` (`create`/`drop`), `Type` (`create`/`alter`/`drop`) and
> `Extension` (`create`/`drop`). Table, index and foreign-key statements
> implement `SchemaStatementBuilder` (`build`, `build_any`, `to_string`), all
> of which delegate to the corresponding `prepare_*` method on the single
> Postgres `QueryBuilder`; type and extension statements provide equivalent
> `build_ref`/`build_collect`/`to_string` inherent methods. Identifiers that
> render through `Iden::prepare` (table, column and type names) MUST render
> double-quoted (quote character `"`, embedded quotes doubled); index and
> constraint names are written raw between quote characters, without
> doubling. `TableStatement` is an enum wrapper carrying its own
> `build`/`build_any`/`to_string` dispatch methods; `IndexStatement`,
> `ForeignKeyStatement` and `SchemaStatement` are plain wrapper enums whose
> variants render through the same builders.

## Tables

> [spec:pgorm:req:sql.ddl.create-table]
> `TableCreateStatement` composes a table name (`table()`, any
> `IntoTableRef`), ordered `ColumnDef`s (`col()`, which stamps the table ref
> onto each column), table-level indexes (`index()` and `primary_key()` — the
> latter takes an `IndexCreateStatement` and forces its `primary` flag),
> foreign keys (`foreign_key()`), check expressions (`check()`), an
> `if_not_exists` flag, MySQL-era options (`engine`, `collate`,
> `character_set`), a `comment` and a trailing `extra` string.
>
> Rendering MUST emit `CREATE TABLE [IF NOT EXISTS ]<table> ( ... )` with the
> body in this fixed order: column definitions, then embedded index
> expressions, then foreign-key clauses (in `Mode::Creation`, i.e. without
> `ALTER TABLE`/`ADD`), then `CHECK (...)` constraints, all comma-separated.
> Embedded indexes render as `[CONSTRAINT "name" ]PRIMARY KEY |UNIQUE [NULLS
> NOT DISTINCT ](cols)`. After the closing parenthesis the MySQL-style
> options still render verbatim (`ENGINE=`, `COLLATE=`, `DEFAULT CHARSET=`) —
> a leftover that produces invalid Postgres if used — followed by the `extra`
> string (e.g. `USING columnar`). The table-level `comment` is stored and
> exposed via `get_comment()` but is never rendered.

> [spec:pgorm:req:sql.ddl.column-def]
> `ColumnDef` holds a name, an optional `ColumnType` and an ordered list of
> `ColumnSpec`s (`Null`, `NotNull`, `Default(SimpleExpr)`, `AutoIncrement`,
> `UniqueKey`, `PrimaryKey`, `Check(SimpleExpr)`, `Generated { expr, stored }`,
> `Extra(String)`, `Comment(String)`), populated by the fluent typed setters
> (`integer()`, `string_len(n)`, `timestamp_with_time_zone()`, `interval()`,
> `vector()`, `enumeration()`, `array(elem)`, `cidr()`, `ltree()`, ...,
> `not_null()`, `default(v)`, `check(expr)`, `extra(s)`, etc.).
>
> A column MUST render as the quoted name, one space, the type spelling, then
> each spec in insertion order: `NULL`, `NOT NULL`, `DEFAULT <expr>`,
> `UNIQUE`, `PRIMARY KEY`, `CHECK (<expr>)`, `GENERATED ALWAYS AS (<expr>)
> STORED`/`VIRTUAL` (`VIRTUAL` is emitted for non-stored generated columns
> even though Postgres does not accept it), and `Extra` verbatim.
> `AutoIncrement` produces no keyword; instead it replaces the type spelling
> with the serial family — `SmallInteger`→`smallserial`, `Integer`→`serial`,
> `BigInteger`→`bigserial`. `Comment` specs are skipped entirely
> (`column_comment` is a no-op on Postgres). `IntoColumnDef` accepts both
> `ColumnDef` and `&mut ColumnDef` (via `take()`), enabling the
> builder-by-reference doctest style.

> [spec:pgorm:req:sql.ddl.column-types+1]
> `prepare_column_type` defines the `ColumnType` → Postgres type-name
> contract. It MUST spell: `Char(Some(n))`→`char(n)`, `Char(None)`→`char`;
> `String(N(n))`→`varchar(n)`, `String(Max|None)`→`varchar`; `Text`→`text`;
> `TinyInteger`/`SmallInteger`→`smallint`;
> `Integer`/`Unsigned`→`integer`; `BigInteger`/`BigUnsigned`→`bigint`;
> `Float`→`real`; `Double`→`double precision`; `Decimal(Some((p,s)))`→
> `decimal(p, s)`, `Decimal(None)`→`decimal`; `DateTime`→`timestamp without
> time zone`; `Timestamp`→`timestamp`; `TimestampWithTimeZone`→`timestamp
> with time zone`; `Time`→`time`; `Date`→`date`; `Interval(fields, p)`→
> `interval[ FIELDS][(p)]`; `Binary(_)`/`VarBinary(_)`/`Blob`→`bytea`
> (lengths discarded); `Bit(Some(n))`→`bit(n)`, `Bit(None)`→`bit`;
> `VarBit(n)`→`varbit(n)`; `Boolean`→`bool`; `Money(Some((p,s)))`→
> `money(p, s)`, `Money(None)`→`money` (Postgres `money` takes no arguments —
> rendered anyway); `Json`→`json`; `JsonBinary`→`jsonb`; `Uuid`→`uuid`;
> `Array(t)`→ recursive element spelling plus `[]`; `Vector(Some(n))`→
> `vector(n)`, `Vector(None)`→`vector`; `Custom(iden)`→ the unquoted iden
> text; `Enum { name, .. }`→ the unquoted enum type name; `Cidr`→`cidr`;
> `Inet`→`inet`; `MacAddr`→`macaddr`; `LTree`→`ltree`. `Year` has no
> Postgres spelling and panics (see `[spec:pgorm:sem:sql.ddl.panics]`).

> [spec:pgorm:req:sql.ddl.alter-table]
> `TableAlterStatement` collects `TableAlterOption`s: `AddColumn` (with an
> `if_not_exists` flag), `ModifyColumn`, `RenameColumn`, `DropColumn`,
> `AddForeignKey` and `DropForeignKey`. Rendering MUST emit a single `ALTER
> TABLE <table> ` prefix with the options comma-separated: `ADD COLUMN [IF
> NOT EXISTS ]<column-def>` (same column rendering as create, including the
> serial substitution for auto-increment); `RENAME COLUMN "a" TO "b"`;
> `DROP COLUMN "c"`; `ADD CONSTRAINT ... FOREIGN KEY ...` and
> `DROP CONSTRAINT "name"` (foreign-key clauses in `Mode::TableAlter`, i.e.
> without a nested `ALTER TABLE`).
>
> `ModifyColumn` decomposes into per-aspect Postgres actions: when a type is
> present, `ALTER COLUMN "c" TYPE <type>`; then per spec `ALTER COLUMN "c"
> DROP NOT NULL` (for `Null`), `SET NOT NULL`, `SET DEFAULT <expr>`,
> `ADD UNIQUE ("c")`, `ADD PRIMARY KEY ("c")`, `CHECK (<expr>)` or the
> `Extra` string, comma-separated. `AutoIncrement`, `Generated` and `Comment`
> specs are ignored in modify. An alter statement with zero options panics
> with `No alter option found`.

> [spec:pgorm:req:sql.ddl.drop-rename-truncate]
> `TableDropStatement` accumulates multiple table refs and MUST render
> `DROP TABLE [IF EXISTS ]"t1", "t2"[ RESTRICT][ CASCADE]` (`restrict()` and
> `cascade()` append `TableDropOpt`s in call order). `TableRenameStatement`
> MUST render `ALTER TABLE <from> RENAME TO <to>`. `TableTruncateStatement`
> MUST render `TRUNCATE TABLE <table>`; no `CASCADE`/`RESTART IDENTITY`
> options are exposed.

## Indexes

> [spec:pgorm:req:sql.ddl.index-create]
> `IndexCreateStatement` carries a target table, a `TableIndex` (name plus
> ordered `IndexColumn`s), and `primary`, `unique`, `nulls_not_distinct`,
> `index_type` and `if_not_exists` flags. `IntoIndexColumn` accepts an iden,
> `(iden, u32)` prefix, `(iden, IndexOrder)` or `(iden, u32, IndexOrder)`.
> The standalone form MUST render `CREATE [PRIMARY KEY ][UNIQUE ]INDEX [IF
> NOT EXISTS ]"name" ON <table>[ USING <type>] (cols)[ NULLS NOT DISTINCT]`,
> where `<type>` is `BTREE`, `GIN` (the `FullText` mapping, also set by
> `full_text()`), `HASH`, or a custom identifier, and each column renders as
> `"name"[ (prefix)][ ASC|DESC]` — the MySQL-style `(prefix)` length is still
> emitted even though Postgres does not accept it.
>
> There is no support for partial indexes (`WHERE`), `INCLUDE` columns,
> expression columns or operator classes in the current builder. Index table
> refs are limited to `Table` and `SchemaTable`; other `TableRef` forms panic
> with `Not supported`.

> [spec:pgorm:req:sql.ddl.index-drop]
> `IndexDropStatement` MUST render `DROP INDEX [IF EXISTS ]["schema".]"name"`.
> Only the schema portion of a `SchemaTable` ref is used (indexes are
> schema-scoped in Postgres); a plain `Table` ref contributes nothing, and
> any other ref form panics with `Not supported`.

## Foreign keys

> [spec:pgorm:req:sql.ddl.foreign-key]
> `TableForeignKey` holds an optional constraint name, the owning and
> referenced table refs, parallel column/ref-column lists, and optional
> `on_delete`/`on_update` `ForeignKeyAction`s (`Restrict`→`RESTRICT`,
> `Cascade`→`CASCADE`, `SetNull`→`SET NULL`, `NoAction`→`NO ACTION`,
> `SetDefault`→`SET DEFAULT`). `ForeignKeyCreateStatement` wraps one
> `TableForeignKey` with `from(table, cols)`/`to(table, cols)` accepting
> `IdenList` tuples for composite keys.
>
> The standalone statement MUST render `ALTER TABLE <from> ADD [CONSTRAINT
> "name" ]FOREIGN KEY (cols) REFERENCES <to> (ref-cols)[ ON DELETE <action>]
> [ ON UPDATE <action>]`; inside `CREATE TABLE` the same clause renders
> without the `ALTER TABLE`/`ADD` prefix, and inside `ALTER TABLE` options
> only the `ALTER TABLE` prefix is dropped. `ForeignKeyDropStatement` MUST
> render `ALTER TABLE <table> DROP CONSTRAINT "name"`. Foreign-key table
> refs accept `Table`, `SchemaTable` and `DatabaseSchemaTable`; other forms
> panic with `Not supported`.

## Enum types

> [spec:pgorm:req:sql.ddl.type-enum]
> `TypeCreateStatement` (via `Type::create()`) supports exactly one shape:
> `as_enum(name)` sets the type ref and `TypeAs::Enum`, and `values(iter)`
> appends variant idens. It MUST render `CREATE TYPE <name> AS ENUM
> ('v1', 'v2', ...)` — the type name is a quoted identifier (`TypeRef`
> supports `Type`, `SchemaType` and `DatabaseSchemaType` dotted forms) while
> the variants pass through the value pipeline, i.e. single-quoted string
> literals in `to_string` builds and bind parameters in parameterised builds.
> `TypeAs` has no other variants (composite/range/base are commented out
> upstream).

> [spec:pgorm:req:sql.ddl.type-alter-drop]
> `TypeAlterStatement` MUST render `ALTER TYPE <name>` followed by one
> option: `ADD VALUE 'v'`, `ADD VALUE 'v' BEFORE 'w'` / `AFTER 'w'`
> (`before()`/`after()` only upgrade an existing `Add` option and are no-ops
> otherwise), `RENAME TO 'new'`, or `RENAME VALUE 'old' TO 'new'`. All of
> these operands — including the `RENAME TO` target, which Postgres actually
> expects as an identifier — go through the value pipeline and render as
> single-quoted string literals; this is current behaviour. Unlike the other
> type builders, `TypeAlterStatement` methods take `self` by value.
>
> `TypeDropStatement` MUST render `DROP TYPE [IF EXISTS ]<name1>, <name2>
> [ CASCADE|RESTRICT]` with names as quoted (possibly schema-qualified)
> identifiers; `cascade()` and `restrict()` overwrite the same option slot,
> so the last call wins.

## Extensions

> [spec:pgorm:req:sql.ddl.extension]
> `ExtensionCreateStatement` MUST render `CREATE EXTENSION [IF NOT EXISTS ]
> <name>[ WITH SCHEMA <schema>][ VERSION <version>][ CASCADE]`, and
> `ExtensionDropStatement` MUST render `DROP EXTENSION [IF EXISTS ]<name>
> [ CASCADE][ RESTRICT]`. Name, schema and version are plain `String`s
> written verbatim — unquoted and unescaped. On drop, `cascade` and
> `restrict` are independent flags; setting both renders both keywords.
> `PgLTree` is a ready-made `Iden` rendering `ltree` (usable as an extension
> name via `From<PgLTree> for String`); the ltree column type itself is
> `ColumnType::LTree`.

## Panics and unsupported forms

> [spec:pgorm:sem:sql.ddl.panics]
> DDL building is panic-based rather than error-based at its edges:
> `auto_increment()` on a column whose type is not
> `SmallInteger`/`Integer`/`BigInteger` panics with `... doesn't support auto
> increment` at render time; `ColumnType::Year` panics with `Year is not
> available in Postgres.`; an empty `TableAlterStatement` panics with `No
> alter option found`. Table statements (`create`/`alter`/`rename`/`drop`/
> `truncate`) accept only `Table`, `SchemaTable` and `DatabaseSchemaTable`
> refs — alias-carrying, subquery, values-list and function-call `TableRef`
> forms panic with `Not supported`, as do unsupported ref forms in index and
> foreign-key positions. There is no `Result`-returning DDL build path.
