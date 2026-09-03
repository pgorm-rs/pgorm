use inherent::inherent;

use crate::{
    ColumnDef, IntoColumnDef, QueryBuilder, SchemaStatementBuilder, SimpleExpr, foreign_key::*,
    index::*, types::*,
};

/// Create a table
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let table = Table::create()
///     .table(Char::Table)
///     .if_not_exists()
///     .comment("table's comment")
///     .col(ColumnDef::new(Char::Id).integer().not_null().auto_increment().primary_key())
///     .col(ColumnDef::new(Char::FontSize).integer().not_null().comment("font's size"))
///     .col(ColumnDef::new(Char::Character).string().not_null())
///     .col(ColumnDef::new(Char::SizeW).integer().not_null())
///     .col(ColumnDef::new(Char::SizeH).integer().not_null())
///     .col(ColumnDef::new(Char::FontId).integer().default(Value::Int(None)))
///     .foreign_key(
///         ForeignKey::create()
///             .name("FK_2e303c3a712662f1fc2a4d0aad6")
///             .from(Char::Table, Char::FontId)
///             .to(Font::Table, Font::Id)
///             .on_delete(ForeignKeyAction::Cascade)
///             .on_update(ForeignKeyAction::Cascade)
///     )
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
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
// [spec:pgorm:req:sql.ddl.create-table+5]
#[derive(Default, Debug, Clone)]
pub struct TableCreateStatement {
    pub(crate) table: Option<TableName>,
    pub(crate) columns: Vec<ColumnDef>,
    pub(crate) indexes: Vec<IndexCreateStatement>,
    pub(crate) foreign_keys: Vec<ForeignKeyCreateStatement>,
    pub(crate) if_not_exists: bool,
    pub(crate) check: Vec<SimpleExpr>,
    pub(crate) comment: Option<String>,
    pub(crate) extra: Option<String>,
}

impl TableCreateStatement {
    /// Construct create table statement
    pub fn new() -> Self {
        Self::default()
    }

    /// Create table if table not exists
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// Set table name
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableName,
    {
        self.table = Some(table.into_table_name());
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
        column.table.clone_from(&self.table);
        self.columns.push(column);
        self
    }

    pub fn check(&mut self, value: SimpleExpr) -> &mut Self {
        self.check.push(value);
        self
    }

    /// Add a table-level index expression to the create statement
    pub fn index(&mut self, index: &mut IndexCreateStatement) -> &mut Self {
        self.indexes.push(index.take());
        self
    }

    /// Add an primary key.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let mut statement = Table::create();
    /// statement
    ///     .table(Glyph::Table)
    ///     .col(ColumnDef::new(Glyph::Id).integer().not_null())
    ///     .col(ColumnDef::new(Glyph::Image).string().not_null())
    ///     .primary_key(Index::create(Glyph::Id).col(Glyph::Image));
    ///
    /// assert_eq!(
    ///     statement.to_string(QueryBuilder),
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
        self.indexes.push(index);
        self
    }

    /// Add a foreign key
    pub fn foreign_key(&mut self, foreign_key: &mut ForeignKeyCreateStatement) -> &mut Self {
        self.foreign_keys.push(foreign_key.take());
        self
    }

    pub fn get_table_name(&self) -> Option<&TableName> {
        self.table.as_ref()
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
    /// let table = Table::create()
    ///     .table(Char::Table)
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
    ///     table.to_string(QueryBuilder),
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

    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.take(),
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

#[inherent]
impl SchemaStatementBuilder for TableCreateStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_create_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_create_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
