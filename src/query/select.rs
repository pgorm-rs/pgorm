use crate::{ColumnTrait, EntityTrait, Iterable, QueryFilter, QueryOrder, QuerySelect, QueryTrait};
use core::fmt::Debug;
use core::marker::PhantomData;
use pgorm_query::{AliasName, Expr, IntoColumnRef, SelectExpr, SelectStatement, SimpleExpr};

/// Defines a structure to perform select operations
// [spec:pgorm:req:query.build]
#[derive(Clone, Debug)]
pub struct Select<E>
where
    E: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<E>,
}

/// Defines a structure to perform a SELECT operation on two Models
#[derive(Clone, Debug)]
pub struct SelectTwo<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<(E, F)>,
}

/// Defines a structure to perform a SELECT operation on many Models
#[derive(Clone, Debug)]
pub struct SelectTwoMany<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<(E, F)>,
}

/// A [`Select<E>`] whose projection list has been cleared by
/// [`Select::select_only`] and not yet refilled.
///
/// The statement still renders — `as_query`, `build` and the rest of
/// [`QueryTrait`] are available — but nothing can execute it, because a
/// `SELECT` with no projection has no rows to decode. Adding any column or
/// expression moves to [`SelectProjected<E>`], where the terminal operations
/// live.
// [spec:pgorm:sem:query.build.modifiers+6]
#[derive(Clone, Debug)]
pub struct SelectCustom<E>
where
    E: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<E>,
}

/// A select carrying a caller-authored projection instead of `E`'s columns.
///
/// The rows no longer have `E::Model`'s shape, so the model-typed terminals
/// (`all`, `one`, `stream`) are absent and the decode target must be named:
/// [`into_model`](SelectProjected::into_model),
/// [`into_partial_model`](SelectProjected::into_partial_model),
/// [`into_tuple`](SelectProjected::into_tuple) or
/// [`into_values`](SelectProjected::into_values). The two-model combinators
/// are absent for the same reason — their `A_`/`B_` aliasing scheme assumes
/// `E`'s own select list.
// [spec:pgorm:sem:query.build.modifiers+6]
#[derive(Clone, Debug)]
pub struct SelectProjected<E>
where
    E: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<E>,
}

/// A [`SelectTwo<E, F>`] whose projection list has been cleared by
/// [`SelectTwo::select_only`]: the two-model counterpart of
/// [`SelectCustom<E>`].
// [spec:pgorm:sem:query.build.modifiers+6]
#[derive(Clone, Debug)]
pub struct SelectTwoCustom<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<(E, F)>,
}

/// A two-model select carrying a caller-authored projection: the two-model
/// counterpart of [`SelectProjected<E>`].
// [spec:pgorm:sem:query.build.modifiers+6]
#[derive(Clone, Debug)]
pub struct SelectTwoProjected<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) query: SelectStatement,
    pub(crate) entity: PhantomData<(E, F)>,
}

/// Performs a conversion to [SimpleExpr]
// [spec:pgorm:def:query.build.query-trait]
pub trait IntoSimpleExpr {
    /// Method to perform the conversion
    fn into_simple_expr(self) -> SimpleExpr;
}

macro_rules! impl_trait {
    ( $trait: ident ) => {
        impl<E> $trait for Select<E>
        where
            E: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E> $trait for SelectCustom<E>
        where
            E: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E> $trait for SelectProjected<E>
        where
            E: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E, F> $trait for SelectTwo<E, F>
        where
            E: EntityTrait,
            F: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E, F> $trait for SelectTwoCustom<E, F>
        where
            E: EntityTrait,
            F: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E, F> $trait for SelectTwoProjected<E, F>
        where
            E: EntityTrait,
            F: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }

        impl<E, F> $trait for SelectTwoMany<E, F>
        where
            E: EntityTrait,
            F: EntityTrait,
        {
            type QueryStatement = SelectStatement;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
        }
    };
}

impl_trait!(QueryFilter);
impl_trait!(QueryOrder);

macro_rules! impl_query_select {
    ( $selector: ident < $( $param: ident ),+ >, $projected: ty, | $this: ident | $body: expr ) => {
        impl< $( $param ),+ > QuerySelect for $selector < $( $param ),+ >
        where
            $( $param: EntityTrait, )+
        {
            type QueryStatement = SelectStatement;
            type Projected = $projected;

            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }

            fn into_projected(self) -> Self::Projected {
                let $this = self;
                $body
            }
        }
    };
}

impl_query_select!(Select<E>, Self, |this| this);
impl_query_select!(SelectCustom<E>, SelectProjected<E>, |this| {
    SelectProjected {
        query: this.query,
        entity: PhantomData,
    }
});
impl_query_select!(SelectProjected<E>, Self, |this| this);
impl_query_select!(SelectTwo<E, F>, Self, |this| this);
impl_query_select!(
    SelectTwoCustom<E, F>,
    SelectTwoProjected<E, F>,
    |this| SelectTwoProjected {
        query: this.query,
        entity: PhantomData,
    }
);
impl_query_select!(SelectTwoProjected<E, F>, Self, |this| this);
impl_query_select!(SelectTwoMany<E, F>, Self, |this| this);

impl<C> IntoSimpleExpr for C
where
    C: ColumnTrait,
{
    fn into_simple_expr(self) -> SimpleExpr {
        SimpleExpr::Column(self.as_column_ref().into_column_ref())
    }
}

impl IntoSimpleExpr for Expr {
    fn into_simple_expr(self) -> SimpleExpr {
        self.into()
    }
}

impl IntoSimpleExpr for SimpleExpr {
    fn into_simple_expr(self) -> SimpleExpr {
        self
    }
}

/// One item of a projection list.
///
/// A column projects through `col.select_as(col.into_expr())`, so an
/// enum-typed column is cast to text exactly as it is in the default select
/// list and in [`column`](QuerySelect::column). An [`Expr`] or [`SimpleExpr`]
/// projects as written, an [`AliasName`] as a bare reference to a name some
/// earlier clause bound, and a [`SelectExpr`] carries its own alias — which is
/// how an aliased item joins a list, though
/// [`column_as`](QuerySelect::column_as) chained after
/// [`select`](Select::select) reads better.
// [spec:pgorm:sem:query.build.modifiers+6]
pub trait SelectItem {
    /// The item as a select expression.
    fn into_select_expr(self) -> SelectExpr;
}

impl<C> SelectItem for C
where
    C: ColumnTrait,
{
    fn into_select_expr(self) -> SelectExpr {
        SelectExpr::new(self.select_as(self.into_expr()))
    }
}

impl SelectItem for SelectExpr {
    fn into_select_expr(self) -> SelectExpr {
        self
    }
}

impl SelectItem for SimpleExpr {
    fn into_select_expr(self) -> SelectExpr {
        SelectExpr::new(self)
    }
}

impl SelectItem for Expr {
    fn into_select_expr(self) -> SelectExpr {
        SelectExpr::new(self)
    }
}

impl SelectItem for AliasName {
    fn into_select_expr(self) -> SelectExpr {
        SelectExpr::new(Expr::col(self))
    }
}

/// A projection list, as [`select`](Select::select) takes one.
///
/// The shapes are the pipeline's ([`Pipeline::select`](crate::pipeline::Pipeline::select)):
/// a single item needs no wrapper, a homogeneous list is an array or a `Vec`,
/// and a mixed list — a column of one entity beside a column of another,
/// beside an alias token — is a tuple, because an array of two different
/// column enums has no element type.
///
/// ```
/// use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
///
/// assert_eq!(
///     cake::Entity::find()
///         .left_join_rel(cake::Relation::Fruit)
///         .select((cake::Column::Name, fruit::Column::Name))
///         .as_query()
///         .to_string(),
///     [
///         r#"SELECT "cake"."name", "fruit"."name" FROM "cake""#,
///         r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
///     ]
///     .join(" ")
/// );
/// ```
///
/// A list computed at run time is an iterator, which no tuple arity can cover;
/// that stays [`select_only`](Select::select_only) plus
/// [`columns`](QuerySelect::columns).
// [spec:pgorm:sem:query.build.modifiers+6]
pub trait SelectList {
    /// The items, in the order written.
    fn into_select_exprs(self) -> Vec<SelectExpr>;
}

impl<C> SelectList for C
where
    C: ColumnTrait,
{
    fn into_select_exprs(self) -> Vec<SelectExpr> {
        vec![self.into_select_expr()]
    }
}

macro_rules! impl_select_list_single {
    ( $item: ty ) => {
        impl SelectList for $item {
            fn into_select_exprs(self) -> Vec<SelectExpr> {
                vec![self.into_select_expr()]
            }
        }
    };
}

impl_select_list_single!(SelectExpr);
impl_select_list_single!(SimpleExpr);
impl_select_list_single!(Expr);
impl_select_list_single!(AliasName);

impl<T, const N: usize> SelectList for [T; N]
where
    T: SelectItem,
{
    fn into_select_exprs(self) -> Vec<SelectExpr> {
        self.into_iter().map(SelectItem::into_select_expr).collect()
    }
}

impl<T> SelectList for Vec<T>
where
    T: SelectItem,
{
    fn into_select_exprs(self) -> Vec<SelectExpr> {
        self.into_iter().map(SelectItem::into_select_expr).collect()
    }
}

macro_rules! impl_select_list_tuple {
    ( $( $name: ident ),+ ) => {
        impl< $( $name ),+ > SelectList for ( $( $name, )+ )
        where
            $( $name: SelectItem, )+
        {
            #[allow(non_snake_case)]
            fn into_select_exprs(self) -> Vec<SelectExpr> {
                let ( $( $name, )+ ) = self;
                vec![ $( $name.into_select_expr() ),+ ]
            }
        }
    };
}

impl_select_list_tuple!(A, B);
impl_select_list_tuple!(A, B, C);
impl_select_list_tuple!(A, B, C, D);
impl_select_list_tuple!(A, B, C, D, E);
impl_select_list_tuple!(A, B, C, D, E, F);
impl_select_list_tuple!(A, B, C, D, E, F, G);
impl_select_list_tuple!(A, B, C, D, E, F, G, H);
impl_select_list_tuple!(A, B, C, D, E, F, G, H, I);
impl_select_list_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_select_list_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_select_list_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

impl<E> Select<E>
where
    E: EntityTrait,
{
    // [spec:pgorm:sem:query.build.select-defaults]
    pub(crate) fn new() -> Self {
        Self {
            query: SelectStatement::new(),
            entity: PhantomData,
        }
        .prepare_select()
        .prepare_from()
    }

    fn prepare_select(mut self) -> Self {
        self.query.exprs(self.column_list());
        self
    }

    fn column_list(&self) -> Vec<SimpleExpr> {
        E::Column::iter()
            .map(|col| col.select_as(col.into_expr()))
            .collect()
    }

    fn prepare_from(mut self) -> Self {
        self.query.from(E::default().table_ref());
        self
    }

    /// Clear the selection list, moving to the projection-less
    /// [`SelectCustom<E>`] state.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .select_only()
    ///         .column(cake::Column::Name)
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."name" FROM "cake""#
    /// );
    /// ```
    ///
    /// A cleared select list has nothing to decode, so no execution path
    /// exists until a column or expression is re-added:
    ///
    /// ```compile_fail,E0599
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabaseConnection};
    /// # async fn example(db: &DatabaseConnection) -> Result<(), Error> {
    /// cake::Entity::find().select_only().all(db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Nor can a custom projection be combined with a second model — the
    /// `A_`/`B_` aliasing scheme assumes `E`'s own select list:
    ///
    /// ```compile_fail,E0599
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// cake::Entity::find()
    ///     .select_only()
    ///     .column(cake::Column::Name)
    ///     .find_also_related(fruit::Entity);
    /// ```
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select_only(mut self) -> SelectCustom<E> {
        self.query.clear_selects();
        SelectCustom {
            query: self.query,
            entity: PhantomData,
        }
    }

    /// Project exactly these columns, replacing `E`'s own select list.
    ///
    /// This is [`select_only`](Select::select_only) and
    /// [`columns`](QuerySelect::columns) in one call, and the same verb the
    /// pipeline projects with
    /// ([`Pipeline::select`](crate::pipeline::Pipeline::select)). See
    /// [`SelectList`] for the shapes a list can take.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .select([cake::Column::Id, cake::Column::Name])
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake""#
    /// );
    /// ```
    ///
    /// An aliased item chains after it, since the projection is now the
    /// caller's:
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .select(cake::Column::Name)
    ///         .column_as(cake::Column::Id.count(), "count")
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."name", COUNT("cake"."id") AS "count" FROM "cake""#
    /// );
    /// ```
    ///
    /// The rows are no longer `E::Model`'s shape, so — exactly as after
    /// `select_only` — the decode target has to be named:
    ///
    /// ```compile_fail,E0599
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabaseConnection};
    /// # async fn example(db: &DatabaseConnection) -> Result<(), Error> {
    /// cake::Entity::find().select(cake::Column::Name).all(db).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// An empty list projects nothing at all, which the execution-boundary
    /// guard catches for the same reason `columns([])` does.
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectProjected<E>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        SelectProjected {
            query: self.query,
            entity: PhantomData,
        }
    }
}

impl<E> SelectCustom<E>
where
    E: EntityTrait,
{
    /// Project exactly these items. See [`Select::select`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectProjected<E>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        SelectProjected {
            query: self.query,
            entity: PhantomData,
        }
    }
}

impl<E> SelectProjected<E>
where
    E: EntityTrait,
{
    /// Project exactly these items, discarding the projection built so far.
    /// See [`Select::select`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectProjected<E>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        self
    }

    /// Discard the projection built so far and start over from
    /// [`SelectCustom<E>`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select_only(mut self) -> SelectCustom<E> {
        self.query.clear_selects();
        SelectCustom {
            query: self.query,
            entity: PhantomData,
        }
    }
}

impl<E, F> SelectTwo<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Clear the selection list, moving to the projection-less
    /// [`SelectTwoCustom<E, F>`] state.
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select_only(mut self) -> SelectTwoCustom<E, F> {
        self.query.clear_selects();
        SelectTwoCustom {
            query: self.query,
            entity: PhantomData,
        }
    }

    /// Project exactly these items, replacing the two models' select list.
    /// See [`Select::select`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectTwoProjected<E, F>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        SelectTwoProjected {
            query: self.query,
            entity: PhantomData,
        }
    }
}

impl<E, F> SelectTwoCustom<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Project exactly these items. See [`Select::select`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectTwoProjected<E, F>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        SelectTwoProjected {
            query: self.query,
            entity: PhantomData,
        }
    }
}

impl<E, F> SelectTwoProjected<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Project exactly these items, discarding the projection built so far.
    /// See [`Select::select`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select<L>(mut self, items: L) -> SelectTwoProjected<E, F>
    where
        L: SelectList,
    {
        self.query.clear_selects();
        self.query.exprs(items.into_select_exprs());
        self
    }

    /// Discard the projection built so far and start over from
    /// [`SelectTwoCustom<E, F>`].
    // [spec:pgorm:sem:query.build.modifiers+6]
    pub fn select_only(mut self) -> SelectTwoCustom<E, F> {
        self.query.clear_selects();
        SelectTwoCustom {
            query: self.query,
            entity: PhantomData,
        }
    }
}

macro_rules! impl_query_trait {
    ( $selector: ident < $( $param: ident ),+ > ) => {
        impl< $( $param ),+ > QueryTrait for $selector < $( $param ),+ >
        where
            $( $param: EntityTrait, )+
        {
            type QueryStatement = SelectStatement;
            fn query(&mut self) -> &mut SelectStatement {
                &mut self.query
            }
            fn as_query(&self) -> &SelectStatement {
                &self.query
            }
            fn into_query(self) -> SelectStatement {
                self.query
            }
        }
    };
}

impl_query_trait!(Select<E>);
impl_query_trait!(SelectCustom<E>);
impl_query_trait!(SelectProjected<E>);
impl_query_trait!(SelectTwo<E, F>);
impl_query_trait!(SelectTwoCustom<E, F>);
impl_query_trait!(SelectTwoProjected<E, F>);
impl_query_trait!(SelectTwoMany<E, F>);
