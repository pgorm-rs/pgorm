use crate::{
    ColumnDef, IntoColumnDef, QueryBuilder, SimpleExpr, foreign_key::*, index::*, types::*,
};

/// Create a table
///
/// The table is taken by the constructor: `CREATE TABLE` has no spelling without
/// one, so a statement that never names its target does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::create().col(ColumnDef::new(Glyph::Id).integer());
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let table = Table::create(Char::Table)
///     .if_not_exists()
///     .comment("table's comment")
///     .col(ColumnDef::new(Char::Id).integer().not_null().auto_increment().primary_key())
///     .col(ColumnDef::new(Char::FontSize).integer().not_null().comment("font's size"))
///     .col(ColumnDef::new(Char::Character).string().not_null())
///     .col(ColumnDef::new(Char::SizeW).integer().not_null())
///     .col(ColumnDef::new(Char::SizeH).integer().not_null())
///     .col(ColumnDef::new(Char::FontId).integer().default(Value::Int(None)))
///     .foreign_key(
///         ForeignKey::create(Char::Table, Char::FontId, Font::Table, Font::Id)
///             .name("FK_2e303c3a712662f1fc2a4d0aad6")
///             .on_delete(ForeignKeyAction::Cascade)
///             .on_update(ForeignKeyAction::Cascade)
///     )
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(),
///     [
///         r#"CREATE TABLE IF NOT EXISTS "character" ("#,
///             r#""id" serial NOT NULL PRIMARY KEY,"#,
///             r#""font_size" integer NOT NULL,"#,
///             r#""character" varchar NOT NULL,"#,
///             r#""size_w" integer NOT NULL,"#,
///             r#""size_h" integer NOT NULL,"#,
///             r#""font_id" integer DEFAULT NULL,"#,
///             r#"CONSTRAINT "FK_2e303c3a712662f1fc2a4d0aad6""#,
///                 r#"FOREIGN KEY ("font_id") REFERENCES "font" ("id")"#,
///                 r#"ON DELETE CASCADE ON UPDATE CASCADE"#,
///         r#")"#,
///     ].join(" ")
/// );
/// ```
// [spec:pgorm:req:sql.ddl.create-table+6]
#[derive(Debug, Clone)]
pub struct TableCreateStatement {
    pub(crate) table: TableName,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) indexes: Vec<IndexCreateStatement>,
    pub(crate) foreign_keys: Vec<ForeignKeyCreateStatement>,
    pub(crate) if_not_exists: bool,
    pub(crate) check: Vec<SimpleExpr>,
    pub(crate) comment: Option<String>,
    pub(crate) extra: Option<String>,
}

impl TableCreateStatement {
    /// Construct create table statement over the table it creates
    pub fn new<T>(table: T) -> Self
    where
        T: IntoTableName,
    {
        Self {
            table: table.into_table_name(),
            columns: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            if_not_exists: false,
            check: Vec::new(),
            comment: None,
            extra: None,
        }
    }

    /// Create table if table not exists
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// Set table comment
    pub fn comment<T>(&mut self, comment: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.comment = Some(comment.into());
        self
    }

    /// Add a new table column
    pub fn col<C: IntoColumnDef>(&mut self, column: C) -> &mut Self {
        let mut column = column.into_column_def();
        column.table = Some(self.table.clone());
        self.columns.push(column);
        self
    }

    pub fn check(&mut self, value: SimpleExpr) -> &mut Self {
        self.check.push(value);
        self
    }

    /// Add a table-level index expression to the create statement
    ///
    /// The index is restamped onto this statement's table, as `col()` restamps
    /// each column: an embedded index constrains the table it sits inside and
    /// cannot name another.
    pub fn index(&mut self, index: &mut IndexCreateStatement) -> &mut Self {
        let mut index = index.take();
        index.table = self.table.clone();
        self.indexes.push(index);
        self
    }

    /// Add an primary key.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let mut statement = Table::create(Glyph::Table);
    /// statement
    ///     .col(ColumnDef::new(Glyph::Id).integer().not_null())
    ///     .col(ColumnDef::new(Glyph::Image).string().not_null())
    ///     .primary_key(Index::create(Glyph::Table, Glyph::Id).col(Glyph::Image));
    ///
    /// assert_eq!(
    ///     statement.to_string(),
    ///     [
    ///         r#"CREATE TABLE "glyph" ("#,
    ///         r#""id" integer NOT NULL,"#,
    ///         r#""image" varchar NOT NULL,"#,
    ///         r#"PRIMARY KEY ("id", "image")"#,
    ///         r#")"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn primary_key(&mut self, index: &mut IndexCreateStatement) -> &mut Self {
        let mut index = index.take();
        index.kind = IndexKind::PrimaryKey;
        index.table = self.table.clone();
        self.indexes.push(index);
        self
    }

    /// Add a foreign key
    ///
    /// The key is restamped onto this statement's table, as `col()` and
    /// `index()` restamp each column and index: an embedded key constrains the
    /// table it sits inside and cannot name another.
    pub fn foreign_key(&mut self, foreign_key: &mut ForeignKeyCreateStatement) -> &mut Self {
        let mut foreign_key = foreign_key.take();
        foreign_key.foreign_key.retarget(self.table.clone());
        self.foreign_keys.push(foreign_key);
        self
    }

    pub fn get_table_name(&self) -> &TableName {
        &self.table
    }

    pub fn get_columns(&self) -> &Vec<ColumnDef> {
        self.columns.as_ref()
    }

    pub fn get_comment(&self) -> Option<&String> {
        self.comment.as_ref()
    }

    pub fn get_foreign_key_create_stmts(&self) -> &Vec<ForeignKeyCreateStatement> {
        self.foreign_keys.as_ref()
    }

    pub fn get_indexes(&self) -> &Vec<IndexCreateStatement> {
        self.indexes.as_ref()
    }

    /// Rewriting extra param. You should take care self about concat extra params. Add extra after options.
    /// Example for PostgresSQL [Citus](https://github.com/citusdata/citus) extension:
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// let table = Table::create(Char::Table)
    ///     .col(
    ///         ColumnDef::new(Char::Id)
    ///             .uuid()
    ///             .extra("DEFAULT uuid_generate_v4()")
    ///             .primary_key()
    ///             .not_null(),
    ///     )
    ///     .col(
    ///         ColumnDef::new(Char::CreatedAt)
    ///             .timestamp_with_time_zone()
    ///             .extra("DEFAULT NOW()")
    ///             .not_null(),
    ///     )
    ///     .col(ColumnDef::new(Char::UserData).json_binary().not_null())
    ///     .extra("USING columnar")
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     table.to_string(),
    ///     [
    ///         r#"CREATE TABLE "character" ("#,
    ///         r#""id" uuid DEFAULT uuid_generate_v4() PRIMARY KEY NOT NULL,"#,
    ///         r#""created_at" timestamp with time zone DEFAULT NOW() NOT NULL,"#,
    ///         r#""user_data" jsonb NOT NULL"#,
    ///         r#") USING columnar"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn extra<T>(&mut self, extra: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.extra = Some(extra.into());
        self
    }

    pub fn get_extra(&self) -> Option<&String> {
        self.extra.as_ref()
    }

    /// Clone this statement out of a builder chain.
    ///
    /// The table is copied rather than moved: moving it out would leave the
    /// targetless statement this type exists to rule out.
    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.clone(),
            columns: std::mem::take(&mut self.columns),
            indexes: std::mem::take(&mut self.indexes),
            foreign_keys: std::mem::take(&mut self.foreign_keys),
            if_not_exists: self.if_not_exists,
            check: std::mem::take(&mut self.check),
            comment: std::mem::take(&mut self.comment),
            extra: std::mem::take(&mut self.extra),
        }
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for TableCreateStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_table_create_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
