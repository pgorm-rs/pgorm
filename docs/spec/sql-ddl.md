# DDL Statement Builders

This section specifies the schema (DDL) statement builders in `pgorm-query`:
table create/alter/drop/rename/truncate (`pgorm-query/src/table/`), index and
foreign key statements (`pgorm-query/src/index/`,
`pgorm-query/src/foreign_key/`), `CREATE TYPE ... AS ENUM` and extension
statements (`pgorm-query/src/extension.rs`), `COMMENT ON` statements
(`pgorm-query/src/comment.rs`), and the rendering contract
implemented by the Postgres `QueryBuilder`
(`pgorm-query/src/backend/query_builder.rs`). All rules describe current
behaviour, including panics and leftovers from the multi-backend ancestry.

> [spec:pgorm:req:sql.ddl+3]
> The DDL surface MUST be reachable through the entry-point helpers: `Table`
> (`create`/`alter`/`drop`/`rename`/`truncate`), `Index` (`create`/`drop`),
> `ForeignKey` (`create`/`drop`), `Type` (`create`/`alter`/`drop`),
> `Extension` (`create`/`drop`) and `Comment` (`on_table`/`on_column`). Table,
> index, foreign-key and comment statements
> implement `SchemaStatementBuilder` (`build`, `build_any`, `to_string`), all
> of which delegate to the corresponding `prepare_*` method on the single
> Postgres `QueryBuilder`; type and extension statements provide equivalent
> `build_ref`/`build_collect`/`to_string` inherent methods. Every identifier a
> DDL statement renders — table, column and type names, and index, constraint
> and foreign-key names alike — MUST go through `Iden::prepare` and so render
> double-quoted (quote character `"`, embedded quotes doubled); no identifier
> is interpolated raw between quote characters. Index and constraint names are
> held as `DynIden` and accepted as `IntoIden`, so a `&str` or `String` name
> escapes through `Alias` like any other identifier. `TableStatement` is an
> enum wrapper carrying its own
> `build`/`build_any`/`to_string` dispatch methods; `IndexStatement`,
> `ForeignKeyStatement` and `SchemaStatement` are plain wrapper enums whose
> variants render through the same builders.

## Tables

> [spec:pgorm:req:sql.ddl.create-table+4]
> `TableCreateStatement` composes a table name (`table()`, any
> `IntoTableName`), ordered `ColumnDef`s (`col()`, which stamps the table ref
> onto each column), table-level indexes (`index()` and `primary_key()` — the
> latter takes an `IndexCreateStatement` and forces its kind to
> `IndexKind::PrimaryKey`, the one position in which that kind is spelled),
> foreign keys (`foreign_key()`), check expressions (`check()`), an
> `if_not_exists` flag, a `comment` and a trailing `extra` string.
>
> Rendering MUST emit `CREATE TABLE [IF NOT EXISTS ]<table> ( ... )` with the
> body in this fixed order: column definitions, then embedded index
> expressions, then foreign-key clauses (in `Mode::Creation`, i.e. without
> `ALTER TABLE`/`ADD`), then `CHECK (...)` constraints, all comma-separated.
> Embedded indexes render as `[CONSTRAINT "name" ][PRIMARY KEY |UNIQUE
> ][NULLS NOT DISTINCT ](cols)`, the keyword chosen by the statement's
> `IndexKind` (`[spec:pgorm:req:sql.ddl.index-create+2]`) and `NULLS NOT
> DISTINCT` emitted only for `Unique`. A `Plain` kind — reachable only through
> `index()`, since `primary_key()` sets the kind — contributes no keyword and
> so renders a constraint Postgres rejects. After the closing parenthesis only
> the `extra` string follows (e.g. `USING columnar`). There are no table
> options: the MySQL-era `TableOpt` (`Engine`, `Collate`, `CharacterSet`) and
> its `engine`/`collate`/`character_set` builders rendered `ENGINE=`,
> `COLLATE=` and `DEFAULT CHARSET=` trailers Postgres rejects, and the
> uninhabited `TablePartition` had no renderer at all; both are gone with the
> statement's `options` and `partitions` fields, and MUST NOT return. The
> table-level
> `comment` is stored and exposed via `get_comment()` but is not rendered
> here: on Postgres a table comment is a statement of its own, built through
> `[spec:pgorm:req:sql.ddl.comment]`.

> [spec:pgorm:req:sql.ddl.column-def+2]
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
> with the serial family, which `ColumnType::serial_spelling` defines over the
> integer trio alone — `SmallInteger`→`smallserial`, `Integer`→`serial`,
> `BigInteger`→`bigserial`. Every other type has no serial form, so the spec
> contributes nothing and the column renders its declared type; the
> substitution MUST NOT panic, and MUST NOT invent a serial spelling for a
> type Postgres has none for. `Comment` specs are skipped entirely — a column
> comment is a statement of its own (`[spec:pgorm:req:sql.ddl.comment]`).
> `IntoColumnDef` accepts both `ColumnDef` and `&mut ColumnDef` (via
> `take()`), enabling the builder-by-reference doctest style.

> [spec:pgorm:req:sql.ddl.column-types+2]
> `prepare_column_type` defines the `ColumnType` → Postgres type-name
> contract, and it is total: every variant has exactly one Postgres spelling
> and none can fail. It MUST spell: `Char(Some(n))`→`char(n)`,
> `Char(None)`→`char`; `String(N(n))`→`varchar(n)`,
> `String(Max|None)`→`varchar`; `Text`→`text`; `SmallInteger`→`smallint`;
> `Integer`→`integer`; `BigInteger`→`bigint`; `Float`→`real`;
> `Double`→`double precision`; `Decimal(Some((p,s)))`→`decimal(p, s)`,
> `Decimal(None)`→`decimal`; `Timestamp`→`timestamp`;
> `TimestampWithTimeZone`→`timestamp with time zone`; `Time`→`time`;
> `Date`→`date`; `Interval(fields, p)`→`interval[ FIELDS][(p)]`;
> `Bytea`→`bytea`; `Bit(Some(n))`→`bit(n)`, `Bit(None)`→`bit`;
> `VarBit(n)`→`varbit(n)`; `Boolean`→`bool`; `Money`→`money`; `Json`→`json`;
> `JsonBinary`→`jsonb`; `Uuid`→`uuid`; `Array(t)`→ recursive element spelling
> plus `[]`; `Vector(Some(n))`→`vector(n)`, `Vector(None)`→`vector`;
> `Custom(iden)`→ the unquoted iden text; `Enum { name, .. }`→ the unquoted
> enum type name; `Cidr`→`cidr`; `Inet`→`inet`; `MacAddr`→`macaddr`;
> `LTree`→`ltree`.

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

> [spec:pgorm:req:sql.ddl.drop-rename-truncate+1]
> `TableDropStatement` accumulates multiple `TableName`s and MUST render
> `DROP TABLE [IF EXISTS ]"t1", "t2"[ RESTRICT][ CASCADE]` (`restrict()` and
> `cascade()` append `TableDropOpt`s in call order). `TableRenameStatement`
> MUST render `ALTER TABLE <from> RENAME TO <to>`. `TableTruncateStatement`
> MUST render `TRUNCATE TABLE <table>`; no `CASCADE`/`RESTART IDENTITY`
> options are exposed.

## Comments

> [spec:pgorm:req:sql.ddl.comment+1]
> A comment is a statement of its own on Postgres, not a clause of `CREATE
> TABLE`, so `CommentStatement` is built separately from the DDL creating the
> object it describes. `Comment::on_table(table, text)` and
> `Comment::on_column(table, column, text)` are the only constructors and both
> take target and text up front, so every `CommentStatement` denotes a
> complete statement and no build path can fail or panic. The target table is
> a `TableName` (`[spec:pgorm:def:sql.types.table-ref+1]`) — the same type
> every other DDL statement targets, reached through `IntoTableName` from an
> iden or a `(schema, table)` tuple — so a comment can only name a table the
> DDL beside it could also name, and there is no conversion to fail.
>
> Rendering MUST emit `COMMENT ON TABLE <table> IS '<text>'` or
> `COMMENT ON COLUMN <table>.<column> IS '<text>'`, where the table, schema
> and column names render through `Iden::prepare` (double-quoted,
> embedded quotes doubled) and the text renders as a standard-conforming
> string literal: wrapped in single quotes with every embedded single quote
> doubled and nothing else altered — backslashes are literal, so no `E''`
> prefix is used and the escaping of
> `[spec:pgorm:req:sql.render.string-escape]` does not apply here. The text is
> never a bind parameter (`SchemaStatementBuilder` yields SQL alone), so this
> quoting is the whole injection boundary for comment text.

## Indexes

> [spec:pgorm:req:sql.ddl.index-create+2]
> `IndexCreateStatement` carries a target table, a `TableIndex` (name plus
> ordered `IndexColumn`s), an `IndexKind`, and `nulls_not_distinct`,
> `index_type` and `if_not_exists` flags. `IndexKind` is the closed set
> `Plain | Unique | PrimaryKey`, so what an index constrains is one state and
> never a combination: `primary()` and `unique()` each set the kind outright,
> replacing whatever was set before, and an index is never both a primary key
> and a unique key. `is_primary_key()`, `is_unique_key()` and `kind()` read it
> back. `IntoIndexColumn` accepts an iden, `(iden, u32)` prefix,
> `(iden, IndexOrder)` or `(iden, u32, IndexOrder)`.
>
> Postgres spells `PRIMARY KEY` only as an inline table constraint, so
> `IndexKind::PrimaryKey` has no standalone spelling and the standalone
> renderer MUST NOT be able to see it: it reads the kind through
> `IndexKind::standalone`, whose image is the two-variant
> `StandaloneIndexKind` (`Plain | Unique`) and which maps `PrimaryKey` to
> `None`. That absence is typed rather than a failure — a statement marked
> primary and rendered standalone emits a plain `CREATE INDEX`, and the
> primary-key constraint is reachable only through the embedded path of
> `[spec:pgorm:req:sql.ddl.create-table+4]`.
>
> The standalone form MUST render `CREATE [UNIQUE ]INDEX [IF NOT EXISTS
> ]"name" ON <table>[ USING <type>] (cols)[ NULLS NOT DISTINCT]`, where
> `<type>` is `BTREE`, `GIN` (the `FullText` mapping, also set by
> `full_text()`), `HASH`, or a custom identifier, and each column renders as
> `"name"[ (prefix)][ ASC|DESC]` — the MySQL-style `(prefix)` length is still
> emitted even though Postgres does not accept it. Postgres defines
> `NULLS NOT DISTINCT` for unique indexes alone, so the flag MUST render only
> when the kind is `Unique`; on any other kind it is carried but not spelled.
>
> There is no support for partial indexes (`WHERE`), `INCLUDE` columns,
> expression columns or operator classes in the current builder. The index
> target is a `TableName`, so both of its forms render and no other shape is
> constructible.

> [spec:pgorm:req:sql.ddl.index-drop+1]
> `IndexDropStatement` MUST render `DROP INDEX [IF EXISTS ]["schema".]"name"`.
> Only the schema portion of the target `TableName` is used (indexes are
> schema-scoped in Postgres); a plain `Table` name contributes nothing.

## Foreign keys

> [spec:pgorm:req:sql.ddl.foreign-key+1]
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
> targets are `TableName`s, so both forms render and no other shape is
> constructible.

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

> [spec:pgorm:sem:sql.ddl.panics+2]
> DDL building is panic-based rather than error-based at its one remaining
> edge: an empty `TableAlterStatement` panics with `No alter option found`.
> There is no `Result`-returning DDL build path.
>
> Column type and auto-increment shape are no longer among them, and MUST NOT
> return to them. `ColumnType` carries no variant without a Postgres spelling
> — `Year` is gone with the enum entry that produced the `Year is not
> available in Postgres.` panic — so `prepare_column_type` is total. The
> serial substitution is likewise total: `auto_increment()` on a type outside
> the integer trio renders the declared type rather than panicking with
> `... doesn't support auto increment`
> (`[spec:pgorm:req:sql.ddl.column-def+2]`). Neither guard is a `Result`; both
> are closed by making the renderer's match exhaustive over spellings that
> exist.
>
> Table reference shape is no longer among them, and MUST NOT return to
> them. Table statements (`create`/`alter`/`rename`/`drop`/`truncate`), index
> and foreign-key targets and comment targets take a `TableName`
> (`[spec:pgorm:def:sql.types.table-ref+1]`), which has no form the renderer
> could refuse. The five `Not supported` panics and the `TableRef with values
> is not support` panic that guarded these positions are gone, and a caller
> cannot reintroduce them: an aliased, subquery, values-list or function-call
> reference is a `FromItem` and does not typecheck as a DDL target.
