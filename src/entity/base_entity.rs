use crate::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, DbErr, Delete, DeleteMany, DeleteOne,
    FromQueryResult, Insert, ModelTrait, NoColumns, PrimaryKeyToColumn, PrimaryKeyTrait,
    QueryFilter, Related, RelationBuilder, RelationTrait, RelationType, Select, Update, UpdateMany,
    UpdateOne,
};
use pgorm_query::{Alias, Iden, IntoIden, IntoTableName, IntoValueTuple, TableName};
use std::fmt::Debug;
pub use strum::IntoEnumIterator as Iterable;

/// Ensure the identifier for an Entity can be converted to a static str
// [spec:pgorm:def:entity.traits]
pub trait IdenStatic: Iden + Copy + Debug + 'static {
    /// Method to call to get the static string identity
    fn as_str(&self) -> &str;
}

/// A Trait for mapping an Entity to a database table
// [spec:pgorm:req:entity.traits.entity-name+1]
pub trait EntityName: IdenStatic + Default {
    /// Method to get the name for the schema, defaults to [Option::None] if not set
    fn schema_name(&self) -> Option<&str> {
        None
    }

    /// Method to get the comment for the schema, defaults to [Option::None] if not set
    fn comment(&self) -> Option<&str> {
        None
    }

    /// Get the name of the table
    fn table_name(&self) -> &str;

    /// Get the name of the module from the invoking `self.table_name()`
    fn module_name(&self) -> &str {
        self.table_name()
    }

    /// Get the [TableName] from invoking the `self.schema_name()`
    fn table_ref(&self) -> TableName {
        match self.schema_name() {
            Some(schema) => (Alias::new(schema).into_iden(), self.into_iden()).into_table_name(),
            None => self.into_table_name(),
        }
    }
}

/// An abstract base class for defining Entities.
///
/// This trait provides an API for you to inspect it's properties
/// - Column (implemented [`ColumnTrait`])
/// - Relation (implemented [`RelationTrait`])
/// - Primary Key (implemented [`PrimaryKeyTrait`] and [`PrimaryKeyToColumn`])
///
/// This trait also provides an API for CRUD actions
/// - Select: `find`, `find_*`
/// - Insert: `insert`, `insert_*`
/// - Update: `update`, `update_*`
/// - Delete: `delete`, `delete_*`
// [spec:pgorm:def:entity.traits]
// [spec:pgorm:req:entity.traits.crud+1]
pub trait EntityTrait: EntityName {
    #[allow(missing_docs)]
    type Model: ModelTrait<Entity = Self> + FromQueryResult;

    #[allow(missing_docs)]
    type ActiveModel: ActiveModelBehavior<Entity = Self>;

    #[allow(missing_docs)]
    type Column: ColumnTrait;

    #[allow(missing_docs)]
    type Relation: RelationTrait;

    #[allow(missing_docs)]
    type PrimaryKey: PrimaryKeyTrait + PrimaryKeyToColumn<Column = Self::Column>;

    /// Check if the relation belongs to an Entity
    fn belongs_to<R>(related: R) -> RelationBuilder<Self, R, NoColumns>
    where
        R: EntityTrait,
    {
        RelationBuilder::new(RelationType::HasOne, Self::default(), related, false)
    }

    /// Check if the entity has at least one relation
    fn has_one<R>(_: R) -> RelationBuilder<Self, R>
    where
        R: EntityTrait + Related<Self>,
    {
        RelationBuilder::from_rel(RelationType::HasOne, R::to().rev(), true)
    }

    /// Check if the Entity has many relations
    fn has_many<R>(_: R) -> RelationBuilder<Self, R>
    where
        R: EntityTrait + Related<Self>,
    {
        RelationBuilder::from_rel(RelationType::HasMany, R::to().rev(), true)
    }

    /// Construct select statement to find one / all models
    ///
    /// - To select columns, join tables and group by expressions, see [`QuerySelect`](crate::query::QuerySelect)
    /// - To apply where conditions / filters, see [`QueryFilter`](crate::query::QueryFilter)
    /// - To apply order by expressions, see [`QueryOrder`](crate::query::QueryOrder)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// // `one` appends `LIMIT 1` and fails with `DbErr::RecordNotFound` when no
    /// // row matches; `one_opt` returns `None` in that case instead.
    /// let cake: cake::Model = cake::Entity::find().one(&db).await?;
    /// let maybe_cake: Option<cake::Model> = cake::Entity::find().one_opt(&db).await?;
    ///
    /// let cakes: Vec<cake::Model> = cake::Entity::find().all(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// The statement selects every column of the entity:
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find().build().0,
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake""#
    /// );
    /// ```
    fn find() -> Select<Self> {
        Select::new()
    }

    /// Find a model by primary key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let sponge_cake: cake::Model = cake::Entity::find_by_id(11).one(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find_by_id(11).build().0,
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = $1"#
    /// );
    /// ```
    /// Find by composite key
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake_filling};
    ///
    /// assert_eq!(
    ///     cake_filling::Entity::find_by_id((2, 3)).build().0,
    ///     [
    ///         r#"SELECT "cake_filling"."cake_id", "cake_filling"."filling_id" FROM "cake_filling""#,
    ///         r#"WHERE "cake_filling"."cake_id" = $1 AND "cake_filling"."filling_id" = $2"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if arity of input values don't match arity of primary key
    // [spec:pgorm:req:entity.traits.crud+1]
    fn find_by_id<T>(values: T) -> Select<Self>
    where
        T: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        let mut select = Self::find();
        let mut keys = Self::PrimaryKey::iter();
        for v in values.into().into_value_tuple() {
            if let Some(key) = keys.next() {
                let col = key.into_column();
                select = select.filter(col.eq(v));
            } else {
                panic!("primary key arity mismatch");
            }
        }
        if keys.next().is_some() {
            panic!("primary key arity mismatch");
        }
        select
    }

    /// Insert an model into database
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let apple = cake::ActiveModel {
    ///     name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// // `exec` appends `RETURNING` for the primary key and resolves it into
    /// // `InsertResult::last_insert_id`.
    /// let insert_result = cake::Entity::insert(apple).exec(&db).await?;
    /// let id: i32 = insert_result.last_insert_id;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// let apple = cake::ActiveModel {
    ///     name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// assert_eq!(
    ///     cake::Entity::insert(apple).build().0,
    ///     r#"INSERT INTO "cake" ("name") VALUES ($1)"#
    /// );
    /// ```
    fn insert<A>(model: A) -> Insert<A>
    where
        A: ActiveModelTrait<Entity = Self>,
    {
        Insert::one(model)
    }

    /// Insert many models into database
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let apple = cake::ActiveModel {
    ///     name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     ..Default::default()
    /// };
    /// let orange = cake::ActiveModel {
    ///     name: ActiveValue::Set("Orange Scone".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// // `last_insert_id` is taken from the last returned row.
    /// let insert_result = cake::Entity::insert_many([apple, orange]).exec(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// let apple = cake::ActiveModel {
    ///     name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     ..Default::default()
    /// };
    /// let orange = cake::ActiveModel {
    ///     name: ActiveValue::Set("Orange Scone".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// assert_eq!(
    ///     cake::Entity::insert_many([apple, orange]).build().0,
    ///     r#"INSERT INTO "cake" ("name") VALUES ($1), ($2)"#
    /// );
    /// ```
    fn insert_many<A, I>(models: I) -> Insert<A>
    where
        A: ActiveModelTrait<Entity = Self>,
        I: IntoIterator<Item = A>,
    {
        Insert::many(models)
    }

    /// Update an model in database
    ///
    /// - To apply where conditions / filters, see [`QueryFilter`](crate::query::QueryFilter)
    ///
    /// Fails with [`DbErr::PrimaryKeyNotSet`] when a primary-key column of
    /// `model` is [`ActiveValue::NotSet`](crate::ActiveValue::NotSet).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(1),
    ///     name: ActiveValue::Set("Orange".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// // `exec` returns the updated model through `RETURNING`, and fails with
    /// // `DbErr::RecordNotFound` when the statement matches no row.
    /// let updated: fruit::Model = fruit::Entity::update(orange)?
    ///     .filter(fruit::Column::Name.contains("orange"))
    ///     .exec(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::fruit};
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(1),
    ///     name: ActiveValue::Set("Orange".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// assert_eq!(
    ///     fruit::Entity::update(orange)
    ///         .expect("the primary key is set")
    ///         .filter(fruit::Column::Name.contains("orange"))
    ///         .build()
    ///         .0,
    ///     [
    ///         r#"UPDATE "fruit" SET "name" = $1"#,
    ///         r#"WHERE "fruit"."id" = $2 AND "fruit"."name" LIKE $3"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    fn update<A>(model: A) -> Result<UpdateOne<A>, DbErr>
    where
        A: ActiveModelTrait<Entity = Self>,
    {
        Update::one(model)
    }

    /// Update many models in database
    ///
    /// - To apply where conditions / filters, see [`QueryFilter`](crate::query::QueryFilter)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// # use pgorm::pgorm_query::{Expr, Value};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let update_result = fruit::Entity::update_many()
    ///     .col_expr(fruit::Column::CakeId, Expr::value(Value::Int(None)))
    ///     .filter(fruit::Column::Name.contains("Apple"))
    ///     .exec(&db)
    ///     .await?;
    ///
    /// let affected: u64 = update_result.rows_affected;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::pgorm_query::{Expr, Value};
    /// use pgorm::{entity::*, query::*, tests_cfg::fruit};
    ///
    /// assert_eq!(
    ///     fruit::Entity::update_many()
    ///         .col_expr(fruit::Column::CakeId, Expr::value(Value::Int(None)))
    ///         .filter(fruit::Column::Name.contains("Apple"))
    ///         .build()
    ///         .0,
    ///     r#"UPDATE "fruit" SET "cake_id" = $1 WHERE "fruit"."name" LIKE $2"#
    /// );
    /// ```
    fn update_many() -> UpdateMany<Self> {
        Update::many(Self::default())
    }

    /// Delete an model from database
    ///
    /// - To apply where conditions / filters, see [`QueryFilter`](crate::query::QueryFilter)
    ///
    /// Fails with [`DbErr::PrimaryKeyNotSet`] when a primary-key column of
    /// `model` is [`ActiveValue::NotSet`](crate::ActiveValue::NotSet).
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(3),
    ///     ..Default::default()
    /// };
    ///
    /// // Deleting zero rows is `Ok` with `rows_affected: 0`, never an error.
    /// let delete_result = fruit::Entity::delete(orange)?.exec(&db).await?;
    /// let affected: u64 = delete_result.rows_affected;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::fruit};
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(3),
    ///     ..Default::default()
    /// };
    ///
    /// assert_eq!(
    ///     fruit::Entity::delete(orange)
    ///         .expect("the primary key is set")
    ///         .build()
    ///         .0,
    ///     r#"DELETE FROM "fruit" WHERE "fruit"."id" = $1"#
    /// );
    /// ```
    fn delete<A>(model: A) -> Result<DeleteOne<A>, DbErr>
    where
        A: ActiveModelTrait<Entity = Self>,
    {
        Delete::one(model)
    }

    /// Delete many models from database
    ///
    /// - To apply where conditions / filters, see [`QueryFilter`](crate::query::QueryFilter)
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let delete_result = fruit::Entity::delete_many()
    ///     .filter(fruit::Column::Name.contains("Apple"))
    ///     .exec(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::fruit};
    ///
    /// assert_eq!(
    ///     fruit::Entity::delete_many()
    ///         .filter(fruit::Column::Name.contains("Apple"))
    ///         .build()
    ///         .0,
    ///     r#"DELETE FROM "fruit" WHERE "fruit"."name" LIKE $1"#
    /// );
    /// ```
    fn delete_many() -> DeleteMany<Self> {
        Delete::many(Self::default())
    }

    /// Delete a model based on primary key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), DbErr> {
    /// let db = pool.get().await?;
    ///
    /// let delete_result = fruit::Entity::delete_by_id(1).exec(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::fruit};
    ///
    /// assert_eq!(
    ///     fruit::Entity::delete_by_id(1).build().0,
    ///     r#"DELETE FROM "fruit" WHERE "fruit"."id" = $1"#
    /// );
    /// ```
    /// Delete by composite key
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake_filling};
    ///
    /// assert_eq!(
    ///     cake_filling::Entity::delete_by_id((2, 3)).build().0,
    ///     [
    ///         r#"DELETE FROM "cake_filling""#,
    ///         r#"WHERE "cake_filling"."cake_id" = $1 AND "cake_filling"."filling_id" = $2"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if arity of input values don't match arity of primary key
    fn delete_by_id<T>(values: T) -> DeleteMany<Self>
    where
        T: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        let mut delete = Self::delete_many();
        let mut keys = Self::PrimaryKey::iter();
        for v in values.into().into_value_tuple() {
            if let Some(key) = keys.next() {
                let col = key.into_column();
                delete = delete.filter(col.eq(v));
            } else {
                panic!("primary key arity mismatch");
            }
        }
        if keys.next().is_some() {
            panic!("primary key arity mismatch");
        }
        delete
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_delete_by_id_1() {
        use crate::tests_cfg::cake;
        use crate::{entity::*, query::*};

        assert_eq!(
            cake::Entity::delete_by_id(1).as_query().to_string(),
            r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#,
        );
    }

    #[test]
    fn test_delete_by_id_2() {
        use crate::tests_cfg::cake_filling_price;
        use crate::{entity::*, query::*};

        assert_eq!(
            cake_filling_price::Entity::delete_by_id((1, 2))
                .as_query()
                .to_string(),
            r#"DELETE FROM "public"."cake_filling_price" WHERE "cake_filling_price"."cake_id" = 1 AND "cake_filling_price"."filling_id" = 2"#,
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn entity_model_1() {
        use crate::entity::*;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Entity.table_name(), "hello");
        assert_eq!(hello::Entity.schema_name(), None);
    }

    #[test]
    #[cfg(feature = "macros")]
    fn entity_model_2() {
        use crate::entity::*;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello", schema_name = "world")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Entity.table_name(), "hello");
        assert_eq!(hello::Entity.schema_name(), Some("world"));
    }

    #[test]
    #[cfg(feature = "macros")]
    fn entity_model_3() {
        use crate::{entity::*, query::*};

        use std::borrow::Cow;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello", schema_name = "world")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        fn delete_by_id<T>(value: T)
        where
            T: Into<<<hello::Entity as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType>,
        {
            assert_eq!(
                hello::Entity::delete_by_id(value).as_query().to_string(),
                r#"DELETE FROM "world"."hello" WHERE "hello"."id" = 'UUID'"#
            );
        }

        delete_by_id("UUID".to_string());
        delete_by_id("UUID");
        delete_by_id(Cow::from("UUID"));
    }
}
