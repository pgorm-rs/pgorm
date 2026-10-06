use crate::{
    ColumnDef, IntoColumnDef, IntoTableKey, Primary, SimpleExpr, TableForeignKey, TableKey, Unique,
    backend::QueryBuilder, types::*,
};

/// A table awaiting its first alter action.
///
/// PostgreSQL has no spelling for an `ALTER TABLE` that does nothing, so this is
/// what [`Table::alter`] returns: naming the table is not yet a statement. Each
/// action method consumes it and yields a [`TableAlterStatement`], which carries
/// the table and at least one action for the rest of its life.
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::alter(Font::Table).to_string();
/// ```
///
/// [`Table::alter`]: crate::Table::alter
// [spec:pgorm:req:sql.ddl.alter-table+8]
#[derive(Debug, Clone)]
pub struct PendingTableAlter {
    table: TableName,
}

impl PendingTableAlter {
    pub(crate) fn new(table: TableName) -> Self {
        Self { table }
    }

    fn with(self, option: TableAlterOption) -> TableAlterStatement {
        TableAlterStatement {
            table: self.table,
            options: vec![option],
        }
    }

    /// Add a column to an existing table
    pub fn add_column<C: IntoColumnDef>(self, column_def: C) -> TableAlterStatement {
        self.with(TableAlterOption::add_column(column_def, false))
    }

    /// Try add a column to an existing table if it does not exists
    pub fn add_column_if_not_exists<C: IntoColumnDef>(self, column_def: C) -> TableAlterStatement {
        self.with(TableAlterOption::add_column(column_def, true))
    }

    /// Modify a column in an existing table
    pub fn modify_column<C: IntoColumnDef>(self, column_def: C) -> TableAlterStatement {
        self.with(TableAlterOption::ModifyColumn(column_def.into_column_def()))
    }

    /// Drop a column from an existing table
    pub fn drop_column<T>(self, col_name: T) -> TableAlterStatement
    where
        T: IntoName,
    {
        self.with(TableAlterOption::DropColumn(col_name.into_name()))
    }

    /// Add a foreign key to existing table
    ///
    /// The key is consumed, as [`add_column`](Self::add_column) consumes its
    /// column: pass an owned value, and `.to_owned()` a binding you mean to
    /// reuse.
    pub fn add_foreign_key<F>(self, foreign_key: F) -> TableAlterStatement
    where
        F: Into<TableForeignKey>,
    {
        self.with(TableAlterOption::AddForeignKey(foreign_key.into()))
    }

    /// Drop a foreign key from existing table
    pub fn drop_foreign_key<T>(self, name: T) -> TableAlterStatement
    where
        T: IntoName,
    {
        self.with(TableAlterOption::DropForeignKey(name.into_name()))
    }

    /// Give the table its primary key: `ADD PRIMARY KEY (…)`.
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn add_primary_key<K>(self, key: K) -> TableAlterStatement
    where
        K: IntoTableKey<Primary>,
    {
        self.with(TableAlterOption::AddPrimaryKey(key.into_table_key()))
    }

    /// Add a unique key to the table: `ADD UNIQUE (…)`.
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn add_unique<K>(self, key: K) -> TableAlterStatement
    where
        K: IntoTableKey<Unique>,
    {
        self.with(TableAlterOption::AddUnique(key.into_table_key()))
    }

    /// Recompute a generated column from a new expression:
    /// `ALTER COLUMN "c" SET EXPRESSION AS (<expr>)`. See
    /// [`TableAlterStatement::set_expression`].
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn set_expression<C, E>(self, column: C, expr: E) -> TableAlterStatement
    where
        C: IntoName,
        E: Into<SimpleExpr>,
    {
        self.with(TableAlterOption::SetExpression {
            column: column.into_name(),
            expr: expr.into(),
        })
    }

    /// Make a stored generated column a plain one that keeps its values:
    /// `ALTER COLUMN "c" DROP EXPRESSION`. See
    /// [`TableAlterStatement::drop_expression`].
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn drop_expression<C>(self, column: C) -> TableAlterStatement
    where
        C: IntoName,
    {
        self.with(TableAlterOption::drop_expression(column, false))
    }

    /// `ALTER COLUMN "c" DROP EXPRESSION IF EXISTS`, which leaves a column
    /// that is not generated as it is. See
    /// [`TableAlterStatement::drop_expression_if_exists`].
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn drop_expression_if_exists<C>(self, column: C) -> TableAlterStatement
    where
        C: IntoName,
    {
        self.with(TableAlterOption::drop_expression(column, true))
    }
}

/// Alter a table
///
/// A statement of this type always names a table and always carries at least one
/// action: it is reachable only by choosing an action on a
/// [`PendingTableAlter`], so the `ALTER TABLE "font"` PostgreSQL rejects has no
/// constructor.
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::alter(Font::Table).add_column(
///     ColumnDef::new(Name::runtime("new_col"))
///         .integer()
///         .not_null()
///         .default(100),
/// );
///
/// assert_eq!(
///     table.to_string(),
///     r#"ALTER TABLE "font" ADD COLUMN "new_col" integer NOT NULL DEFAULT 100"#
/// );
/// ```
///
/// There is no `take()`: draining the options would leave the action-less
/// statement this type exists to rule out, so the method a reader would expect
/// to move is absent rather than quietly copying. A second copy is
/// `.to_owned()`.
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// let mut alter = Table::alter(Font::Table).drop_column(Font::Name).to_owned();
/// let moved: TableAlterStatement = alter.take();
/// ```
// [spec:pgorm:req:sql.ddl.alter-table+8]
// [spec:pgorm:req:sql.ast+2]
#[derive(Debug, Clone)]
pub struct TableAlterStatement {
    pub(crate) table: TableName,
    pub(crate) options: Vec<TableAlterOption>,
}

/// table alter add column options
#[derive(Debug, Clone)]
pub struct AddColumnOption {
    pub(crate) column: ColumnDef,
    pub(crate) if_not_exists: bool,
}

/// All available table alter options
///
/// `RENAME` is absent: PostgreSQL takes it only as the sole action of a
/// statement, so it lives in [`ColumnRenameStatement`](crate::ColumnRenameStatement)
/// where it cannot be
/// listed beside anything else.
// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
// [spec:pgorm:req:sql.ddl.alter-table+8]
#[derive(Debug, Clone)]
pub enum TableAlterOption {
    AddColumn(AddColumnOption),
    ModifyColumn(ColumnDef),
    DropColumn(Name),
    AddForeignKey(TableForeignKey),
    DropForeignKey(Name),
    /// `ADD [CONSTRAINT "name"] PRIMARY KEY (…)`. The table may already have
    /// one, which only the server knows: it refuses a second (`42P16`).
    AddPrimaryKey(TableKey<Primary>),
    /// `ADD [CONSTRAINT "name"] UNIQUE [NULLS NOT DISTINCT] (…)`.
    AddUnique(TableKey<Unique>),
    /// `ALTER COLUMN "c" SET EXPRESSION AS (<expr>)`: a generated column's new
    /// expression. Whether the column is generated is the server's knowledge;
    /// it refuses one that is not (`55000`).
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    SetExpression {
        column: Name,
        expr: SimpleExpr,
    },
    /// `ALTER COLUMN "c" DROP EXPRESSION[ IF EXISTS]`: a stored generated
    /// column made plain, keeping its values.
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    DropExpression {
        column: Name,
        if_exists: bool,
    },
}

impl TableAlterOption {
    fn add_column<C: IntoColumnDef>(column_def: C, if_not_exists: bool) -> Self {
        Self::AddColumn(AddColumnOption {
            column: column_def.into_column_def(),
            if_not_exists,
        })
    }

    fn drop_expression<C: IntoName>(column: C, if_exists: bool) -> Self {
        Self::DropExpression {
            column: column.into_name(),
            if_exists,
        }
    }
}

impl TableAlterStatement {
    /// Add a column to an existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Font::Table)
    ///     .drop_column(Name::runtime("old_col"))
    ///     .add_column(
    ///         ColumnDef::new(Name::runtime("new_col"))
    ///             .integer()
    ///             .not_null()
    ///             .default(100),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "font" DROP COLUMN "old_col","#,
    ///         r#"ADD COLUMN "new_col" integer NOT NULL DEFAULT 100"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn add_column<C: IntoColumnDef>(&mut self, column_def: C) -> &mut Self {
        self.add_alter_option(TableAlterOption::add_column(column_def, false))
    }

    /// Try add a column to an existing table if it does not exists
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Font::Table).add_column_if_not_exists(
    ///     ColumnDef::new(Name::runtime("new_col"))
    ///         .integer()
    ///         .not_null()
    ///         .default(100),
    /// );
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     r#"ALTER TABLE "font" ADD COLUMN IF NOT EXISTS "new_col" integer NOT NULL DEFAULT 100"#
    /// );
    /// ```
    pub fn add_column_if_not_exists<C: IntoColumnDef>(&mut self, column_def: C) -> &mut Self {
        self.add_alter_option(TableAlterOption::add_column(column_def, true))
    }

    /// Modify a column in an existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Font::Table).modify_column(
    ///     ColumnDef::new(Name::runtime("new_col"))
    ///         .big_integer()
    ///         .default(999),
    /// );
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "font""#,
    ///         r#"ALTER COLUMN "new_col" TYPE bigint,"#,
    ///         r#"ALTER COLUMN "new_col" SET DEFAULT 999"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn modify_column<C: IntoColumnDef>(&mut self, column_def: C) -> &mut Self {
        self.add_alter_option(TableAlterOption::ModifyColumn(column_def.into_column_def()))
    }

    /// Drop a column from an existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Font::Table).drop_column(Name::runtime("new_column"));
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     r#"ALTER TABLE "font" DROP COLUMN "new_column""#
    /// );
    /// ```
    pub fn drop_column<T>(&mut self, col_name: T) -> &mut Self
    where
        T: IntoName,
    {
        self.add_alter_option(TableAlterOption::DropColumn(col_name.into_name()))
    }

    /// Add a foreign key to existing table
    ///
    /// The key is consumed, as [`add_column`](Self::add_column) consumes its
    /// column: pass an owned value, and `.to_owned()` a binding you mean to
    /// reuse.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let foreign_key_char =
    ///     TableForeignKey::new(Char::Table, Char::FontId, Glyph::Table, Char::FontId)
    ///         .name(Name::runtime("FK_character_glyph"))
    ///         .col(Char::Id, Char::Id)
    ///         .on_delete(ForeignKeyAction::Cascade)
    ///         .on_update(ForeignKeyAction::Cascade)
    ///         .to_owned();
    ///
    /// let foreign_key_font = TableForeignKey::new(Char::Table, Char::FontId, Font::Table, Font::Id)
    ///     .name(Name::runtime("FK_character_font"))
    ///     .on_delete(ForeignKeyAction::Cascade)
    ///     .on_update(ForeignKeyAction::Cascade)
    ///     .to_owned();
    ///
    /// let table = Table::alter(Character::Table)
    ///     .add_foreign_key(foreign_key_char)
    ///     .add_foreign_key(foreign_key_font)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "character""#,
    ///         r#"ADD CONSTRAINT "FK_character_glyph""#,
    ///         r#"FOREIGN KEY ("font_id", "id") REFERENCES "glyph" ("font_id", "id")"#,
    ///         r#"ON DELETE CASCADE ON UPDATE CASCADE,"#,
    ///         r#"ADD CONSTRAINT "FK_character_font""#,
    ///         r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
    ///         r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn add_foreign_key<F>(&mut self, foreign_key: F) -> &mut Self
    where
        F: Into<TableForeignKey>,
    {
        self.add_alter_option(TableAlterOption::AddForeignKey(foreign_key.into()))
    }

    /// Drop a foreign key from existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Character::Table)
    ///     .drop_foreign_key(Name::runtime("FK_character_glyph"))
    ///     .drop_foreign_key(Name::runtime("FK_character_font"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "character""#,
    ///         r#"DROP CONSTRAINT "FK_character_glyph","#,
    ///         r#"DROP CONSTRAINT "FK_character_font""#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn drop_foreign_key<T>(&mut self, name: T) -> &mut Self
    where
        T: IntoName,
    {
        self.add_alter_option(TableAlterOption::DropForeignKey(name.into_name()))
    }

    /// Give the table its primary key: `ADD PRIMARY KEY (…)`, the key the
    /// table declares when it is created
    /// ([`TableCreateStatement::primary_key`](crate::TableCreateStatement::primary_key)).
    ///
    /// Whether the table has a key already is the server's knowledge, not the
    /// builder's, so a second is refused there (`42P16`).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Glyph::Table)
    ///     .add_column(ColumnDef::new(Glyph::Aspect).integer().not_null())
    ///     .add_primary_key(TableKey::new(Glyph::Id).col(Glyph::Aspect).name(Name::runtime("glyph_pk")))
    ///     .add_unique(TableKey::new(Glyph::Image).nulls_not_distinct())
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "glyph" ADD COLUMN "aspect" integer NOT NULL,"#,
    ///         r#"ADD CONSTRAINT "glyph_pk" PRIMARY KEY ("id", "aspect"),"#,
    ///         r#"ADD UNIQUE NULLS NOT DISTINCT ("image")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn add_primary_key<K>(&mut self, key: K) -> &mut Self
    where
        K: IntoTableKey<Primary>,
    {
        self.add_alter_option(TableAlterOption::AddPrimaryKey(key.into_table_key()))
    }

    /// Add a unique key to the table: `ADD UNIQUE (…)`, the key a table
    /// declares with [`TableCreateStatement::unique`](crate::TableCreateStatement::unique).
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn add_unique<K>(&mut self, key: K) -> &mut Self
    where
        K: IntoTableKey<Unique>,
    {
        self.add_alter_option(TableAlterOption::AddUnique(key.into_table_key()))
    }

    /// Recompute a generated column from a new expression:
    /// `ALTER COLUMN "c" SET EXPRESSION AS (<expr>)`.
    ///
    /// The column keeps its kind and every existing row takes the new value.
    /// A [stored](crate::GeneratedKind::Stored) column is rewritten to hold
    /// it, a [virtual](crate::GeneratedKind::Virtual) one is not, since it
    /// computes on read. The expression is held to what a generated column's
    /// is (immutable, reading no other generated column: `42P17`), and a
    /// column that is not generated, an identity included, is refused
    /// (`55000`). So is a virtual column whose table has a `CHECK` constraint
    /// or belongs to a publication (`0A000`).
    ///
    /// This is the one way to change a generated column's expression:
    /// [`modify_column`](Self::modify_column) writes no
    /// [`generated`](crate::ColumnDef::generated) spec, since a kind it
    /// carried could not be honoured.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Glyph::Table)
    ///     .set_expression(Glyph::Aspect, Expr::col(Glyph::Id).mul(3))
    ///     .drop_expression(Glyph::Tokens)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"ALTER TABLE "glyph""#,
    ///         r#"ALTER COLUMN "aspect" SET EXPRESSION AS ("id" * 3),"#,
    ///         r#"ALTER COLUMN "tokens" DROP EXPRESSION"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn set_expression<C, E>(&mut self, column: C, expr: E) -> &mut Self
    where
        C: IntoName,
        E: Into<SimpleExpr>,
    {
        self.add_alter_option(TableAlterOption::SetExpression {
            column: column.into_name(),
            expr: expr.into(),
        })
    }

    /// Make a stored generated column a plain one:
    /// `ALTER COLUMN "c" DROP EXPRESSION`.
    ///
    /// Every row keeps the value it was last computed to, and from then on
    /// the column is written like any other. PostgreSQL refuses it for a
    /// [virtual](crate::GeneratedKind::Virtual) column, which has no stored
    /// values to keep (`0A000`), and for a column that is not generated
    /// (`55000`) — unless the action says
    /// [`IF EXISTS`](Self::drop_expression_if_exists).
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn drop_expression<C>(&mut self, column: C) -> &mut Self
    where
        C: IntoName,
    {
        self.add_alter_option(TableAlterOption::drop_expression(column, false))
    }

    /// `ALTER COLUMN "c" DROP EXPRESSION IF EXISTS`: as
    /// [`drop_expression`](Self::drop_expression), except that a column that
    /// is not generated is left as it is, with a notice, rather than refused.
    /// A virtual column is refused either way (`0A000`).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// assert_eq!(
    ///     Table::alter(Glyph::Table)
    ///         .drop_expression_if_exists(Glyph::Aspect)
    ///         .to_string(),
    ///     r#"ALTER TABLE "glyph" ALTER COLUMN "aspect" DROP EXPRESSION IF EXISTS"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ddl.alter-table+8]
    pub fn drop_expression_if_exists<C>(&mut self, column: C) -> &mut Self
    where
        C: IntoName,
    {
        self.add_alter_option(TableAlterOption::drop_expression(column, true))
    }

    fn add_alter_option(&mut self, alter_option: TableAlterOption) -> &mut Self {
        self.options.push(alter_option);
        self
    }
}

/// Renders the statement with every value inlined as an escaped SQL literal.
/// This is its only rendering: it exposes no placeholder-emitting build, so
/// nothing here is left to bind.
// [spec:pgorm:req:sql.ddl+8]
impl std::fmt::Display for TableAlterStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_table_alter_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
