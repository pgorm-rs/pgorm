use crate::{
    ActiveModelTrait, ActiveValue, ColumnTrait, EntityName, EntityTrait, IntoActiveModel, Iterable,
    PrimaryKeyTrait, QueryTrait,
};
use core::marker::PhantomData;
use pgorm_query::{Expr, InsertStatement, OnConflict, ValueTuple};

/// Performs INSERT operations on a ActiveModel
#[derive(Debug)]
pub struct Insert<A>
where
    A: ActiveModelTrait,
{
    pub(crate) query: InsertStatement,
    pub(crate) columns: Vec<bool>,
    pub(crate) primary_key: Option<ValueTuple>,
    pub(crate) model: PhantomData<A>,
}

impl<A> Default for Insert<A>
where
    A: ActiveModelTrait,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<A> Insert<A>
where
    A: ActiveModelTrait,
{
    pub(crate) fn new() -> Self {
        Self {
            query: InsertStatement::new()
                .into_table(A::Entity::default().table_ref())
                .or_default_values()
                .to_owned(),
            columns: Vec::new(),
            primary_key: None,
            model: PhantomData,
        }
    }

    /// Insert one Model or ActiveModel
    ///
    /// Model
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake, DbBackend};
    ///
    /// assert_eq!(
    ///     Insert::one(cake::Model {
    ///         id: 1,
    ///         name: "Apple Pie".to_owned(),
    ///     })
    ///     .build(DbBackend::Postgres)
    ///     .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#,
    /// );
    /// ```
    /// ActiveModel
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake, DbBackend};
    ///
    /// assert_eq!(
    ///     Insert::one(cake::ActiveModel {
    ///         id: NotSet,
    ///         name: Set("Apple Pie".to_owned()),
    ///     })
    ///     .build(DbBackend::Postgres)
    ///     .to_string(),
    ///     r#"INSERT INTO "cake" ("name") VALUES ('Apple Pie')"#,
    /// );
    /// ```
    pub fn one<M>(m: M) -> Self
    where
        M: IntoActiveModel<A>,
    {
        Self::new().add(m)
    }

    /// Insert many Model or ActiveModel
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake, DbBackend};
    ///
    /// assert_eq!(
    ///     Insert::many([
    ///         cake::Model {
    ///             id: 1,
    ///             name: "Apple Pie".to_owned(),
    ///         },
    ///         cake::Model {
    ///             id: 2,
    ///             name: "Orange Scone".to_owned(),
    ///         }
    ///     ])
    ///     .build(DbBackend::Postgres)
    ///     .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie'), (2, 'Orange Scone')"#,
    /// );
    /// ```
    pub fn many<M, I>(models: I) -> Self
    where
        M: IntoActiveModel<A>,
        I: IntoIterator<Item = M>,
    {
        Self::new().add_many(models)
    }

    /// Add a Model to Self
    ///
    /// # Panics
    ///
    /// Panics if the column value has discrepancy across rows
    #[allow(clippy::should_implement_trait)]
    pub fn add<M>(mut self, m: M) -> Self
    where
        M: IntoActiveModel<A>,
    {
        let mut am: A = m.into_active_model();
        self.primary_key =
            if !<<A::Entity as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::auto_increment() {
                am.get_primary_key_value()
            } else {
                None
            };
        let mut columns = Vec::new();
        let mut values = Vec::new();
        let columns_empty = self.columns.is_empty();
        for (idx, col) in <A::Entity as EntityTrait>::Column::iter().enumerate() {
            let av = am.take(col);
            let av_has_val = av.is_set() || av.is_unchanged();
            if columns_empty {
                self.columns.push(av_has_val);
            } else if self.columns[idx] != av_has_val {
                panic!("columns mismatch");
            }
            match av {
                ActiveValue::Set(value) | ActiveValue::Unchanged(value) => {
                    columns.push(col);
                    values.push(col.save_as(Expr::val(value)));
                }
                ActiveValue::NotSet => {}
            }
        }
        self.query.columns(columns);
        self.query.values_panic(values);
        self
    }

    /// Add many Models to Self
    pub fn add_many<M, I>(mut self, models: I) -> Self
    where
        M: IntoActiveModel<A>,
        I: IntoIterator<Item = M>,
    {
        for model in models.into_iter() {
            self = self.add(model);
        }
        self
    }

    /// On conflict
    ///
    /// on conflict do nothing
    /// ```
    /// use pgorm::{entity::*, query::*, pgorm_query::OnConflict, tests_cfg::cake, DbBackend};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict(
    ///             OnConflict::column(cake::Column::Name)
    ///                 .do_nothing()
    ///                 .to_owned()
    ///         )
    ///         .build(DbBackend::Postgres)
    ///         .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO NOTHING"#,
    /// );
    /// ```
    ///
    /// on conflict do update
    /// ```
    /// use pgorm::{entity::*, query::*, pgorm_query::OnConflict, tests_cfg::cake, DbBackend};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict(
    ///             OnConflict::column(cake::Column::Name)
    ///                 .update_column(cake::Column::Name)
    ///                 .to_owned()
    ///         )
    ///         .build(DbBackend::Postgres)
    ///         .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO UPDATE SET "name" = "excluded"."name""#,
    /// );
    /// ```
    pub fn on_conflict(mut self, on_conflict: OnConflict) -> Self {
        self.query.on_conflict(on_conflict);
        self
    }

    /// Allow insert statement return safely if inserting nothing.
    /// The database will not be affected.
    pub fn do_nothing(self) -> TryInsert<A>
    where
        A: ActiveModelTrait,
    {
        TryInsert::from_insert(self)
    }

    /// alias to do_nothing
    pub fn on_empty_do_nothing(self) -> TryInsert<A>
    where
        A: ActiveModelTrait,
    {
        TryInsert::from_insert(self)
    }

    /// Set ON CONFLICT on primary key do nothing, but with MySQL specific polyfill.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, pgorm_query::OnConflict, tests_cfg::cake, DbBackend};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    ///
    /// assert_eq!(
    ///     cake::Entity::insert(orange.clone())
    ///         .on_conflict_do_nothing()
    ///         .build(DbBackend::MySql)
    ///         .to_string(),
    ///     r#"INSERT INTO `cake` (`id`, `name`) VALUES (2, 'Orange') ON DUPLICATE KEY UPDATE `id` = `id`"#,
    /// );
    /// assert_eq!(
    ///     cake::Entity::insert(orange.clone())
    ///         .on_conflict_do_nothing()
    ///         .build(DbBackend::Postgres)
    ///         .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("id") DO NOTHING"#,
    /// );
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict_do_nothing()
    ///         .build(DbBackend::Sqlite)
    ///         .to_string(),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("id") DO NOTHING"#,
    /// );
    /// ```
    pub fn on_conflict_do_nothing(mut self) -> TryInsert<A>
    where
        A: ActiveModelTrait,
    {
        let primary_keys = <A::Entity as EntityTrait>::PrimaryKey::iter();
        self.query.on_conflict(
            OnConflict::columns(primary_keys.clone())
                .do_nothing_on(primary_keys)
                .to_owned(),
        );

        TryInsert::from_insert(self)
    }
}

impl<A> QueryTrait for Insert<A>
where
    A: ActiveModelTrait,
{
    type QueryStatement = InsertStatement;

    fn query(&mut self) -> &mut InsertStatement {
        &mut self.query
    }

    fn as_query(&self) -> &InsertStatement {
        &self.query
    }

    fn into_query(self) -> InsertStatement {
        self.query
    }
}

/// Performs INSERT operations on a ActiveModel, will do nothing if input is empty.
///
/// All functions works the same as if it is Insert<A>. Please refer to Insert<A> page for more information
#[derive(Debug)]
pub struct TryInsert<A>
where
    A: ActiveModelTrait,
{
    pub(crate) insert_struct: Insert<A>,
}

impl<A> Default for TryInsert<A>
where
    A: ActiveModelTrait,
{
    fn default() -> Self {
        Self::new()
    }
}

#[allow(missing_docs)]
impl<A> TryInsert<A>
where
    A: ActiveModelTrait,
{
    pub(crate) fn new() -> Self {
        Self {
            insert_struct: Insert::new(),
        }
    }

    pub fn one<M>(m: M) -> Self
    where
        M: IntoActiveModel<A>,
    {
        Self::new().add(m)
    }

    pub fn many<M, I>(models: I) -> Self
    where
        M: IntoActiveModel<A>,
        I: IntoIterator<Item = M>,
    {
        Self::new().add_many(models)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn add<M>(mut self, m: M) -> Self
    where
        M: IntoActiveModel<A>,
    {
        self.insert_struct = self.insert_struct.add(m);
        self
    }

    pub fn add_many<M, I>(mut self, models: I) -> Self
    where
        M: IntoActiveModel<A>,
        I: IntoIterator<Item = M>,
    {
        for model in models.into_iter() {
            self.insert_struct = self.insert_struct.add(model);
        }
        self
    }

    pub fn on_conflict(mut self, on_conflict: OnConflict) -> Self {
        self.insert_struct.query.on_conflict(on_conflict);
        self
    }

    // helper function for do_nothing in Insert<A>
    pub fn from_insert(insert: Insert<A>) -> Self {
        Self {
            insert_struct: insert,
        }
    }
}

impl<A> QueryTrait for TryInsert<A>
where
    A: ActiveModelTrait,
{
    type QueryStatement = InsertStatement;

    fn query(&mut self) -> &mut InsertStatement {
        &mut self.insert_struct.query
    }

    fn as_query(&self) -> &InsertStatement {
        &self.insert_struct.query
    }

    fn into_query(self) -> InsertStatement {
        self.insert_struct.query
    }
}
#[cfg(test)]
mod tests {
    use pgorm_query::OnConflict;

    use crate::tests_cfg::cake::{self};
    use crate::{ActiveValue, DbBackend, DbErr, EntityTrait, Insert, IntoActiveModel, QueryTrait};

    #[test]
    fn insert_1() {
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add(cake::ActiveModel {
                    id: ActiveValue::not_set(),
                    name: ActiveValue::set("Apple Pie".to_owned()),
                })
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("name") VALUES ('Apple Pie')"#,
        );
    }

    #[test]
    fn insert_2() {
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add(cake::ActiveModel {
                    id: ActiveValue::set(1),
                    name: ActiveValue::set("Apple Pie".to_owned()),
                })
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#,
        );
    }

    #[test]
    fn insert_3() {
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add(cake::Model {
                    id: 1,
                    name: "Apple Pie".to_owned(),
                })
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#,
        );
    }

    #[test]
    fn insert_4() {
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add_many([
                    cake::Model {
                        id: 1,
                        name: "Apple Pie".to_owned(),
                    },
                    cake::Model {
                        id: 2,
                        name: "Orange Scone".to_owned(),
                    }
                ])
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie'), (2, 'Orange Scone')"#,
        );
    }

    #[test]
    #[should_panic(expected = "columns mismatch")]
    fn insert_5() {
        let apple = cake::ActiveModel {
            name: ActiveValue::set("Apple".to_owned()),
            ..Default::default()
        };
        let orange = cake::ActiveModel {
            id: ActiveValue::set(2),
            name: ActiveValue::set("Orange".to_owned()),
        };
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add_many([apple, orange])
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (NULL, 'Apple'), (2, 'Orange')"#,
        );
    }

    #[test]
    fn insert_6() {
        let orange = cake::ActiveModel {
            id: ActiveValue::set(2),
            name: ActiveValue::set("Orange".to_owned()),
        };

        assert_eq!(
            cake::Entity::insert(orange)
                .on_conflict(
                    OnConflict::column(cake::Column::Name)
                        .do_nothing()
                        .to_owned()
                )
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO NOTHING"#,
        );
    }

    #[test]
    fn insert_7() {
        let orange = cake::ActiveModel {
            id: ActiveValue::set(2),
            name: ActiveValue::set("Orange".to_owned()),
        };

        assert_eq!(
            cake::Entity::insert(orange)
                .on_conflict(
                    OnConflict::column(cake::Column::Name)
                        .update_column(cake::Column::Name)
                        .to_owned()
                )
                .build(DbBackend::Postgres)
                .to_string(),
            r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO UPDATE SET "name" = "excluded"."name""#,
        );
    }

    #[smol_potat::test]
    async fn insert_8() -> Result<(), DbErr> {
        use crate::{DbBackend, MockDatabase, Statement, Transaction};

        mod post {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "posts")]
            pub struct Model {
                #[pgorm(primary_key, select_as = "INTEGER", save_as = "TEXT")]
                pub id: i32,
                pub title: String,
                pub text: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        let model = post::Model {
            id: 1,
            title: "News wrap up 2022".into(),
            text: "brbrbrrrbrbrbrr...".into(),
        };

        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[model.clone()]])
            .into_connection();

        post::Entity::insert(model.into_active_model())
            .exec(&db)
            .await?;

        assert_eq!(
            db.into_transaction_log(),
            [Transaction::many([Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO "posts" ("id", "title", "text") VALUES (CAST($1 AS TEXT), $2, $3) RETURNING CAST("id" AS INTEGER)"#,
                [
                    1.into(),
                    "News wrap up 2022".into(),
                    "brbrbrrrbrbrbrr...".into(),
                ]
            )])]
        );

        Ok(())
    }

    #[smol_potat::test]
    async fn insert_9() -> Result<(), DbErr> {
        use crate::{DbBackend, MockDatabase, MockExecResult, Statement, Transaction};

        mod post {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "posts")]
            pub struct Model {
                #[pgorm(
                    primary_key,
                    auto_increment = false,
                    select_as = "INTEGER",
                    save_as = "TEXT"
                )]
                pub id_primary: i32,
                #[pgorm(
                    primary_key,
                    auto_increment = false,
                    select_as = "INTEGER",
                    save_as = "TEXT"
                )]
                pub id_secondary: i32,
                pub title: String,
                pub text: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        let model = post::Model {
            id_primary: 1,
            id_secondary: 1001,
            title: "News wrap up 2022".into(),
            text: "brbrbrrrbrbrbrr...".into(),
        };

        let db = MockDatabase::new(DbBackend::Postgres)
            .append_exec_results([MockExecResult {
                last_insert_id: 1,
                rows_affected: 1,
            }])
            .into_connection();

        post::Entity::insert(model.into_active_model())
            .exec(&db)
            .await?;

        assert_eq!(
            db.into_transaction_log(),
            [Transaction::many([Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO "posts" ("id_primary", "id_secondary", "title", "text") VALUES (CAST($1 AS TEXT), CAST($2 AS TEXT), $3, $4) RETURNING CAST("id_primary" AS INTEGER), CAST("id_secondary" AS INTEGER)"#,
                [
                    1.into(),
                    1001.into(),
                    "News wrap up 2022".into(),
                    "brbrbrrrbrbrbrr...".into(),
                ]
            )])]
        );

        Ok(())
    }
}
