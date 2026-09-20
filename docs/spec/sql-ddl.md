# DDL Statement Builders

This section specifies the schema (DDL) statement builders in `pgorm-query`:
table create/alter/drop/rename/truncate (`pgorm-query/src/table/`), index and
foreign key statements (`pgorm-query/src/index/`,
`pgorm-query/src/foreign_key/`), `CREATE TYPE ... AS ENUM` and extension
statements (`pgorm-query/src/extension.rs`), `COMMENT ON` statements
(`pgorm-query/src/comment.rs`), and the rendering contract
implemented by the Postgres `QueryBuilder`
(`pgorm-query/src/backend/query_builder.rs`). All rules describe current
behaviour, including the leftovers from the multi-backend ancestry.

> [spec:pgorm:req:sql.ddl+7]
> The DDL surface MUST be reachable through the entry-point helpers: `Table`
> (`create`/`alter`/`drop`/`rename`/`rename_column`/`truncate`), `Index`
> (`create`/`drop`),
> `ForeignKey` (`create`/`drop`), `Type` (`create`/`alter`/`drop`),
> `Extension` (`create`/`drop`) and `Comment` (`on_table`/`on_column`).
>
> The render surface a statement exposes MUST follow from what it binds, not
> from which family it belongs to. Every statement has the value-inlined
> rendering — its `Display`, reached as `to_string()` — and that rendering MUST
> carry, on the `impl` itself rather than only in module prose, the note that
> it inlines rather than binds, pointing at `build` where the statement has
> one.
>
> `TypeCreateStatement` and `TypeAlterStatement` push their enum labels through
> `push_param` (`[spec:pgorm:req:sql.render.ddl.enum-type+5]`), so they have a
> second rendering and MUST expose it as `build() -> (String, Values)` beside
> the `build_collect(sink)` they already had — the capability named, rather
> than reachable only by constructing a `SqlWriterValues` and calling the
> undocumented `into_parts`. PostgreSQL accepts no bind parameter in DDL, so
> that pair is for inspection and the inlined rendering is what a caller
> executes; both the `build` doc and the `Display` doc MUST say so.
>
> Every other DDL statement — table, index, foreign-key, comment, extension,
> `DROP TYPE` — has no placeholder-emitting entry point at all: its renderer is
> `pub(crate)` and its only public route is `Display` over a `String` sink, so
> a value it carries (a column `DEFAULT`, a `CHECK` expression) is always
> inlined as an escaped literal and there is nothing left to bind. These expose
> `to_string()` and nothing else, delegating to the corresponding `prepare_*`
> method on the single Postgres `QueryBuilder`. The claim they "carry no bind
> parameters" is the one to avoid restating: they carry values, and inline
> them.
>
> The `SchemaStatementBuilder` trait and its `build`/`build_any`/`to_string`
> triplication are gone, as are the `build_ref`/`build_collect_ref` inherent
> methods on type and extension statements. No
> rendering method takes a `QueryBuilder` argument — the builder is a stateless
> unit struct, so passing one carried no information.
>
> Every identifier a
> DDL statement renders — table, column and type names, and index, constraint
> and foreign-key names alike — MUST go through `SqlName::prepare` and so render
> double-quoted (quote character `"`, embedded quotes doubled); no identifier
> is interpolated raw between quote characters. Index and constraint names are
> held as `Name` and accepted as `IntoName`, so a runtime name minted by
> `Name::runtime` escapes like any other. `TableStatement` is an
> enum wrapper whose `Display` dispatches to the variant's own; `IndexStatement`,
> `ForeignKeyStatement` and `SchemaStatement` are plain wrapper enums whose
> variants render through the same builders.

## Tables

> [spec:pgorm:req:sql.ddl.create-table+7]
> `TableCreateStatement` composes a table name, ordered `ColumnDef`s (`col()`,
> which stamps the table ref onto each column), table-level indexes (`index()`
> and `primary_key()` — the latter takes an `IndexCreateStatement` and forces
> its kind to `IndexKind::PrimaryKey`, the one position in which that kind is
> spelled; both restamp the index onto the owning table, as `col()` restamps
> each column, so an embedded index cannot name another table),
> foreign keys (`foreign_key()`), check expressions (`check()`), an
> `if_not_exists` flag, a `comment` and a trailing `extra` string.
>
> Every embedder MUST take what it embeds by value. `index()`, `primary_key()`
> and `foreign_key()` are bounded `Into<IndexCreateStatement>` /
> `Into<ForeignKeyCreateStatement>` and consume the argument, as `col()`'s
> `IntoColumnDef` consumes a column: a caller reusing the value writes
> `.to_owned()` and sees the copy in the source, rather than the embedder
> cloning or draining a `&mut` behind their back. Reuse after the call
> therefore has one outcome across all four, not three
> (`[spec:pgorm:req:sql.ast+1]`).
>
> Rendering MUST emit `CREATE TABLE [IF NOT EXISTS ]<table> ( ... )` with the
> body in this fixed order: column definitions, then embedded index
> expressions, then foreign-key clauses (in `Mode::Creation`, i.e. without
> `ALTER TABLE`/`ADD`), then `CHECK (...)` constraints, all comma-separated.
> Embedded indexes render as `[CONSTRAINT "name" ][PRIMARY KEY |UNIQUE
> ][NULLS NOT DISTINCT ](cols)`, the keyword chosen by the statement's
> `IndexKind` (`[spec:pgorm:req:sql.ddl.index-create+7]`) and `NULLS NOT
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
>
> The table name is structural rather than checked: `Table::create(table)` and
> `TableCreateStatement::new(table)` take any `IntoTableName` and there is no
> `table()` setter, so the `CREATE TABLE  ( ... )` PostgreSQL rejects at the
> parenthesis has no constructor
> (`[dec:pgorm:invalid-states-unrepresentable]`). `take()` moves every
> accumulated part out and copies only that name, for the same reason
> (`[spec:pgorm:req:sql.ast+1]`).
>
> A statement with no columns renders `CREATE TABLE <table> (  )`, and that is
> deliberately left buildable: PostgreSQL accepts a table with no columns, so
> the empty body is odd rather than invalid and gets documented rather than
> forbidden. Unlike an empty alter or a missing target, there is no unparseable
> render here for a type to prevent.

> [spec:pgorm:req:sql.ddl.column-def+4]
> `ColumnDef` holds a name, an optional `ColumnType` and an ordered list of
> `ColumnSpec`s (`Null`, `NotNull`, `Default(SimpleExpr)`, `AutoIncrement`,
> `UniqueKey`, `PrimaryKey`, `Check(SimpleExpr)`, `Generated { expr }`,
> `Extra(String)`, `Comment(String)`), populated by the fluent typed setters
> (`integer()`, `string_len(n)`, `timestamp_with_time_zone()`, `interval()`,
> `vector()`, `enumeration()`, `array(elem)`, `cidr()`, `ltree()`, ...,
> `not_null()`, `default(v)`, `check(expr)`, `extra(s)`, etc.).
>
> A column MUST render as the quoted name, one space, the type spelling, then
> each spec in insertion order: `NULL`, `NOT NULL`, `DEFAULT <expr>`,
> `UNIQUE`, `PRIMARY KEY`, `CHECK (<expr>)`, `GENERATED ALWAYS AS (<expr>)
> STORED`, and `Extra` verbatim. A generated column is always stored and
> `generated(expr)` takes no flag saying otherwise: `VIRTUAL` is a syntax
> error on every PostgreSQL before 18, the builder cannot know which release
> it is writing for, and rendering is infallible — there is no error channel
> to refuse through, so the refusal is that the spec carries no `stored`
> field and the non-stored column has no constructor
> (`[dec:pgorm:invalid-states-unrepresentable]`). The `ColumnSpec::Generated
> { expr, stored }` shape and the `VIRTUAL` render MUST NOT return.
> `AutoIncrement` produces no keyword; instead it replaces the type spelling
> with the serial family, which `ColumnType::serial_spelling` defines over the
> integer trio alone — `SmallInteger`→`smallserial`, `Integer`→`serial`,
> `BigInteger`→`bigserial`. Every other type has no serial form, so the spec
> contributes nothing and the column renders its declared type; the
> substitution MUST NOT panic, and MUST NOT invent a serial spelling for a
> type Postgres has none for. `Comment` specs are skipped entirely — a column
> comment is a statement of its own (`[spec:pgorm:req:sql.ddl.comment]`).
> `IntoColumnDef` accepts both `ColumnDef` and `&mut ColumnDef` (via
> `take()`), enabling the builder-by-reference doctest style; `take()` clones
> the name rather than swapping in a placeholder identifier, so no empty
> identifier exists to leak into a rendered column.
>
> `Extra` is deliberately verbatim and is the one DDL render that interpolates
> a caller string unquoted. It exists as the escape hatch for column SQL the
> `ColumnType`/`ColumnSpec` vocabulary cannot spell, so quoting or escaping it
> would defeat its only purpose: whatever a caller puts there is emitted as
> written, and the caller owns its trustworthiness — including whether it
> parses at all. Anything expressible through the typed setters MUST use them
> instead.

> [spec:pgorm:req:sql.ddl.column-types+4]
> `prepare_column_type` defines the `ColumnType` → Postgres type-name
> contract, and it is total: every variant has exactly one Postgres spelling
> and none can fail. It MUST spell: `Char(Some(n))`→`char(n)`,
> `Char(None)`→`char`; `String(N(n))`→`varchar(n)`,
> `String(Max|None)`→`varchar`; `Text`→`text`; `SmallInteger`→`smallint`;
> `Integer`→`integer`; `BigInteger`→`bigint`; `Float`→`real`;
> `Double`→`double precision`; `Decimal(Some((p,s)))`→`decimal(p, s)`,
> `Decimal(None)`→`decimal`; `Timestamp`→`timestamp`;
> `TimestampWithTimeZone`→`timestamp with time zone`; `Time`→`time`;
> `Date`→`date`; `Interval(Any(None))`→`interval`,
> `Interval(Any(Some(p)))`→`interval(p)`,
> `Interval(Fields(f))`→`interval FIELDS`, where a second-bearing field
> spells its own precision (`SECOND(3)`, `HOUR TO SECOND(3)`);
> `Bytea`→`bytea`; `Bit(Some(n))`→`bit(n)`, `Bit(None)`→`bit`;
> `VarBit(n)`→`varbit(n)`; `Boolean`→`bool`; `Money`→`money`; `Json`→`json`;
> `JsonBinary`→`jsonb`; `Uuid`→`uuid`; `Array(t)`→ recursive element spelling
> plus `[]`; `Vector(Some(n))`→`vector(n)`, `Vector(None)`→`vector`;
> `Named(type_name)` and `Enum { name, .. }`→ the type name through
> `TypeName`'s part policy (`sql.types.type-name`), a safe lowercase name
> bare and anything else quoted; `Cidr`→`cidr`; `Inet`→`inet`; `MacAddr`→`macaddr`;
> `LTree`→`ltree`.

> [spec:pgorm:req:sql.ddl.alter-table+4]
> `TableAlterStatement` names one table and collects `TableAlterOption`s:
> `AddColumn` (with an `if_not_exists` flag), `ModifyColumn`, `DropColumn`,
> `AddForeignKey` and `DropForeignKey`. Both the table and a first option are
> structural rather than checked: `Table::alter(table)` yields a
> `PendingTableAlter`, which is a named table and nothing more — it implements no
> build path and cannot render — and each of its six action methods consumes it
> and returns the statement, whose own methods append the rest. PostgreSQL parses
> neither `ALTER TABLE "font"` nor `ALTER TABLE ADD COLUMN ...`, and neither MUST
> be constructible (`[dec:pgorm:invalid-states-unrepresentable]`); the
> `No alter option found` panic that stood in for the first of those is gone, and
> MUST NOT return. The statement has no `take()` for the same reason: moving the
> options out would leave the action-less statement this type exists to rule
> out, and a `take()` that copied instead would be a promise the name does not
> keep (`[spec:pgorm:req:sql.ast+1]`) — a second copy is `.to_owned()`.
>
> `add_foreign_key` — on both `PendingTableAlter` and `TableAlterStatement` —
> takes `Into<TableForeignKey>` by value, as `add_column` takes
> `IntoColumnDef`: an embedder consumes what it embeds, and a borrow that
> silently cloned would be the third reuse-outcome
> `[spec:pgorm:req:sql.ddl.create-table+7]` rules out.
>
> Rendering MUST emit a single `ALTER TABLE <table> ` prefix
> with the options comma-separated: `ADD COLUMN [IF NOT EXISTS ]<column-def>`
> (same column rendering as create, including the serial substitution for
> auto-increment); `DROP COLUMN "c"`; `ADD CONSTRAINT ... FOREIGN KEY ...` and
> `DROP CONSTRAINT "name"` (foreign-key clauses in `Mode::TableAlter`, i.e.
> without a nested `ALTER TABLE`).
>
> A column rename is NOT one of those options. PostgreSQL admits `RENAME` only
> as the sole action of an `ALTER TABLE`, so it is a statement of its own:
> `Table::rename_column(table, from, to)` builds a `ColumnRenameStatement`
> rendering `ALTER TABLE <table> RENAME COLUMN "a" TO "b"`, and
> `TableStatement::RenameColumn` carries it. A rename listed beside an
> `ADD COLUMN` therefore does not construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). All three names are
> constructor arguments and none has a setter: PostgreSQL rejects the render
> that omits any of them, so the partly-named rename does not construct
> either.
>
> `ModifyColumn` decomposes into per-aspect Postgres actions: when a type is
> present, `ALTER COLUMN "c" TYPE <type>`; then per spec `ALTER COLUMN "c"
> DROP NOT NULL` (for `Null`), `SET NOT NULL`, `SET DEFAULT <expr>`,
> `ADD UNIQUE ("c")`, `ADD PRIMARY KEY ("c")`, `CHECK (<expr>)` or the
> `Extra` string, comma-separated. `AutoIncrement`, `Generated` and `Comment`
> specs are ignored in modify.

> [spec:pgorm:req:sql.ddl.drop-rename-truncate+4]
> `TableDropStatement` accumulates multiple `TableName`s and MUST render
> `DROP TABLE [IF EXISTS ]"t1", "t2"[ RESTRICT][ CASCADE]` (`restrict()` and
> `cascade()` append `TableDropOpt`s in call order). `TableRenameStatement`
> MUST render `ALTER TABLE <from> RENAME TO <to>`, where the source is a
> `TableName` and the target is a bare `Name`: `RENAME TO` cannot move a
> table between schemas, so a qualified target does not construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). `TableTruncateStatement`
> MUST render `TRUNCATE TABLE <table>`; no `CASCADE`/`RESTART IDENTITY`
> options are exposed.
>
> All three take their targets in the constructor, because PostgreSQL rejects
> every one of these statements with the name left out: `Table::drop(table)`
> seeds the list and `table()` appends the rest, in the pattern
> `[spec:pgorm:req:sql.ddl.index-create+7]` uses for index columns, so the
> empty `DROP TABLE ` cannot be built; `Table::rename(from, to)` and
> `Table::truncate(table)` take theirs whole and expose no setter. `take()` on
> a drop copies the target list rather than moving it, so no target-less husk
> is left behind.

## Comments

> [spec:pgorm:req:sql.ddl.comment+4]
> A comment is a statement of its own on Postgres, not a clause of `CREATE
> TABLE`, so `CommentStatement` is built separately from the DDL creating the
> object it describes. `Comment::on_table(table, text)` and
> `Comment::on_column(table, column, text)` are the only constructors and both
> take target and text up front, so every `CommentStatement` denotes a
> complete statement and no build path can fail or panic. The target table is
> a `TableName` (`[spec:pgorm:def:sql.types.table-ref+3]`) — the same type
> every other DDL statement targets, reached through `IntoTableName` from an
> iden or a `(schema, table)` tuple — so a comment can only name a table the
> DDL beside it could also name, and there is no conversion to fail.
>
> Rendering MUST emit `COMMENT ON TABLE <table> IS '<text>'` or
> `COMMENT ON COLUMN <table>.<column> IS '<text>'`, where the table, schema
> and column names render through `SqlName::prepare` (double-quoted,
> embedded quotes doubled) and the text renders as a standard-conforming
> string literal: wrapped in single quotes with every embedded single quote
> doubled and nothing else altered — backslashes are literal, so no `E''`
> prefix is used and the escaping of
> `[spec:pgorm:req:sql.render.string-escape+1]` does not apply here. The text is
> never a bind parameter (a DDL statement yields SQL alone), so this
> quoting is the whole injection boundary for comment text.
>
> The comment text a `TableCreateStatement` carries — its own
> (`[spec:pgorm:req:sql.ddl.create-table+7]`) and each `ColumnSpec::Comment`
> (`[spec:pgorm:req:sql.ddl.column-def+4]`) — MUST be reachable as those
> statements: `TableCreateStatement::comments()` returns one
> `CommentStatement` per carried comment, the table's first and then one per
> commented column in column order, each targeting the statement's own table.
> The setters therefore describe a table completely and no carried text is
> unrenderable. They MUST NOT instead be appended to the create statement's
> own SQL: that string is executed as a single prepared statement
> (`[spec:pgorm:sem:conn.pool.statement-cache+2]`), and PostgreSQL refuses to
> prepare a string holding more than one command, so appending would trade a
> comment silently dropped for a `CREATE TABLE` that no longer runs.

## Indexes

> [spec:pgorm:req:sql.ddl.index-create+7]
> `IndexCreateStatement` carries a target table, a `TableIndex` (name plus
> ordered `IndexColumn`s), an `IndexKind`, and `nulls_not_distinct`,
> `index_type` and `if_not_exists` flags. Its target table and its column list
> MUST both be non-empty by construction: `Index::create(table, col)` and
> `IndexCreateStatement::new(table, col)` take the table and the first column
> and `col()` appends the rest, in the pattern
> `[spec:pgorm:def:sql.ast.with+3]` uses for CTEs, and there is no `table()`
> setter. It has no `take()` at all: moving the table or the columns out would
> leave exactly the target-less, column-less husk the constructor rules out, and
> a `take()` that copied instead would be a promise the name does not keep
> (`[spec:pgorm:req:sql.ast+1]`) — a second copy is `.to_owned()`. PostgreSQL rejects an empty
> column list in every position this statement reaches — standalone
> `CREATE INDEX ... ()` and the embedded `PRIMARY KEY ()` and `UNIQUE ()` of
> `[spec:pgorm:req:sql.ddl.create-table+7]` alike — and rejects
> `CREATE INDEX "n" ON  (...)` at the parenthesis, so both states are
> unreachable rather than checked
> (`[dec:pgorm:invalid-states-unrepresentable]`). The index *name* is the one
> part that stays optional: PostgreSQL derives a name when `CREATE INDEX`
> omits it, so `CREATE INDEX  ON "t" ("c")` parses and is left buildable. In
> the embedded position of `[spec:pgorm:req:sql.ddl.create-table+7]` the table
> is not rendered at all and the owning statement restamps it, so the
> constructor argument there names the table the index already belongs to
> rather than a second one. `IndexKind`
> is the closed set
> `Plain | Unique | PrimaryKey`, so what an index constrains is one state and
> never a combination: `primary()` and `unique()` each set the kind outright,
> replacing whatever was set before, and an index is never both a primary key
> and a unique key. `is_primary_key()`, `is_unique_key()` and `kind()` read it
> back. `IntoIndexColumn` accepts an iden or an `(iden, IndexOrder)` pair,
> and nothing else: the MySQL prefix-length forms `(iden, u32)` and
> `(iden, u32, IndexOrder)` are gone with the `IndexColumn::prefix` field they
> fed, and MUST NOT return.
>
> Postgres spells `PRIMARY KEY` only as an inline table constraint, so
> `IndexKind::PrimaryKey` has no standalone spelling and the standalone
> renderer MUST NOT be able to see it: it reads the kind through
> `IndexKind::standalone`, whose image is the two-variant
> `StandaloneIndexKind` (`Plain | Unique`) and which maps `PrimaryKey` to
> `None`. That absence is typed rather than a failure — a statement marked
> primary and rendered standalone emits a plain `CREATE INDEX`, and the
> primary-key constraint is reachable only through the embedded path of
> `[spec:pgorm:req:sql.ddl.create-table+7]`.
>
> The standalone form MUST render `CREATE [UNIQUE ]INDEX [IF NOT EXISTS
> ]"name" ON <table>[ USING <type>] (cols)[ NULLS NOT DISTINCT]`, where
> `<type>` is `BTREE`, `GIN` (the `IndexType::Gin` variant, also set by the
> `gin()` shorthand — the access method is named for what PostgreSQL calls it,
> not for the full-text use it serves), `HASH`, or a custom identifier, and
> each column renders as
> `"name"[ ASC|DESC]`. There is no prefix length: `"name" (128)` is MySQL's
> syntax for indexing a leading substring, PostgreSQL rejects it outright, and
> an index the server cannot accept MUST NOT be constructible — the expression
> index is the legitimate occupant of that syntactic position. Postgres defines
> `NULLS NOT DISTINCT` for unique indexes alone, so the flag MUST render only
> when the kind is `Unique`; on any other kind it is carried but not spelled.
>
> There is no support for partial indexes (`WHERE`), `INCLUDE` columns,
> expression columns or operator classes in the current builder. The index
> target is a `TableName`, so both of its forms render and no other shape is
> constructible.

> [spec:pgorm:req:sql.ddl.index-drop+3]
> `IndexDropStatement` MUST render `DROP INDEX [IF EXISTS ]["schema".]"name"`.
> The index name is a `Name` taken by `Index::drop(name)`, being the whole
> of what the statement names: PostgreSQL rejects `DROP INDEX ` at end of
> input, so the nameless drop does not construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). The table is the part that
> stays optional, and only its schema portion is used (indexes are
> schema-scoped in Postgres); a plain `Table` name contributes nothing, and
> `DROP INDEX "name"` with no table at all is valid PostgreSQL.

## Foreign keys

> [spec:pgorm:req:sql.ddl.foreign-key+4]
> `TableForeignKey` holds the owning and referenced table names, a non-empty
> list of `(column, referenced column)` pairs, an optional constraint name, and
> optional `on_delete`/`on_update` `ForeignKeyAction`s (`Restrict`→`RESTRICT`,
> `Cascade`→`CASCADE`, `SetNull`→`SET NULL`, `NoAction`→`NO ACTION`,
> `SetDefault`→`SET DEFAULT`). `TableForeignKey::new(table, column, ref_table,
> ref_column)` — reached from a statement as `ForeignKey::create(..)` — takes
> both tables and the first pair, and `col(column, ref_column)` appends further
> pairs; there is no setter for either table and no constructor taking a column
> list, because PostgreSQL rejects `ALTER TABLE  ADD FOREIGN KEY`,
> `FOREIGN KEY ()`, `REFERENCES  ()` and `REFERENCES "t" ()` alike
> (`[dec:pgorm:invalid-states-unrepresentable]`). Holding the two sides as one
> list of pairs also makes the arity mismatch unrepresentable — a render the
> grammar accepts and only parse analysis rejects, so no oracle could have
> caught it. Neither `TableForeignKey` nor the `ForeignKeyCreateStatement`
> wrapping it has a `take()`: moving the tables and the first pair out would
> leave exactly the husk the constructor rules out, and a `take()` that copied
> them would be a promise the name does not keep
> (`[spec:pgorm:req:sql.ast+1]`). A second copy is `.to_owned()`.
>
> The standalone statement MUST render `ALTER TABLE <from> ADD [CONSTRAINT
> "name" ]FOREIGN KEY (cols) REFERENCES <to> (ref-cols)[ ON DELETE <action>]
> [ ON UPDATE <action>]`; inside `CREATE TABLE` the same clause renders
> without the `ALTER TABLE`/`ADD` prefix, and inside `ALTER TABLE` options
> only the `ALTER TABLE` prefix is dropped. On the `CREATE TABLE` path the key
> is restamped onto the owning table by `TableCreateStatement::foreign_key`, as
> an embedded index is by `index()`: an embedded key constrains the table it
> sits inside and MUST NOT name another. That embedder, and the
> `add_foreign_key` of `[spec:pgorm:req:sql.ddl.alter-table+4]`, take the key by
> value (`Into<ForeignKeyCreateStatement>` and `Into<TableForeignKey>`
> respectively) rather than by reference: an embedder consumes what it embeds,
> so a caller who reuses the key writes the copy
> (`[spec:pgorm:req:sql.ddl.create-table+7]`). `ForeignKeyDropStatement` MUST
> render `ALTER TABLE <table> DROP CONSTRAINT "name"`; both halves are taken by
> `ForeignKey::drop(table, name)` and neither has a setter, for the same reason.
> It holds the constraint name
> directly rather than a whole `TableForeignKey`, and renders through its own
> `prepare_foreign_key_drop_statement`; the `DROP CONSTRAINT` clause of an
> `ALTER TABLE` option is written by the alter renderer instead of borrowing
> this statement. Foreign-key table targets are `TableName`s, so both forms
> render and no other shape is constructible.

## Enum types

> [spec:pgorm:req:sql.ddl.type-enum+5]
> An enum type reference declares an optional schema — `Type::create` and its
> siblings take any `IntoTypeRef`, so `(schema, name)` names a qualified type
> and a bare name an unqualified one — and every DDL rendering MUST qualify
> when a schema is present, via `TypeRef`'s quoted, dot-joined parts.
> `TypeCreateStatement` takes its type name in `Type::create(name)`, because
> `CREATE TYPE ` is rejected at end of input and a nameless statement
> therefore MUST NOT construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). The name alone MUST render
> `CREATE TYPE <name>`, which PostgreSQL accepts as a shell type. `as_enum()`
> makes it an enumeration and `values(iter)` appends labels, implying
> `as_enum()` when it has not been called — the marker and the labels are one
> field (`TypeAs::Enum(Vec<String>)`), so no label list survives without the
> `AS ENUM` that renders it. A label is DATA, not a name: it renders as a
> string literal, so `values` is bound `Into<String>` and MUST NOT take an
> identifier type. The two are not interchangeable — an identifier bound
> would invite a caller to pass an `SqlName` whose text is then emitted as a
> literal, spelling a contract the render does not keep. An enumeration MUST render `CREATE TYPE <name> AS
> ENUM (<labels>)` with the parentheses always present, empty list included:
> `CREATE TYPE "t" AS ENUM ()` is an accepted spelling of the empty enum, and
> it was the missing parentheses — `CREATE TYPE "t" AS ENUM` — that PostgreSQL
> rejected. The type name is a quoted identifier (`TypeRef`
> supports `Type`, `SchemaType` and `DatabaseSchemaType` dotted forms) while
> the labels pass through the value pipeline, i.e. single-quoted string
> literals in `to_string` builds and bind parameters in parameterised builds.
> `TypeAs` has no other variants (composite/range/base are commented out
> upstream).

> [spec:pgorm:req:sql.ddl.type-alter-drop+5]
> `TypeAlterStatement` MUST render `ALTER TYPE <name>` followed by exactly one
> option: `ADD VALUE 'v'`, `ADD VALUE 'v' BEFORE 'w'` / `AFTER 'w'`
> (`before()`/`after()` only upgrade an existing `Add` option and are no-ops
> otherwise), `RENAME TO "new"`, or `RENAME VALUE 'old' TO 'new'`. The enum
> labels go through the value pipeline and render as single-quoted string
> literals; the `RENAME TO` target is a type name, not a label, and MUST
> render as a quoted identifier. The bounds MUST say which is which:
> `add_value`, `rename_value`, `before` and `after` take `Into<String>` and
> carry `String` in `TypeAlterOpt::Add` / `RenameValue` and
> `TypeAlterAddOpt::Before` / `After`, while `rename_to` keeps `IntoName`
> and `TypeAlterOpt::Rename` keeps `Name`. Unlike the other type builders,
> `TypeAlterStatement` methods take `self` by value.
>
> `Type::alter(name)` yields a `PendingTypeAlter` rather than a statement, and
> each option method consumes it into a `TypeAlterStatement` carrying the name
> and that one option: PostgreSQL rejects both `ALTER TYPE ` and `ALTER TYPE
> "t"` with no option, so neither the nameless nor the option-less form MUST be
> constructible (`[dec:pgorm:invalid-states-unrepresentable]`), the same
> `PendingTableAlter` shape `Table::alter` uses.
>
> `TypeDropStatement` MUST render `DROP TYPE [IF EXISTS ]<name1>, <name2>
> [ CASCADE|RESTRICT]` with names as quoted (possibly schema-qualified)
> identifiers; `cascade()` and `restrict()` overwrite the same option slot,
> so the last call wins. `Type::drop(name)` takes the first name and `name()` /
> `names()` append further ones, so the list is non-empty by construction and
> the `DROP TYPE ` PostgreSQL rejects at end of input does not build.

## Extensions

> [spec:pgorm:req:sql.ddl.extension+5]
> `ExtensionCreateStatement` MUST render `CREATE EXTENSION [IF NOT EXISTS ]
> <name>[ WITH SCHEMA <schema>][ VERSION <version>][ CASCADE]`, and
> `ExtensionDropStatement` MUST render `DROP EXTENSION [IF EXISTS ]<name>
> [ CASCADE| RESTRICT]`. The name is a `Name` taken by
> `Extension::create(name)` / `Extension::drop(name)` and has no setter: it used
> to default to the empty `String`, which renders as the zero-length delimited
> identifier `""` PostgreSQL rejects, so a statement that never names an
> extension MUST NOT construct
> (`[dec:pgorm:invalid-states-unrepresentable]`). An explicitly empty
> identifier remains the caller's own to avoid, as `Name::runtime("")` is
> everywhere else in the crate. The schema is a `Name` set by
> `schema(impl IntoName)` — a schema qualifier is a name, and it takes the
> identifier type every other schema position in the crate takes — while the
> version stays a `String`, because the grammar puts a string literal there
> and text is what it is. None of the three is written verbatim: name and
> schema render as quoted identifiers and version as a quoted string literal
> (`[spec:pgorm:sem:sql.render.ddl.extension+3]`). On drop, `CASCADE` and
> `RESTRICT` share one `ExtensionDropOpt` slot that `cascade()`/`restrict()`
> overwrite, so the pair PostgreSQL rejects does not construct; a drop carries
> no schema or version, because it renders neither.
> `PgLTree` is a ready-made `SqlName` rendering `ltree` (usable directly as an
> extension name); the ltree column type itself is `ColumnType::LTree`.

## Panics and unsupported forms

> [spec:pgorm:sem:sql.ddl.panics+4]
> DDL building does not panic. Every `prepare_*` path over a constructible
> statement runs to a rendered string, so the absence of a `Result`-returning
> DDL build path costs a caller nothing: there is no failure for one to carry.
> The last edge was the empty `TableAlterStatement`, which panicked with
> `No alter option found`; it is closed by construction rather than converted to
> an error, because a statement with no action is not a statement
> (`[spec:pgorm:req:sql.ddl.alter-table+4]`). It MUST NOT come back, in that
> form or as a `Result`.
>
> Column type and auto-increment shape were panics of their own, and MUST NOT
> come back either. `ColumnType` carries no variant without a Postgres spelling
> — `Year` is gone with the enum entry that produced the `Year is not
> available in Postgres.` panic — so `prepare_column_type` is total. The
> serial substitution is likewise total: `auto_increment()` on a type outside
> the integer trio renders the declared type rather than panicking with
> `... doesn't support auto increment`
> (`[spec:pgorm:req:sql.ddl.column-def+4]`). Neither guard is a `Result`; both
> are closed by making the renderer's match exhaustive over spellings that
> exist.
>
> Table reference shape was the third, and MUST NOT come back
> either. Table statements (`create`/`alter`/`rename`/`drop`/`truncate`), index
> and foreign-key targets and comment targets take a `TableName`
> (`[spec:pgorm:def:sql.types.table-ref+3]`), which has no form the renderer
> could refuse. The five `Not supported` panics and the `TableRef with values
> is not support` panic that guarded these positions are gone, and a caller
> cannot reintroduce them: binding an alias makes a reference a `NamedTable`,
> and a subquery, values-list or function-call reference is a `FromItem`;
> neither typechecks as a DDL target.
