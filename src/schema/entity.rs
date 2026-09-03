use crate::{
    ActiveEnum, ColumnTrait, ColumnType, EntityTrait, Error, Iterable, PrimaryKeyArity,
    PrimaryKeyToColumn, PrimaryKeyTrait, RelationTrait, Schema,
};
use pgorm_query::{
    ColumnDef, Comment, CommentStatement, Iden, Index, IndexCreateStatement, SeaRc,
    TableCreateStatement,
    extension::{Type, TypeCreateStatement},
};

impl Schema {
    /// Creates Postgres enums from an ActiveEnum. See [TypeCreateStatement] for more details.
    ///
    /// An `ActiveEnum` may be backed by a plain column type — a `String` column,
    /// say — rather than a database enum, in which case there is no type to
    /// create and this returns [`Error::Type`].
    pub fn create_enum_from_active_enum<A>(&self) -> Result<TypeCreateStatement, Error>
    where
        A: ActiveEnum,
    {
        create_enum_from_active_enum::<A>()
    }

    /// Creates Postgres enums from an Entity. See [TypeCreateStatement] for more details
    pub fn create_enum_from_entity<E>(&self, entity: E) -> Vec<TypeCreateStatement>
    where
        E: EntityTrait,
    {
        create_enum_from_entity(entity)
    }

    /// Creates a table from an Entity. See [TableCreateStatement] for more details.
    pub fn create_table_from_entity<E>(&self, entity: E) -> TableCreateStatement
    where
        E: EntityTrait,
    {
        create_table_from_entity(entity)
    }

    /// Creates the indexes from an Entity, returning an empty Vec if there are none
    /// to create. See [IndexCreateStatement] for more details
    pub fn create_index_from_entity<E>(&self, entity: E) -> Vec<IndexCreateStatement>
    where
        E: EntityTrait,
    {
        create_index_from_entity(entity)
    }

    /// Creates the comments from an Entity, returning an empty Vec if neither the
    /// entity nor any of its columns declares one. A comment is a statement of its
    /// own in Postgres, so these are executed alongside — not as part of — the
    /// statement from [`Schema::create_table_from_entity`].
    /// See [CommentStatement] for more details.
    ///
    /// ```
    /// use crate::pgorm::IdenStatic;
    /// use pgorm::{
    ///     ActiveModelBehavior, ColumnDef, ColumnTrait, ColumnType, EntityName, EntityTrait,
    ///     EnumIter, PrimaryKeyTrait, RelationDef, RelationTrait, Schema,
    /// };
    /// use pgorm_macros::{DeriveEntityModel, DerivePrimaryKey};
    ///
    /// #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    /// #[pgorm(table_name = "posts", comment = "one row per post")]
    /// pub struct Model {
    ///     #[pgorm(primary_key)]
    ///     pub id: i32,
    ///     #[pgorm(comment = "the author's title")]
    ///     pub title: String,
    /// }
    ///
    /// #[derive(Copy, Clone, Debug, EnumIter)]
    /// pub enum Relation {}
    ///
    /// impl RelationTrait for Relation {
    ///     fn def(&self) -> RelationDef {
    ///         panic!("No RelationDef")
    ///     }
    /// }
    /// impl ActiveModelBehavior for ActiveModel {}
    ///
    /// let schema = Schema::new();
    /// let comments: Vec<String> = schema
    ///     .create_comments_from_entity(Entity)
    ///     .iter()
    ///     .map(|stmt| stmt.to_string())
    ///     .collect();
    ///
    /// assert_eq!(
    ///     comments,
    ///     [
    ///         r#"COMMENT ON TABLE "posts" IS 'one row per post'"#,
    ///         r#"COMMENT ON COLUMN "posts"."title" IS 'the author''s title'"#,
    ///     ]
    /// );
    /// ```
    pub fn create_comments_from_entity<E>(&self, entity: E) -> Vec<CommentStatement>
    where
        E: EntityTrait,
    {
        create_comments_from_entity(entity)
    }

    /// Creates a column definition for example to update a table.
    ///
    /// ```
    /// use crate::pgorm::IdenStatic;
    /// use pgorm::{
    ///     ActiveModelBehavior, ColumnDef, ColumnTrait, ColumnType, EntityName, EntityTrait,
    ///     EnumIter, PrimaryKeyTrait, RelationDef, RelationTrait, Schema,
    /// };
    /// use pgorm_macros::{DeriveEntityModel, DerivePrimaryKey};
    /// use pgorm_query::Table;
    ///
    /// #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    /// #[pgorm(table_name = "posts")]
    /// pub struct Model {
    ///     #[pgorm(primary_key)]
    ///     pub id: i32,
    ///     pub title: String,
    /// }
    ///
    /// #[derive(Copy, Clone, Debug, EnumIter)]
    /// pub enum Relation {}
    ///
    /// impl RelationTrait for Relation {
    ///     fn def(&self) -> RelationDef {
    ///         panic!("No RelationDef")
    ///     }
    /// }
    /// impl ActiveModelBehavior for ActiveModel {}
    ///
    /// let schema = Schema::new();
    ///
    /// let alter_table = Table::alter(Entity)
    ///     .add_column(&mut schema.get_column_def::<Entity>(Column::Title));
    /// assert_eq!(
    ///     alter_table.to_string(),
    ///     r#"ALTER TABLE "posts" ADD COLUMN "title" varchar NOT NULL"#
    /// );
    /// ```
    pub fn get_column_def<E>(&self, column: E::Column) -> ColumnDef
    where
        E: EntityTrait,
    {
        column_def_from_entity_column::<E>(column)
    }
}

// [spec:pgorm:sem:schema.from-entity.enum+2]    from a single ActiveEnum
pub(crate) fn create_enum_from_active_enum<A>() -> Result<TypeCreateStatement, Error>
where
    A: ActiveEnum,
{
    let col_def = A::db_type();
    create_enum_from_column_type(col_def.get_column_type()).ok_or_else(|| {
        Error::Type(format!(
            "`{}` is not backed by a database enum, so there is no type to create",
            A::name().to_string()
        ))
    })
}

pub(crate) fn create_enum_from_column_type(col_type: &ColumnType) -> Option<TypeCreateStatement> {
    let ColumnType::Enum { name, variants } = col_type else {
        return None;
    };
    Some(
        Type::create()
            .as_enum(name.clone())
            .values(variants.clone())
            .to_owned(),
    )
}

// [spec:pgorm:sem:schema.from-entity.enum+2]
pub(crate) fn create_enum_from_entity<E>(_: E) -> Vec<TypeCreateStatement>
where
    E: EntityTrait,
{
    let mut vec = Vec::new();
    for col in E::Column::iter() {
        let col_def = col.def();
        vec.extend(create_enum_from_column_type(col_def.get_column_type()));
    }
    vec
}

// [spec:pgorm:sem:schema.from-entity.index+1]
pub(crate) fn create_index_from_entity<E>(entity: E) -> Vec<IndexCreateStatement>
where
    E: EntityTrait,
{
    let mut vec = Vec::new();
    for column in E::Column::iter() {
        let column_def = column.def();
        if !column_def.indexed {
            continue;
        }
        let name = format!("idx-{}-{}", entity.to_string(), column.to_string());
        let stmt = Index::create(entity.table_ref(), column)
            .name(name)
            .to_owned();
        vec.push(stmt)
    }
    vec
}

// [spec:pgorm:sem:schema.from-entity+2]    the comment statements, one stream per entity
pub(crate) fn create_comments_from_entity<E>(entity: E) -> Vec<CommentStatement>
where
    E: EntityTrait,
{
    let table = entity.table_ref();

    let mut vec = Vec::new();
    if let Some(comment) = entity.comment() {
        vec.push(Comment::on_table(table.clone(), comment));
    }
    for column in E::Column::iter() {
        if let Some(comment) = column.def().comment {
            vec.push(Comment::on_column(table.clone(), column, comment));
        }
    }
    vec
}

// [spec:pgorm:sem:schema.from-entity+2]
pub(crate) fn create_table_from_entity<E>(entity: E) -> TableCreateStatement
where
    E: EntityTrait,
{
    let mut stmt = TableCreateStatement::new(entity.table_ref());

    if let Some(comment) = entity.comment() {
        stmt.comment(comment);
    }

    for column in E::Column::iter() {
        let mut column_def = column_def_from_entity_column::<E>(column);
        stmt.col(&mut column_def);
    }

    if <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as PrimaryKeyArity>::ARITY > 1 {
        let mut primary_keys = E::PrimaryKey::iter();
        if let Some(first) = primary_keys.next() {
            let mut idx_pk = Index::create(entity.table_ref(), first);
            for primary_key in primary_keys {
                idx_pk.col(primary_key);
            }
            stmt.primary_key(idx_pk.name(format!("pk-{}", entity.to_string())).primary());
        }
    }

    for relation in E::Relation::iter() {
        let relation = relation.def();
        if relation.is_owner {
            continue;
        }
        stmt.foreign_key(&mut relation.into());
    }

    stmt.take()
}

// [spec:pgorm:sem:schema.from-entity+2]    column + primary-key projection
fn column_def_from_entity_column<E>(column: E::Column) -> ColumnDef
where
    E: EntityTrait,
{
    let orm_column_def = column.def();
    let types = match orm_column_def.col_type {
        ColumnType::Enum { ref name, .. } => ColumnType::Custom(SeaRc::clone(name)),
        _ => orm_column_def.col_type,
    };
    let mut column_def = ColumnDef::new_with_type(column, types);
    if !orm_column_def.null {
        column_def.not_null();
    }
    if orm_column_def.unique {
        column_def.unique_key();
    }
    if let Some(default) = orm_column_def.default {
        column_def.default(default);
    }
    if let Some(comment) = orm_column_def.comment {
        column_def.comment(comment);
    }
    for primary_key in E::PrimaryKey::iter() {
        if column.to_string() == primary_key.into_column().to_string() {
            if E::PrimaryKey::auto_increment() {
                column_def.auto_increment();
            }
            if <<E::PrimaryKey as PrimaryKeyTrait>::ValueType as PrimaryKeyArity>::ARITY == 1 {
                column_def.primary_key();
            }
        }
    }
    column_def
}

#[cfg(test)]
mod tests {
    use crate::{EntityName, Schema, pgorm_query::*, tests_cfg::*};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_create_table_from_entity_table_ref() {
        let schema = Schema::new();
        assert_eq!(
            schema
                .create_table_from_entity(CakeFillingPrice)
                .to_string(),
            get_cake_filling_price_stmt().to_string()
        );
    }

    fn get_cake_filling_price_stmt() -> TableCreateStatement {
        Table::create(CakeFillingPrice.table_ref())
            .col(
                ColumnDef::new(cake_filling_price::Column::CakeId)
                    .integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(cake_filling_price::Column::FillingId)
                    .integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(cake_filling_price::Column::Price)
                    .decimal()
                    .not_null(),
            )
            .primary_key(
                Index::create(
                    CakeFillingPrice.table_ref(),
                    cake_filling_price::Column::CakeId,
                )
                .name("pk-cake_filling_price")
                .col(cake_filling_price::Column::FillingId)
                .primary(),
            )
            .foreign_key(
                ForeignKeyCreateStatement::new()
                    .name("fk-cake_filling_price-cake_id-filling_id")
                    .from_tbl(CakeFillingPrice)
                    .from_col(cake_filling_price::Column::CakeId)
                    .from_col(cake_filling_price::Column::FillingId)
                    .to_tbl(CakeFilling)
                    .to_col(cake_filling::Column::CakeId)
                    .to_col(cake_filling::Column::FillingId),
            )
            .to_owned()
    }

    #[test]
    fn test_create_index_from_entity_table_ref() {
        let schema = Schema::new();

        assert_eq!(
            schema.create_table_from_entity(indexes::Entity).to_string(),
            get_indexes_stmt().to_string()
        );

        let stmts = schema.create_index_from_entity(indexes::Entity);
        assert_eq!(stmts.len(), 2);

        let idx: IndexCreateStatement =
            Index::create(indexes::Entity.table_ref(), indexes::Column::Index1Attr)
                .name("idx-indexes-index1_attr")
                .to_owned();
        assert_eq!(stmts[0].to_string(), idx.to_string());

        let idx: IndexCreateStatement =
            Index::create(indexes::Entity.table_ref(), indexes::Column::Index2Attr)
                .name("idx-indexes-index2_attr")
                .to_owned();
        assert_eq!(stmts[1].to_string(), idx.to_string());
    }

    fn get_indexes_stmt() -> TableCreateStatement {
        Table::create(indexes::Entity.table_ref())
            .col(
                ColumnDef::new(indexes::Column::IndexesId)
                    .integer()
                    .not_null()
                    .auto_increment()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(indexes::Column::UniqueAttr)
                    .integer()
                    .not_null()
                    .unique_key(),
            )
            .col(
                ColumnDef::new(indexes::Column::Index1Attr)
                    .integer()
                    .not_null(),
            )
            .col(
                ColumnDef::new(indexes::Column::Index2Attr)
                    .integer()
                    .not_null()
                    .unique_key(),
            )
            .to_owned()
    }
}
