use crate::{
    ActiveModelTrait, ActiveValue, ColumnTrait, DbErr, EntityName, EntityTrait, IdenStatic,
    IntoActiveModel, Iterable, PrimaryKeyTrait, QueryTrait, RuntimeErr,
};
use core::marker::PhantomData;
use pgorm_query::{Expr, InsertStatement, OnConflict, ValueTuple};

/// The column shape an [`Insert`] has recorded from the models added to it.
///
/// Emptiness is a variant rather than a predicate over the bitmap: `Present` is
/// only reachable through [`Insert::add`] for a model that set at least one
/// column, so an all-`NotSet` model cannot present itself as a row of values.
/// A model that disagrees with the shape already recorded is a variant too:
/// [`Insert::add`] returns `Self` so calls chain and has nowhere to report the
/// disagreement, so it holds it until an execution path can fail with it.
#[derive(Debug)]
enum InsertColumns {
    /// No column is set. `rows` counts the models added so far, every one of
    /// which left every column `NotSet`; zero is the initial state.
    Empty { rows: u32 },
    /// Per-column presence recorded from the first model added, which set at
    /// least one column.
    Present(Vec<bool>),
    /// A model's present columns differed from what was already recorded.
    /// Terminal: the statement took neither that model's columns nor its
    /// values, and models added afterwards are no longer compared.
    Mismatch {
        /// Columns the models before it set and it did not.
        first_only: Vec<String>,
        /// Columns it set and the models before it did not.
        later_only: Vec<String>,
    },
}

impl InsertColumns {
    /// Records the disagreement between the presence bitmap already held and
    /// that of the model being added, naming the columns by which they differ.
    /// A `recorded` shorter than `present` reads as "set nothing", which is the
    /// empty state's shape.
    fn mismatch<A>(recorded: &[bool], present: &[bool]) -> Self
    where
        A: ActiveModelTrait,
    {
        let set = |bitmap: &[bool], index: usize| bitmap.get(index).copied().unwrap_or(false);
        let mut first_only = Vec::new();
        let mut later_only = Vec::new();
        for (index, col) in <A::Entity as EntityTrait>::Column::iter().enumerate() {
            match (set(recorded, index), set(present, index)) {
                (true, false) => first_only.push(col.as_str().to_owned()),
                (false, true) => later_only.push(col.as_str().to_owned()),
                (true, true) | (false, false) => {}
            }
        }
        Self::Mismatch {
            first_only,
            later_only,
        }
    }
}

/// Names the columns each side of a mismatch sets and the other does not,
/// dropping the side that has none.
fn columns_mismatch_err(first_only: &[String], later_only: &[String]) -> DbErr {
    let sides = [
        (first_only, "set in the first model but not in a later one"),
        (later_only, "set in a later model but not in the first"),
    ]
    .into_iter()
    .filter(|(names, _)| !names.is_empty())
    .map(|(names, side)| format!("{} {side}", quoted_columns(names)))
    .collect::<Vec<_>>();

    DbErr::Query(RuntimeErr::Internal(format!(
        "models added to one insert do not share a column set: {}",
        sides.join("; ")
    )))
}

/// Renders `` `id` is `` for one column and `` `id`, `name` are `` for several.
fn quoted_columns(names: &[String]) -> String {
    let list = names
        .iter()
        .map(|name| format!("`{name}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let verb = if names.len() == 1 { "is" } else { "are" };
    format!("{list} {verb}")
}

/// Performs INSERT operations on a ActiveModel
#[derive(Debug)]
pub struct Insert<A>
where
    A: ActiveModelTrait,
{
    pub(crate) query: InsertStatement,
    columns: InsertColumns,
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

// [spec:pgorm:sem:query.build.insert+1]
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
            columns: InsertColumns::Empty { rows: 0 },
            primary_key: None,
            model: PhantomData,
        }
    }

    /// Whether the statement carries no column at all: either no model was
    /// added, or every model added left every column `NotSet`.
    // [spec:pgorm:sem:query.build.insert.empty-failsafe+1]
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self.columns, InsertColumns::Empty { .. })
    }

    /// The columns mismatch recorded while models were added, if any.
    ///
    /// [`add`](Self::add) returns `Self` so that calls chain, so a model whose
    /// present columns disagree with the first model's is recorded rather than
    /// reported there. Every execution path asks here and fails with the
    /// resulting [`DbErr::Query`] before sending any SQL; callers that want the
    /// error sooner can ask directly.
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub fn ensure_uniform_columns(&self) -> Result<(), DbErr> {
        match &self.columns {
            InsertColumns::Mismatch {
                first_only,
                later_only,
            } => Err(columns_mismatch_err(first_only, later_only)),
            InsertColumns::Empty { .. } | InsertColumns::Present(_) => Ok(()),
        }
    }

    /// Insert one Model or ActiveModel
    ///
    /// Model
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     Insert::one(cake::Model {
    ///         id: 1,
    ///         name: "Apple Pie".to_owned(),
    ///     })
    ///     .as_query()
    ///     .to_string(QueryBuilder),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#,
    /// );
    /// ```
    /// ActiveModel
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     Insert::one(cake::ActiveModel {
    ///         id: ActiveValue::NotSet,
    ///         name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     })
    ///     .as_query()
    ///     .to_string(QueryBuilder),
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
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
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
    ///     .as_query()
    ///     .to_string(QueryBuilder),
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
    /// A model whose present columns differ from those of the models already
    /// added contributes neither columns nor values; the disagreement is
    /// recorded and reported by [`ensure_uniform_columns`](Self::ensure_uniform_columns)
    /// and by every execution path.
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    // [spec:pgorm:sem:query.build.insert+1]
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
        let mut present = Vec::new();
        for col in <A::Entity as EntityTrait>::Column::iter() {
            let av = am.take(col);
            present.push(av.is_set() || av.is_unchanged());
            match av {
                ActiveValue::Set(value) | ActiveValue::Unchanged(value) => {
                    columns.push(col);
                    values.push(col.save_as(Expr::val(value)));
                }
                ActiveValue::NotSet => {}
            }
        }

        match &self.columns {
            InsertColumns::Mismatch { .. } => return self,
            InsertColumns::Empty { rows } if columns.is_empty() => {
                let rows = rows.saturating_add(1);
                self.columns = InsertColumns::Empty { rows };
                self.query.or_default_values_many(rows);
                return self;
            }
            InsertColumns::Empty { rows: 0 } => self.columns = InsertColumns::Present(present),
            InsertColumns::Empty { .. } => {
                self.columns = InsertColumns::mismatch::<A>(&[], &present);
                return self;
            }
            InsertColumns::Present(recorded) if *recorded != present => {
                self.columns = InsertColumns::mismatch::<A>(recorded, &present);
                return self;
            }
            InsertColumns::Present(_) => {}
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
    /// use pgorm::{entity::*, query::*, pgorm_query::{OnConflict, QueryBuilder}, tests_cfg::cake};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict(OnConflict::column(cake::Column::Name).do_nothing())
    ///         .as_query()
    ///         .to_string(QueryBuilder),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO NOTHING"#,
    /// );
    /// ```
    ///
    /// on conflict do update
    /// ```
    /// use pgorm::{entity::*, query::*, pgorm_query::{OnConflict, QueryBuilder}, tests_cfg::cake};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict(
    ///             OnConflict::column(cake::Column::Name).update_column(cake::Column::Name)
    ///         )
    ///         .as_query()
    ///         .to_string(QueryBuilder),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO UPDATE SET "name" = "excluded"."name""#,
    /// );
    /// ```
    pub fn on_conflict<T>(mut self, on_conflict: T) -> Self
    where
        T: Into<OnConflict>,
    {
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

    /// Set ON CONFLICT on the primary key columns to do nothing.
    ///
    /// An entity with no primary key column gets the arbiter-less
    /// `ON CONFLICT DO NOTHING`, which answers for every constraint — the
    /// widest reading of "the primary key conflicted" when there is no primary
    /// key to name.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, pgorm_query::{OnConflict, QueryBuilder}, tests_cfg::cake};
    ///
    /// let orange = cake::ActiveModel {
    ///     id: ActiveValue::set(2),
    ///     name: ActiveValue::set("Orange".to_owned()),
    /// };
    ///
    /// assert_eq!(
    ///     cake::Entity::insert(orange)
    ///         .on_conflict_do_nothing()
    ///         .as_query()
    ///         .to_string(QueryBuilder),
    ///     r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("id") DO NOTHING"#,
    /// );
    /// ```
    // [spec:pgorm:sem:query.build.insert.empty-failsafe+1]
    pub fn on_conflict_do_nothing(mut self) -> TryInsert<A>
    where
        A: ActiveModelTrait,
    {
        let mut primary_keys = <A::Entity as EntityTrait>::PrimaryKey::iter();
        let on_conflict = match primary_keys.next() {
            Some(first) => OnConflict::column(first)
                .and_columns(primary_keys)
                .do_nothing(),
            None => OnConflict::do_nothing(),
        };
        self.query.on_conflict(on_conflict);

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
/// All functions works the same as if it is `Insert<A>`. Please refer to the
/// `Insert<A>` page for more information
// [spec:pgorm:sem:query.build.insert.empty-failsafe+1]
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

    pub fn on_conflict<T>(mut self, on_conflict: T) -> Self
    where
        T: Into<OnConflict>,
    {
        self.insert_struct.query.on_conflict(on_conflict);
        self
    }

    // helper function for do_nothing in Insert<A>
    pub fn from_insert(insert: Insert<A>) -> Self {
        Self {
            insert_struct: insert,
        }
    }

    /// The columns mismatch recorded while models were added, if any; see
    /// [`Insert::ensure_uniform_columns`].
    // [spec:pgorm:req:query.build.insert.uniform-columns+1]
    pub fn ensure_uniform_columns(&self) -> Result<(), DbErr> {
        self.insert_struct.ensure_uniform_columns()
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
    use pgorm_query::{OnConflict, QueryBuilder};

    use crate::tests_cfg::cake::{self};
    use crate::{ActiveValue, EntityTrait, Insert, IntoActiveModel, QueryTrait};

    #[test]
    fn insert_1() {
        assert_eq!(
            Insert::<cake::ActiveModel>::new()
                .add(cake::ActiveModel {
                    id: ActiveValue::not_set(),
                    name: ActiveValue::set("Apple Pie".to_owned()),
                })
                .as_query()
                .to_string(QueryBuilder),
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
                .as_query()
                .to_string(QueryBuilder),
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
                .as_query()
                .to_string(QueryBuilder),
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
                .as_query()
                .to_string(QueryBuilder),
            r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie'), (2, 'Orange Scone')"#,
        );
    }

    #[test]
    fn insert_5() {
        let apple = cake::ActiveModel {
            name: ActiveValue::set("Apple".to_owned()),
            ..Default::default()
        };
        let orange = cake::ActiveModel {
            id: ActiveValue::set(2),
            name: ActiveValue::set("Orange".to_owned()),
        };
        let insert = Insert::<cake::ActiveModel>::new().add_many([apple, orange]);

        assert!(insert.ensure_uniform_columns().is_err());
        assert_eq!(
            insert.as_query().to_string(QueryBuilder),
            r#"INSERT INTO "cake" ("name") VALUES ('Apple')"#,
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
                .on_conflict(OnConflict::column(cake::Column::Name).do_nothing())
                .as_query()
                .to_string(QueryBuilder),
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
                    OnConflict::column(cake::Column::Name).update_column(cake::Column::Name)
                )
                .as_query()
                .to_string(QueryBuilder),
            r#"INSERT INTO "cake" ("id", "name") VALUES (2, 'Orange') ON CONFLICT ("name") DO UPDATE SET "name" = "excluded"."name""#,
        );
    }

    #[test]
    fn insert_8() {
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

        assert_eq!(
            post::Entity::insert(model.into_active_model())
                .as_query()
                .to_string(QueryBuilder),
            r#"INSERT INTO "posts" ("id", "title", "text") VALUES (CAST(1 AS TEXT), 'News wrap up 2022', 'brbrbrrrbrbrbrr...')"#,
        );
    }

    #[test]
    fn insert_9() {
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

        assert_eq!(
            post::Entity::insert(model.into_active_model())
                .as_query()
                .to_string(QueryBuilder),
            [
                r#"INSERT INTO "posts" ("id_primary", "id_secondary", "title", "text")"#,
                r#"VALUES (CAST(1 AS TEXT), CAST(1001 AS TEXT), 'News wrap up 2022', 'brbrbrrrbrbrbrr...')"#,
            ]
            .join(" "),
        );
    }
}
