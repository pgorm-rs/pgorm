use crate::{
    ConnectionTrait, EntityTrait, FromQueryResult, IdenStr, Iterable, ModelTrait,
    PartialModelTrait, PrimaryKeyArity, PrimaryKeyToColumn, PrimaryKeyTrait, QueryResult, Select,
    SelectA, SelectB, SelectProjected, SelectTwo, SelectTwoMany, SelectTwoProjected,
    TryGetableMany, error::*,
};
use futures::{Stream, StreamExt};
use pgorm_query::{SelectStatement, Value, Values};
use std::collections::HashMap;
use std::pin::Pin;
use std::{hash::Hash, marker::PhantomData};
use tokio_postgres::types::ToSql;

use super::ValueHolder;

/// A boxed stream of decoded rows.
///
/// Unlike [`PinBoxStream`](crate::PinBoxStream) this is `Send`, so it can be
/// consumed from a spawned task.
// [spec:pgorm:def:exec.stream+1]
pub type PinBoxSendStream<'db, Item> = Pin<Box<dyn Stream<Item = Item> + Send + 'db>>;

/// The guard every ORM path that sends a `SELECT` passes through: a statement
/// whose projection list is empty renders as `SELECT  FROM "tbl"`, which the
/// server rejects with an opaque syntax error, so it is refused here instead.
///
/// The `select_only` typestate keeps the ORM's own builders out of this state,
/// but an empty `columns([])` / `exprs([])` iterator and a hand-rolled
/// [`SelectStatement`] both still reach it.
// [spec:pgorm:sem:query.build.modifiers+6]
pub(crate) fn ensure_select_list(query: &SelectStatement) -> Result<(), Error> {
    if query.selects().is_empty() {
        return Err(Error::Query(RuntimeError::Internal(
            "select list is empty; add at least one column or expression".to_owned(),
        )));
    }
    Ok(())
}

/// Defines a type to do `SELECT` operations through a [SelectStatement] on a Model
// [spec:pgorm:def:exec.crud]
#[derive(Clone, Debug)]
pub struct Selector<S>
where
    S: SelectorTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) selector: S,
}

/// Performs a raw `SELECT` operation on a model
#[derive(Clone, Debug)]
pub struct SelectorRaw<S>
where
    S: SelectorTrait,
{
    pub(crate) stmt: String,
    pub(crate) values: Values,
    #[allow(dead_code)]
    selector: S,
}

/// A Trait for any type that can perform SELECT queries
// [spec:pgorm:def:exec.crud]
pub trait SelectorTrait {
    #[allow(missing_docs)]
    type Item: Sized;

    /// The method to perform a query on a Model
    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error>;
}

/// Get tuple from query result based on a list of column identifiers
#[derive(Debug)]
pub struct SelectGetableValue<T, C>
where
    T: TryGetableMany,
    C: strum::IntoEnumIterator + pgorm_query::Iden,
{
    columns: PhantomData<C>,
    model: PhantomData<T>,
}

/// Get tuple from query result based on column index
#[derive(Debug)]
pub struct SelectGetableTuple<T>
where
    T: TryGetableMany,
{
    model: PhantomData<T>,
}

/// Defines a type to get a Model
#[derive(Debug)]
pub struct SelectModel<M>
where
    M: FromQueryResult,
{
    model: PhantomData<M>,
}

/// Defines a type to get two Models
#[derive(Clone, Debug)]
pub struct SelectTwoModel<M, N>
where
    M: FromQueryResult,
    N: FromQueryResult,
{
    model: PhantomData<(M, N)>,
}

impl<T, C> SelectorTrait for SelectGetableValue<T, C>
where
    T: TryGetableMany,
    C: strum::IntoEnumIterator + pgorm_query::Iden,
{
    type Item = T;

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        let cols: Vec<String> = C::iter().map(|col| col.to_string()).collect();
        T::try_get_many(&res, "", &cols).map_err(Into::into)
    }
}

impl<T> SelectorTrait for SelectGetableTuple<T>
where
    T: TryGetableMany,
{
    type Item = T;

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        T::try_get_many_by_index(&res).map_err(Into::into)
    }
}

impl<M> SelectorTrait for SelectModel<M>
where
    M: FromQueryResult + Sized,
{
    type Item = M;

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        // tracing::debug!("Got raw query result: {:?}", res);
        M::from_query_result(&res, "")
    }
}

impl<M, N> SelectorTrait for SelectTwoModel<M, N>
where
    M: FromQueryResult + Sized,
    N: FromQueryResult + Sized,
{
    type Item = (M, Option<N>);

    fn from_raw_query_result(res: QueryResult) -> Result<Self::Item, Error> {
        Ok((
            M::from_query_result(&res, SelectA.as_str())?,
            N::from_query_result_optional(&res, SelectB.as_str())?,
        ))
    }
}

impl<E> Select<E>
where
    E: EntityTrait,
{
    /// Perform a Select operation on a Model using a raw SQL string and its
    /// bound parameter values
    #[allow(clippy::wrong_self_convention)]
    pub fn from_raw_sql(self, stmt: String, values: Values) -> SelectorRaw<SelectModel<E::Model>> {
        SelectorRaw {
            stmt,
            values,
            selector: SelectModel { model: PhantomData },
        }
    }

    /// Return a [Selector] from `Self` that wraps a [SelectModel]
    pub fn into_model<M>(self) -> Selector<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        Selector {
            query: self.query,
            selector: SelectModel { model: PhantomData },
        }
    }

    /// Return a [Selector] from `Self` that wraps a [SelectModel] with a [PartialModel](PartialModelTrait)
    ///
    /// ```
    /// # #[cfg(feature = "macros")]
    /// # {
    /// use pgorm::{
    ///     entity::*,
    ///     query::*,
    ///     tests_cfg::cake::{self, Entity as Cake},
    ///     DerivePartialModel, FromQueryResult,
    /// };
    /// use pgorm_query::{Expr, Func, SimpleExpr};
    ///
    /// #[derive(DerivePartialModel, FromQueryResult)]
    /// #[pgorm(entity = "Cake")]
    /// struct PartialCake {
    ///     name: String,
    ///     #[pgorm(
    ///         from_expr = r#"SimpleExpr::FunctionCall(Func::upper(Expr::col((Cake, cake::Column::Name))))"#
    ///     )]
    ///     name_upper: String,
    /// }
    ///
    /// // The select list is cleared, then re-filled with exactly the columns
    /// // and expressions the partial model asks for.
    /// assert_eq!(
    ///     PartialCake::select_cols(cake::Entity::find().select_only())
    ///         .build()
    ///         .0,
    ///     r#"SELECT "cake"."name", UPPER("cake"."name") AS "name_upper" FROM "cake""#
    /// );
    /// # }
    /// ```
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{
    /// #     entity::*, error::*, query::*,
    /// #     tests_cfg::cake::{self, Entity as Cake},
    /// #     DatabasePool, DerivePartialModel, FromQueryResult,
    /// # };
    /// # use pgorm_query::{Expr, Func, SimpleExpr};
    /// #
    /// # #[derive(DerivePartialModel, FromQueryResult)]
    /// # #[pgorm(entity = "Cake")]
    /// # struct PartialCake {
    /// #     name: String,
    /// #     #[pgorm(
    /// #         from_expr = r#"SimpleExpr::FunctionCall(Func::upper(Expr::col((Cake, cake::Column::Name))))"#
    /// #     )]
    /// #     name_upper: String,
    /// # }
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let cakes: Vec<PartialCake> = cake::Entity::find()
    ///     .into_partial_model::<PartialCake>()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn into_partial_model<M>(self) -> Selector<SelectModel<M>>
    where
        M: PartialModelTrait,
    {
        M::select_cols(self.select_only()).into_model::<M>()
    }

    /// Decode selected columns into a value or tuple named by a column enum.
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, DeriveColumn, EnumIter};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    /// enum QueryAs {
    ///     CakeName,
    /// }
    ///
    /// let db = pool.get().await?;
    ///
    /// let res: Vec<String> = cake::Entity::find()
    ///     .select_only()
    ///     .column_as(cake::Column::Name, QueryAs::CakeName)
    ///     .into_values::<_, QueryAs>()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    ///
    /// ```
    /// # #[cfg(feature = "macros")]
    /// # {
    /// use pgorm::{entity::*, query::*, tests_cfg::cake, DeriveColumn, EnumIter};
    ///
    /// #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    /// enum QueryAs {
    ///     CakeName,
    /// }
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .select_only()
    ///         .column_as(cake::Column::Name, QueryAs::CakeName)
    ///         .build()
    ///         .0,
    ///     r#"SELECT "cake"."name" AS "cake_name" FROM "cake""#
    /// );
    /// # }
    /// ```
    ///
    /// Several columns decode into a tuple:
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, DeriveColumn, EnumIter};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    /// enum QueryAs {
    ///     CakeName,
    ///     NumOfCakes,
    /// }
    ///
    /// let db = pool.get().await?;
    ///
    /// let res: Vec<(String, i64)> = cake::Entity::find()
    ///     .select_only()
    ///     .column_as(cake::Column::Name, QueryAs::CakeName)
    ///     .column_as(cake::Column::Id.count(), QueryAs::NumOfCakes)
    ///     .group_by(cake::Column::Name)
    ///     .into_values::<_, QueryAs>()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn into_values<T, C>(self) -> Selector<SelectGetableValue<T, C>>
    where
        T: TryGetableMany,
        C: strum::IntoEnumIterator + pgorm_query::Iden,
    {
        Selector::<SelectGetableValue<T, C>>::with_columns(self.query)
    }

    /// Decode selected columns into a value or tuple by ordinal position.
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let res: Vec<String> = cake::Entity::find()
    ///     .select_only()
    ///     .column(cake::Column::Name)
    ///     .into_tuple()
    ///     .all(&db)
    ///     .await?;
    ///
    /// let pairs: Vec<(String, i32)> = cake::Entity::find()
    ///     .select_only()
    ///     .column(cake::Column::Name)
    ///     .column(cake::Column::Id)
    ///     .group_by(cake::Column::Name)
    ///     .into_tuple()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .select_only()
    ///         .column(cake::Column::Name)
    ///         .column(cake::Column::Id)
    ///         .group_by(cake::Column::Name)
    ///         .build()
    ///         .0,
    ///     r#"SELECT "cake"."name", "cake"."id" FROM "cake" GROUP BY "cake"."name""#
    /// );
    /// ```
    pub fn into_tuple<T>(self) -> Selector<SelectGetableTuple<T>>
    where
        T: TryGetableMany,
    {
        Selector::<SelectGetableTuple<T>>::into_tuple(self.query)
    }

    /// Get one Model from the SELECT query
    pub async fn one<C>(self, db: &C) -> Result<E::Model, Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().one(db).await
    }

    /// Get one Model from the SELECT query
    pub async fn one_opt<C>(self, db: &C) -> Result<Option<E::Model>, Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().one_opt(db).await
    }

    /// Get all Models from the SELECT query
    pub async fn all<C>(self, db: &C) -> Result<Vec<E::Model>, Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().all(db).await
    }

    /// Stream the results of a SELECT operation on a Model
    // [spec:pgorm:def:exec.stream+1]
    pub async fn stream<'b, C>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<E::Model, Error>>, Error>
    where
        C: ConnectionTrait,
        E::Model: 'b,
    {
        self.into_model().stream(db).await
    }

    /// Stream the result of the operation with PartialModel
    // [spec:pgorm:def:exec.stream+1]
    pub async fn stream_partial_model<'b, C, M>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<M, Error>>, Error>
    where
        C: ConnectionTrait,
        M: PartialModelTrait + 'b,
    {
        self.into_partial_model::<M>().stream(db).await
    }
}

// [spec:pgorm:sem:query.build.modifiers+6]
impl<E> SelectProjected<E>
where
    E: EntityTrait,
{
    /// Name the type the custom projection decodes into.
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, FromQueryResult};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// #[derive(Debug, FromQueryResult)]
    /// struct NameOnly {
    ///     name: String,
    /// }
    ///
    /// let db = pool.get().await?;
    ///
    /// let names: Vec<NameOnly> = cake::Entity::find()
    ///     .select_only()
    ///     .column(cake::Column::Name)
    ///     .into_model::<NameOnly>()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn into_model<M>(self) -> Selector<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        Selector {
            query: self.query,
            selector: SelectModel { model: PhantomData },
        }
    }

    /// Decode the custom projection into a value or tuple named by a column
    /// enum. See [`Select::into_values`].
    pub fn into_values<T, C>(self) -> Selector<SelectGetableValue<T, C>>
    where
        T: TryGetableMany,
        C: strum::IntoEnumIterator + pgorm_query::Iden,
    {
        Selector::<SelectGetableValue<T, C>>::with_columns(self.query)
    }

    /// Decode the custom projection into a value or tuple by ordinal
    /// position. See [`Select::into_tuple`].
    pub fn into_tuple<T>(self) -> Selector<SelectGetableTuple<T>>
    where
        T: TryGetableMany,
    {
        Selector::<SelectGetableTuple<T>>::into_tuple(self.query)
    }

    /// Replace the projection with the one the partial model declares.
    ///
    /// The partial model owns the whole select list, exactly as it does on
    /// [`Select::into_partial_model`]; the projection built so far is
    /// discarded, while filters, joins and ordering are kept.
    pub fn into_partial_model<M>(self) -> Selector<SelectModel<M>>
    where
        M: PartialModelTrait,
    {
        M::select_cols(self.select_only()).into_model::<M>()
    }
}

// [spec:pgorm:sem:query.build.modifiers+6]
impl<E, F> SelectTwoProjected<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Name the pair of types the custom projection decodes into. The `A_` and
    /// `B_` column-name prefixes still apply, so the projection has to alias
    /// its expressions accordingly.
    pub fn into_model<M, N>(self) -> Selector<SelectTwoModel<M, N>>
    where
        M: FromQueryResult,
        N: FromQueryResult,
    {
        Selector {
            query: self.query,
            selector: SelectTwoModel { model: PhantomData },
        }
    }

    /// Replace the projection with the two partial models' declared columns.
    pub fn into_partial_model<M, N>(self) -> Selector<SelectTwoModel<M, N>>
    where
        M: PartialModelTrait,
        N: PartialModelTrait,
    {
        let select = self.select_only();
        let select = M::select_cols(select);
        let select = N::select_cols(select);
        select.into_model::<M, N>()
    }
}

impl<E, F> SelectTwo<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Perform a conversion into a [SelectTwoModel]
    pub fn into_model<M, N>(self) -> Selector<SelectTwoModel<M, N>>
    where
        M: FromQueryResult,
        N: FromQueryResult,
    {
        Selector {
            query: self.query,
            selector: SelectTwoModel { model: PhantomData },
        }
    }

    /// Perform a conversion into a [SelectTwoModel] with [PartialModel](PartialModelTrait)
    pub fn into_partial_model<M, N>(self) -> Selector<SelectTwoModel<M, N>>
    where
        M: PartialModelTrait,
        N: PartialModelTrait,
    {
        let select = self.select_only();
        let select = M::select_cols(select);
        let select = N::select_cols(select);
        select.into_model::<M, N>()
    }

    /// Get one Model from the Select query
    pub async fn one<C>(self, db: &C) -> Result<(E::Model, Option<F::Model>), Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().one(db).await
    }

    /// Get one Model from the Select query
    pub async fn one_opt<C>(self, db: &C) -> Result<Option<(E::Model, Option<F::Model>)>, Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().one_opt(db).await
    }

    /// Get all Models from the Select query
    pub async fn all<C>(self, db: &C) -> Result<Vec<(E::Model, Option<F::Model>)>, Error>
    where
        C: ConnectionTrait,
    {
        self.into_model().all(db).await
    }

    /// Stream the results of a Select operation on a Model
    // [spec:pgorm:def:exec.stream+1]
    pub async fn stream<'b, C>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<(E::Model, Option<F::Model>), Error>>, Error>
    where
        C: ConnectionTrait,
        E::Model: 'b,
        F::Model: 'b,
    {
        self.into_model().stream(db).await
    }

    /// Stream the result of the operation with PartialModel
    // [spec:pgorm:def:exec.stream+1]
    pub async fn stream_partial_model<'b, C, M, N>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<(M, Option<N>), Error>>, Error>
    where
        C: ConnectionTrait,
        M: PartialModelTrait + 'b,
        N: PartialModelTrait + 'b,
    {
        self.into_partial_model::<M, N>().stream(db).await
    }
}

impl<E, F> SelectTwoMany<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Performs a conversion to [Selector]
    fn into_model<M, N>(self) -> Selector<SelectTwoModel<M, N>>
    where
        M: FromQueryResult,
        N: FromQueryResult,
    {
        Selector {
            query: self.query,
            selector: SelectTwoModel { model: PhantomData },
        }
    }

    /// Get all Models from the select operation
    ///
    /// > `SelectTwoMany::one()` method has been dropped (#486)
    /// >
    /// > You can get `(Entity, Vec<relatedEntity>)` by first querying a single model from Entity,
    /// > then use [`ModelTrait::find_related`] on the model.
    /// >
    /// > See <https://github.com/pgorm-rs/pgorm/docs/basic-crud/select#lazy-loading> for details.
    // [spec:pgorm:sem:exec.crud.consolidate]
    pub async fn all<C>(self, db: &C) -> Result<Vec<(E::Model, Vec<F::Model>)>, Error>
    where
        C: ConnectionTrait,
    {
        let rows = self.into_model().all(db).await?;
        Ok(consolidate_query_result::<E, F>(rows))
    }

    // pub fn paginate()
    // we could not implement paginate easily, if the number of children for a
    // parent is larger than one page, then we will end up splitting it in two pages
    // so the correct way is actually perform query in two stages
    // paginate the parent model and then populate the children

    // pub fn count()
    // we should only count the number of items of the parent model
}

impl<S> Selector<S>
where
    S: SelectorTrait,
{
    /// Create `Selector` from Statement and columns. Executing this `Selector`
    /// will return a type `T` which implement `TryGetableMany`.
    pub fn with_columns<T, C>(query: SelectStatement) -> Selector<SelectGetableValue<T, C>>
    where
        T: TryGetableMany,
        C: strum::IntoEnumIterator + pgorm_query::Iden,
    {
        Selector {
            query,
            selector: SelectGetableValue {
                columns: PhantomData,
                model: PhantomData,
            },
        }
    }

    /// Get tuple from query result based on column index
    pub fn into_tuple<T>(query: SelectStatement) -> Selector<SelectGetableTuple<T>>
    where
        T: TryGetableMany,
    {
        Selector {
            query,
            selector: SelectGetableTuple { model: PhantomData },
        }
    }

    /// Decode a caller-built [`SelectStatement`]'s rows into a
    /// [`FromQueryResult`] type.
    ///
    /// The statement need not have an entity behind it — a CTE used as the
    /// driving table is the motivating case — and it stays a statement, so
    /// [`one`](Selector::one) still injects its `LIMIT 1` and the
    /// empty-projection guard still runs before anything reaches the server.
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{DatabasePool, Error, FromQueryResult, SelectModel, Selector, tests_cfg::cake};
    /// # use pgorm::alias;
    /// # use pgorm::pgorm_query::{Expr, Query};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// #[derive(Debug, FromQueryResult)]
    /// struct NameOnly {
    ///     name: String,
    /// }
    ///
    /// let statement = Query::select()
    ///     .expr_as(Expr::col(cake::Column::Name), alias("name"))
    ///     .from(cake::Entity)
    ///     .to_owned();
    ///
    /// let db = pool.get().await?;
    /// let names: Vec<NameOnly> = Selector::<SelectModel<NameOnly>>::from_select::<NameOnly>(statement)
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.crud.selector-entry+1]
    pub fn from_select<M>(query: SelectStatement) -> Selector<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        Selector {
            query,
            selector: SelectModel { model: PhantomData },
        }
    }

    fn into_selector_raw(self) -> Result<SelectorRaw<S>, Error> {
        ensure_select_list(&self.query)?;
        let (stmt, values) = self.query.build();

        Ok(SelectorRaw {
            stmt,
            values,
            selector: self.selector,
        })
    }

    /// Get an item from the Select query
    // [spec:pgorm:sem:exec.crud.select+2]
    pub async fn one<C>(mut self, db: &C) -> Result<S::Item, Error>
    where
        C: ConnectionTrait,
    {
        self.query.limit(1);
        self.into_selector_raw()?.one(db).await
    }

    /// Get an item from the Select query
    pub async fn one_opt<C>(mut self, db: &C) -> Result<Option<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        self.query.limit(1);
        self.into_selector_raw()?.one_opt(db).await
    }

    /// Get all items from the Select query
    pub async fn all<C>(self, db: &C) -> Result<Vec<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        self.into_selector_raw()?.all(db).await
    }

    /// Stream the results of the Select operation
    // [spec:pgorm:def:exec.stream+1]
    pub async fn stream<'b, C>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<S::Item, Error>>, Error>
    where
        C: ConnectionTrait,
        S: 'b,
    {
        self.into_selector_raw()?.stream(db).await
    }
}

impl<S> SelectorRaw<S>
where
    S: SelectorTrait,
{
    /// Select a custom Model from a raw SQL string and its bound parameter
    /// values.
    pub fn from_statement<M>(stmt: String, values: Values) -> SelectorRaw<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        SelectorRaw {
            stmt,
            values,
            selector: SelectModel { model: PhantomData },
        }
    }

    /// Decode a raw statement's rows into a tuple by column index.
    ///
    /// The ordinal counterpart of [`with_columns`](SelectorRaw::with_columns),
    /// which needs an `Iden` enum to name the columns — an enum a caller who
    /// already holds the SQL text usually does not have.
    ///
    /// ```no_run
    /// # use pgorm::{DatabasePool, Error, SelectGetableTuple, SelectorRaw};
    /// # use pgorm::pgorm_query::Values;
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let rows: Vec<(i32, String)> =
    ///     SelectorRaw::<SelectGetableTuple<(i32, String)>>::into_tuple::<(i32, String)>(
    ///         r#"SELECT "cake"."id", "cake"."name" FROM "cake""#.to_owned(),
    ///         Values(vec![]),
    ///     )
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.crud.selector-entry+1]
    pub fn into_tuple<T>(stmt: String, values: Values) -> SelectorRaw<SelectGetableTuple<T>>
    where
        T: TryGetableMany,
    {
        SelectorRaw {
            stmt,
            values,
            selector: SelectGetableTuple { model: PhantomData },
        }
    }

    /// Create `SelectorRaw` from Statement and columns. Executing this `SelectorRaw` will
    /// return a type `T` which implement `TryGetableMany`.
    pub fn with_columns<T, C>(stmt: String, values: Values) -> SelectorRaw<SelectGetableValue<T, C>>
    where
        T: TryGetableMany,
        C: strum::IntoEnumIterator + pgorm_query::Iden,
    {
        SelectorRaw {
            stmt,
            values,
            selector: SelectGetableValue {
                columns: PhantomData,
                model: PhantomData,
            },
        }
    }

    /// Decode the raw statement's rows into a custom `FromQueryResult` type.
    ///
    /// ```no_run
    /// # #[cfg(feature = "macros")]
    /// # {
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, FromQueryResult};
    /// # use pgorm::pgorm_query::Values;
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// #[derive(Debug, PartialEq, FromQueryResult)]
    /// struct SelectResult {
    ///     name: String,
    ///     num_of_cakes: i64,
    /// }
    ///
    /// let db = pool.get().await?;
    ///
    /// let res: Vec<SelectResult> = cake::Entity::find()
    ///     .from_raw_sql(
    ///         r#"SELECT "cake"."name", count("cake"."id") AS "num_of_cakes" FROM "cake" GROUP BY "cake"."name""#
    ///             .to_owned(),
    ///         Values(vec![]),
    ///     )
    ///     .into_model::<SelectResult>()
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// # }
    /// ```
    pub fn into_model<M>(self) -> SelectorRaw<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        SelectorRaw {
            stmt: self.stmt,
            values: self.values,
            selector: SelectModel { model: PhantomData },
        }
    }

    /// Get an item from the Select query.
    ///
    /// The raw statement is executed exactly as written — no `LIMIT` is
    /// injected. Zero rows fails with [`Error::RecordNotFound`]; use
    /// [`one_opt`](SelectorRaw::one_opt) for an `Option` instead.
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// # use pgorm::pgorm_query::{Value, Values};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let cake: cake::Model = cake::Entity::find()
    ///     .from_raw_sql(
    ///         r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "id" = $1"#.to_owned(),
    ///         Values(vec![Value::Int(Some(1))]),
    ///     )
    ///     .one(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.crud.select+2]
    pub async fn one<C>(self, db: &C) -> Result<S::Item, Error>
    where
        C: ConnectionTrait,
    {
        let values = self
            .values
            .0
            .into_iter()
            .map(ValueHolder)
            .collect::<Vec<_>>();
        let values = values.iter().map(|x| x as _).collect::<Vec<_>>();
        let row = db.query_opt(&self.stmt, &values).await?;
        match row {
            Some(row) => Ok(S::from_raw_query_result(QueryResult { row })?),
            None => Err(Error::RecordNotFound),
        }
    }

    /// Get an item from the Select query, or `None` when it returns no rows.
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// # use pgorm::pgorm_query::{Value, Values};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let cake: Option<cake::Model> = cake::Entity::find()
    ///     .from_raw_sql(
    ///         r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "id" = $1"#.to_owned(),
    ///         Values(vec![Value::Int(Some(1))]),
    ///     )
    ///     .one_opt(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn one_opt<C>(self, db: &C) -> Result<Option<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        let values = self
            .values
            .0
            .into_iter()
            .map(ValueHolder)
            .collect::<Vec<_>>();
        let values = values.iter().map(|x| x as _).collect::<Vec<_>>();
        let row = db.query_opt(&self.stmt, &values).await?;
        match row {
            Some(row) => Ok(Some(S::from_raw_query_result(QueryResult { row })?)),
            None => Ok(None),
        }
    }

    /// Get all items from the Select query.
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// # use pgorm::pgorm_query::Values;
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let cakes: Vec<cake::Model> = cake::Entity::find()
    ///     .from_raw_sql(
    ///         r#"SELECT "cake"."id", "cake"."name" FROM "cake""#.to_owned(),
    ///         Values(vec![]),
    ///     )
    ///     .all(&db)
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn all<C>(self, db: &C) -> Result<Vec<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        // tracing::warn!("Querying all");
        let values = self
            .values
            .0
            .into_iter()
            .map(ValueHolder)
            .collect::<Vec<_>>();
        let values = values.iter().map(|x| x as _).collect::<Vec<_>>();
        let rows = db.query_all(&self.stmt, &values).await?;
        // tracing::warn!("Got rows!");
        let mut models = Vec::new();
        for row in rows.into_iter() {
            models.push(S::from_raw_query_result(QueryResult { row })?);
        }
        // tracing::warn!("Got models!");
        Ok(models)
    }

    /// Stream the results of the Select operation, decoding each row as it
    /// arrives rather than buffering the whole result set
    // [spec:pgorm:sem:exec.stream.decode+1]
    pub async fn stream<'b, C>(
        self,
        db: &C,
    ) -> Result<PinBoxSendStream<'b, Result<S::Item, Error>>, Error>
    where
        C: ConnectionTrait,
        S: 'b,
    {
        let values = self
            .values
            .0
            .into_iter()
            .map(ValueHolder)
            .collect::<Vec<_>>();
        let rows = db
            .query_raw(&self.stmt, values.iter().map(|x| x as &(dyn ToSql + Sync)))
            .await?;
        Ok(Box::pin(rows.map(|row| match row {
            Ok(row) => S::from_raw_query_result(QueryResult { row }),
            Err(err) => Err(Error::Postgres(err)),
        })))
    }
}

// [spec:pgorm:sem:exec.crud.consolidate]
#[allow(clippy::unwrap_used)]
fn consolidate_query_result<L, R>(
    rows: Vec<(L::Model, Option<R::Model>)>,
) -> Vec<(L::Model, Vec<R::Model>)>
where
    L: EntityTrait,
    R: EntityTrait,
{
    match <<L::PrimaryKey as PrimaryKeyTrait>::ValueType as PrimaryKeyArity>::ARITY {
        1 => {
            let col = <L::PrimaryKey as Iterable>::iter()
                .next()
                .unwrap()
                .into_column();
            consolidate_query_result_of::<L, R, UnitPk<L>>(rows, UnitPk(col))
        }
        2 => {
            let mut iter = <L::PrimaryKey as Iterable>::iter();
            let col1 = iter.next().unwrap().into_column();
            let col2 = iter.next().unwrap().into_column();
            consolidate_query_result_of::<L, R, PairPk<L>>(rows, PairPk(col1, col2))
        }
        _ => {
            let cols: Vec<_> = <L::PrimaryKey as Iterable>::iter()
                .map(|pk| pk.into_column())
                .collect();
            consolidate_query_result_of::<L, R, TuplePk<L>>(rows, TuplePk(cols))
        }
    }
}

trait ModelKey<E: EntityTrait> {
    type Type: Hash + PartialEq + Eq;
    fn get(&self, model: &E::Model) -> Self::Type;
}

// This could have been an array of [E::Column; <E::PrimaryKey as PrimaryKeyTrait>::ARITY], but it still doesn't compile
struct UnitPk<E: EntityTrait>(E::Column);
struct PairPk<E: EntityTrait>(E::Column, E::Column);
struct TuplePk<E: EntityTrait>(Vec<E::Column>);

impl<E: EntityTrait> ModelKey<E> for UnitPk<E> {
    type Type = Value;
    fn get(&self, model: &E::Model) -> Self::Type {
        model.get(self.0)
    }
}

impl<E: EntityTrait> ModelKey<E> for PairPk<E> {
    type Type = (Value, Value);
    fn get(&self, model: &E::Model) -> Self::Type {
        (model.get(self.0), model.get(self.1))
    }
}

impl<E: EntityTrait> ModelKey<E> for TuplePk<E> {
    type Type = Vec<Value>;
    fn get(&self, model: &E::Model) -> Self::Type {
        let mut key = Vec::new();
        for col in self.0.iter() {
            key.push(model.get(*col));
        }
        key
    }
}

fn consolidate_query_result_of<L, R, KEY: ModelKey<L>>(
    mut rows: Vec<(L::Model, Option<R::Model>)>,
    model_key: KEY,
) -> Vec<(L::Model, Vec<R::Model>)>
where
    L: EntityTrait,
    R: EntityTrait,
{
    let mut hashmap: HashMap<KEY::Type, Vec<R::Model>> =
        rows.iter_mut().fold(HashMap::new(), |mut acc, row| {
            let key = model_key.get(&row.0);
            if let Some(value) = row.1.take() {
                let vec: Option<&mut Vec<R::Model>> = acc.get_mut(&key);
                if let Some(vec) = vec {
                    vec.push(value)
                } else {
                    acc.insert(key, vec![value]);
                }
            } else {
                acc.entry(key).or_default();
            }

            acc
        });

    rows.into_iter()
        .filter_map(|(l_model, _)| {
            let l_pk = model_key.get(&l_model);
            let r_models = hashmap.remove(&l_pk);
            r_models.map(|r_models| (l_model, r_models))
        })
        .collect()
}

/// This is the legacy consolidate algorithm. Kept for reference
#[allow(dead_code)]
fn consolidate_query_result_of_ordered_rows<L, R>(
    rows: Vec<(L::Model, Option<R::Model>)>,
) -> Vec<(L::Model, Vec<R::Model>)>
where
    L: EntityTrait,
    R: EntityTrait,
{
    let mut acc: Vec<(L::Model, Vec<R::Model>)> = Vec::new();
    for (l, r) in rows {
        if let Some((last_l, last_r)) = acc.last_mut() {
            let mut same_l = true;
            for pk_col in <L::PrimaryKey as Iterable>::iter() {
                let col = pk_col.into_column();
                let val = l.get(col);
                let last_val = last_l.get(col);
                if !val.eq(&last_val) {
                    same_l = false;
                    break;
                }
            }
            if same_l && let Some(r) = r {
                last_r.push(r);
                continue;
            }
        }
        let rows = match r {
            Some(r) => vec![r],
            None => vec![],
        };
        acc.push((l, rows));
    }
    acc
}

#[cfg(test)]
mod tests {
    use pgorm::tests_cfg::*;
    use pgorm::{EntityTrait, QueryTrait};

    use pretty_assertions::assert_eq;

    #[test]
    fn also_related_query() {
        assert_eq!(
            Cake::find()
                .find_also_related(Fruit)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn with_related_query() {
        assert_eq!(
            Cake::find()
                .find_with_related(Fruit)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"ORDER BY "cake"."id" ASC"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn also_linked_query() {
        assert_eq!(
            Cake::find()
                .find_also_linked(entity_linked::CakeToFillingVendor)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""r2"."id" AS "B_id", "r2"."name" AS "B_name""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "cake_filling" AS "r0" ON "cake"."id" = "r0"."cake_id""#,
                r#"LEFT JOIN "filling" AS "r1" ON "r0"."filling_id" = "r1"."id""#,
                r#"LEFT JOIN "vendor" AS "r2" ON "r1"."vendor_id" = "r2"."id""#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn with_linked_query() {
        assert_eq!(
            Cake::find()
                .find_with_linked(entity_linked::CakeToFillingVendor)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""r2"."id" AS "B_id", "r2"."name" AS "B_name""#,
                r#"FROM "cake""#,
                r#"LEFT JOIN "cake_filling" AS "r0" ON "cake"."id" = "r0"."cake_id""#,
                r#"LEFT JOIN "filling" AS "r1" ON "r0"."filling_id" = "r1"."id""#,
                r#"LEFT JOIN "vendor" AS "r2" ON "r1"."vendor_id" = "r2"."id""#,
            ]
            .join(" ")
        );
    }
}
