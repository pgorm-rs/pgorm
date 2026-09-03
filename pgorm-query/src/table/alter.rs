use crate::{
    ColumnDef, IntoColumnDef, SchemaStatementBuilder, TableForeignKey, backend::QueryBuilder,
    types::*,
};
use inherent::inherent;

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
/// Table::alter(Font::Table).to_string(QueryBuilder);
/// ```
///
/// [`Table::alter`]: crate::Table::alter
// [spec:pgorm:req:sql.ddl.alter-table+2]
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
        T: IntoIden,
    {
        self.with(TableAlterOption::DropColumn(col_name.into_iden()))
    }

    /// Add a foreign key to existing table
    pub fn add_foreign_key(self, foreign_key: &TableForeignKey) -> TableAlterStatement {
        self.with(TableAlterOption::AddForeignKey(foreign_key.to_owned()))
    }

    /// Drop a foreign key from existing table
    pub fn drop_foreign_key<T>(self, name: T) -> TableAlterStatement
    where
        T: IntoIden,
    {
        self.with(TableAlterOption::DropForeignKey(name.into_iden()))
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
///     ColumnDef::new(Alias::new("new_col"))
///         .integer()
///         .not_null()
///         .default(100),
/// );
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
///     r#"ALTER TABLE "font" ADD COLUMN "new_col" integer NOT NULL DEFAULT 100"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.alter-table+2]
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
/// statement, so it lives in [`ColumnRenameStatement`] where it cannot be
/// listed beside anything else.
// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
// [spec:pgorm:req:sql.ddl.alter-table+2]
#[derive(Debug, Clone)]
pub enum TableAlterOption {
    AddColumn(AddColumnOption),
    ModifyColumn(ColumnDef),
    DropColumn(DynIden),
    AddForeignKey(TableForeignKey),
    DropForeignKey(DynIden),
}

impl TableAlterOption {
    fn add_column<C: IntoColumnDef>(column_def: C, if_not_exists: bool) -> Self {
        Self::AddColumn(AddColumnOption {
            column: column_def.into_column_def(),
            if_not_exists,
        })
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
    ///     .drop_column(Alias::new("old_col"))
    ///     .add_column(
    ///         ColumnDef::new(Alias::new("new_col"))
    ///             .integer()
    ///             .not_null()
    ///             .default(100),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
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
    ///     ColumnDef::new(Alias::new("new_col"))
    ///         .integer()
    ///         .not_null()
    ///         .default(100),
    /// );
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
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
    ///     ColumnDef::new(Alias::new("new_col"))
    ///         .big_integer()
    ///         .default(999),
    /// );
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
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
    /// let table = Table::alter(Font::Table).drop_column(Alias::new("new_column"));
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
    ///     r#"ALTER TABLE "font" DROP COLUMN "new_column""#
    /// );
    /// ```
    pub fn drop_column<T>(&mut self, col_name: T) -> &mut Self
    where
        T: IntoIden,
    {
        self.add_alter_option(TableAlterOption::DropColumn(col_name.into_iden()))
    }

    /// Add a foreign key to existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let foreign_key_char = TableForeignKey::new()
    ///     .name("FK_character_glyph")
    ///     .from_tbl(Char::Table)
    ///     .from_col(Char::FontId)
    ///     .from_col(Char::Id)
    ///     .to_tbl(Glyph::Table)
    ///     .to_col(Char::FontId)
    ///     .to_col(Char::Id)
    ///     .on_delete(ForeignKeyAction::Cascade)
    ///     .on_update(ForeignKeyAction::Cascade)
    ///     .to_owned();
    ///
    /// let foreign_key_font = TableForeignKey::new()
    ///     .name("FK_character_font")
    ///     .from_tbl(Char::Table)
    ///     .from_col(Char::FontId)
    ///     .to_tbl(Font::Table)
    ///     .to_col(Font::Id)
    ///     .on_delete(ForeignKeyAction::Cascade)
    ///     .on_update(ForeignKeyAction::Cascade)
    ///     .to_owned();
    ///
    /// let table = Table::alter(Character::Table)
    ///     .add_foreign_key(&foreign_key_char)
    ///     .add_foreign_key(&foreign_key_font)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
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
    pub fn add_foreign_key(&mut self, foreign_key: &TableForeignKey) -> &mut Self {
        self.add_alter_option(TableAlterOption::AddForeignKey(foreign_key.to_owned()))
    }

    /// Drop a foreign key from existing table
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let table = Table::alter(Character::Table)
    ///     .drop_foreign_key(Alias::new("FK_character_glyph"))
    ///     .drop_foreign_key(Alias::new("FK_character_font"))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(QueryBuilder),
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
        T: IntoIden,
    {
        self.add_alter_option(TableAlterOption::DropForeignKey(name.into_iden()))
    }

    fn add_alter_option(&mut self, alter_option: TableAlterOption) -> &mut Self {
        self.options.push(alter_option);
        self
    }

    /// Clone this statement out of a builder chain.
    ///
    /// Unlike the other DDL builders this copies rather than moves: moving the
    /// options out would leave an action-less `ALTER TABLE` behind, which is the
    /// very state this type exists to rule out.
    pub fn take(&mut self) -> Self {
        self.clone()
    }
}

#[inherent]
impl SchemaStatementBuilder for TableAlterStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_alter_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_alter_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
