use super::QueryResult;
use super::graph::GraphRow;
use super::select::ensure_select_list;
use crate::query::graph::qualified_pk_tiebreaks;
use crate::{
    ConnectionTrait, EntityTrait, Error, FromQueryResult, Identity, IdentityOf, IntoBoundary,
    IntoIdentity, PartialModelTrait, QueryOrder, QuerySelect, Select, SelectGraph, SelectModel,
    SelectProjected, SelectorTrait, Slot, SlotAt, Slots, error::query_err,
};
use pgorm_query::{
    Condition, DynIden, Expr, IntoValueTuple, Order, SelectStatement, SharedIden, SimpleExpr,
    Value, ValueTuple,
};
use tokio_postgres::types::{IsNull, Kind, ToSql, Type, to_sql_checked};
// use uuid::Uuid;
use std::marker::PhantomData;

// #[cfg(feature = "with-json")]
// use crate::JsonValue;

/// Which end of the ordered result set a cursor's row limit is taken from.
// [spec:pgorm:sem:exec.cursor.window+1]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The first `N` rows in the cursor's logical order.
    First(u64),
    /// The last `N` rows in the cursor's logical order.
    Last(u64),
}

impl Window {
    /// The row limit, whichever end the window is taken from.
    pub const fn rows(self) -> u64 {
        match self {
            Window::First(rows) | Window::Last(rows) => rows,
        }
    }
}

/// A cursor over a custom projection that has not named its decode target.
///
/// It is not a [`SelectorTrait`], so [`Cursor::all`] does not exist until
/// [`Cursor::into_model`] or [`Cursor::into_partial_model`] says what the rows
/// are.
// [spec:pgorm:sem:query.build.modifiers+7]
#[derive(Clone, Copy, Debug)]
pub struct SelectUndecoded;

/// Cursor pagination
// [spec:pgorm:def:exec.cursor+4]
#[derive(Debug, Clone)]
pub struct Cursor<S, K = ValueTuple> {
    query: SelectStatement,
    table: DynIden,
    order_columns: Identity,
    secondary_order_by: Vec<(DynIden, Identity)>,
    window: Option<Window>,
    before: Option<ValueTuple>,
    after: Option<ValueTuple>,
    sort_asc: bool,
    is_result_reversed: bool,
    phantom: PhantomData<(S, K)>,
}

// [spec:pgorm:sem:exec.cursor.keyset+3]
fn identity_arity(columns: &Identity) -> usize {
    match columns {
        Identity::Unary(..) => 1,
        Identity::Binary(..) => 2,
        Identity::Ternary(..) => 3,
        Identity::Many(columns) => columns.len(),
    }
}

// [spec:pgorm:sem:exec.cursor.keyset+3]
fn value_tuple_arity(values: &ValueTuple) -> usize {
    match values {
        ValueTuple::One(..) => 1,
        ValueTuple::Two(..) => 2,
        ValueTuple::Three(..) => 3,
        ValueTuple::Many(values) => values.len(),
    }
}

impl<S, K> Cursor<S, K> {
    /// Create a new cursor
    pub fn new<C>(query: SelectStatement, table: DynIden, order_columns: C) -> Self
    where
        C: IntoIdentity<ValueType = K>,
    {
        Self {
            query,
            table,
            order_columns: order_columns.into_identity(),
            window: None,
            after: None,
            before: None,
            sort_asc: true,
            is_result_reversed: false,
            phantom: PhantomData,
            secondary_order_by: Default::default(),
        }
    }

    /// Filter paginated result with corresponding column less than the input value
    pub fn before<V>(&mut self, values: V) -> &mut Self
    where
        V: IntoBoundary<K>,
    {
        self.before = Some(values.into_value_tuple());
        self
    }

    /// Filter paginated result with corresponding column greater than the input value
    pub fn after<V>(&mut self, values: V) -> &mut Self
    where
        V: IntoBoundary<K>,
    {
        self.after = Some(values.into_value_tuple());
        self
    }

    /// [`Cursor::before`] over the cursor's whole sort key, secondary order
    /// columns included.
    ///
    /// A joined cursor sorts by the other entity's primary key after its own
    /// order columns, so a page can end part-way through a run of rows sharing
    /// an order-column value. `before` cannot name that position — its arity is
    /// the order columns' — so resuming from it drops the rest of the run. This
    /// takes the full key of the row to resume from: the order-column values
    /// followed by one value per unary secondary order column, in the order
    /// they were installed.
    ///
    /// The extended arity is not the `K` the order columns fix, so it is
    /// checked when the query is composed rather than by the compiler.
    // [spec:pgorm:sem:exec.cursor.keyset+3]
    pub fn before_with<V>(&mut self, values: V) -> &mut Self
    where
        V: IntoValueTuple,
    {
        self.before = Some(values.into_value_tuple());
        self
    }

    /// [`Cursor::after`] over the cursor's whole sort key, secondary order
    /// columns included. See [`Cursor::before_with`].
    // [spec:pgorm:sem:exec.cursor.keyset+3]
    pub fn after_with<V>(&mut self, values: V) -> &mut Self
    where
        V: IntoValueTuple,
    {
        self.after = Some(values.into_value_tuple());
        self
    }

    /// The cursor's sort key, in comparison order: the order columns on the
    /// cursor's table, then each unary secondary order entry on its own table.
    ///
    /// Both `ORDER BY` and the boundary comparison read this, so the row order
    /// and the keyset predicate cannot disagree about what a page boundary is.
    // [spec:pgorm:sem:exec.cursor.keyset+3]
    fn keyset_columns(&self) -> Vec<(DynIden, DynIden)> {
        self.order_columns
            .clone()
            .into_iter()
            .map(|col| (SharedIden::clone(&self.table), col))
            .chain(
                self.secondary_order_by
                    .iter()
                    .filter_map(|(tbl, col)| match col {
                        Identity::Unary(c1) => {
                            Some((SharedIden::clone(tbl), SharedIden::clone(c1)))
                        }
                        _ => None,
                    }),
            )
            .collect()
    }

    // [spec:pgorm:sem:exec.cursor.keyset+3]
    fn apply_filters(&self, query: &mut SelectStatement) -> Result<(), Error> {
        let beyond = |col: Expr, v| if self.sort_asc { col.gt(v) } else { col.lt(v) };
        let short_of = |col: Expr, v| if self.sort_asc { col.lt(v) } else { col.gt(v) };

        if let Some(values) = self.after.clone() {
            let condition = self.apply_filter(values, beyond)?;
            query.cond_where(condition);
        }

        if let Some(values) = self.before.clone() {
            let condition = self.apply_filter(values, short_of)?;
            query.cond_where(condition);
        }

        Ok(())
    }

    // [spec:pgorm:sem:exec.cursor.keyset+3]
    fn apply_filter<F>(&self, values: ValueTuple, f: F) -> Result<Condition, Error>
    where
        F: Fn(Expr, Value) -> SimpleExpr,
    {
        let keyset = self.keyset_columns();
        let primary = identity_arity(&self.order_columns);
        let arity = value_tuple_arity(&values);

        let columns = if arity == primary {
            &keyset[..primary]
        } else if arity == keyset.len() {
            &keyset[..]
        } else {
            let expected = if keyset.len() > primary {
                format!("{primary} or {}", keyset.len())
            } else {
                primary.to_string()
            };
            return Err(query_err(format!(
                "cursor boundary of arity {arity} does not match {expected} order column(s)"
            )));
        };
        let values: Vec<Value> = values.into_iter().collect();

        // For a key of n columns the boundary is the row-value comparison
        // `(c1, ..., cn) ⋈ (v1, ..., vn)` written out as n disjuncts: the n-th
        // holds the first n-1 columns equal and compares the n-th, so
        //   (c1 = v1 AND ... AND cn ⋈ vn)
        //   OR (c1 = v1 AND ... AND c(n-1) ⋈ v(n-1))
        //   OR ... OR (c1 ⋈ v1)
        let col = |(tbl, col): &(DynIden, DynIden)| {
            Expr::col((SharedIden::clone(tbl), SharedIden::clone(col)))
        };
        Ok((1..=columns.len())
            .rev()
            .fold(Condition::any(), |disjunction, n| {
                disjunction.add(columns.iter().zip(values.iter()).take(n).enumerate().fold(
                    Condition::all(),
                    |conjunction, (i, (column, val))| {
                        let val = val.clone();
                        conjunction.add(if i + 1 == n {
                            f(col(column), val)
                        } else {
                            col(column).eq(val)
                        })
                    },
                ))
            }))
    }

    /// Use ascending sort order
    pub fn asc(&mut self) -> &mut Self {
        self.sort_asc = true;
        self
    }

    /// Use descending sort order
    pub fn desc(&mut self) -> &mut Self {
        self.sort_asc = false;
        self
    }

    /// Take the window of N rows from the near end of the cursor's order,
    /// replacing any window already set
    // [spec:pgorm:sem:exec.cursor.window+1]
    pub fn first(&mut self, num_rows: u64) -> &mut Self {
        self.window = Some(Window::First(num_rows));
        self
    }

    /// Take the window of N rows from the far end of the cursor's order,
    /// replacing any window already set
    // [spec:pgorm:sem:exec.cursor.window+1]
    pub fn last(&mut self, num_rows: u64) -> &mut Self {
        self.window = Some(Window::Last(num_rows));
        self
    }

    // [spec:pgorm:sem:exec.cursor.window+1]
    fn resolve_sort_order(&mut self) -> Order {
        let should_reverse_order = matches!(self.window, Some(Window::Last(_)));
        self.is_result_reversed = should_reverse_order;

        if (self.sort_asc && !should_reverse_order) || (!self.sort_asc && should_reverse_order) {
            Order::Asc
        } else {
            Order::Desc
        }
    }

    // [spec:pgorm:sem:exec.cursor.window+1]
    fn apply_limit(&self, query: &mut SelectStatement) {
        if let Some(window) = self.window {
            query.limit(window.rows());
        }
    }

    // [spec:pgorm:sem:exec.cursor.order+2]
    fn apply_order_by(&mut self, query: &mut SelectStatement) {
        query.clear_order_by();
        let ord = self.resolve_sort_order();

        for (tbl, col) in self.keyset_columns() {
            query.order_by((tbl, col), ord.clone());
        }
    }

    /// Build the query to execute: the caller's query with this cursor's
    /// window, order and boundary applied to a copy of it, so the cursor can be
    /// re-executed with a moved boundary or a flipped direction without the
    /// previous execution's clauses still on it.
    // [spec:pgorm:sem:exec.cursor.order+2]
    fn compose(&mut self) -> Result<SelectStatement, Error> {
        let mut query = self.query.clone();
        self.apply_limit(&mut query);
        self.apply_order_by(&mut query);
        self.apply_filters(&mut query)?;
        ensure_select_list(&query)?;
        Ok(query)
    }

    /// Construct a [Cursor] that fetch any custom struct
    pub fn into_model<M>(self) -> Cursor<SelectModel<M>, K>
    where
        M: FromQueryResult,
    {
        Cursor {
            query: self.query,
            table: self.table,
            order_columns: self.order_columns,
            window: self.window,
            after: self.after,
            before: self.before,
            sort_asc: self.sort_asc,
            is_result_reversed: self.is_result_reversed,
            phantom: PhantomData,
            secondary_order_by: self.secondary_order_by,
        }
    }

    /// Return a [`Cursor`] from `Self` that wraps a [`SelectModel`] decoding a
    /// [`PartialModelTrait`] type
    pub fn into_partial_model<M>(mut self) -> Cursor<SelectModel<M>, K>
    where
        M: PartialModelTrait,
    {
        self.query.clear_selects();
        M::select_cols(self).into_model::<M>()
    }

    /// Set the trailing order entries a joined read tiebreaks on, each
    /// qualified with the table it belongs to.
    pub fn set_secondary_order_by(&mut self, tbl_col: Vec<(DynIden, Identity)>) -> &mut Self {
        self.secondary_order_by = tbl_col;
        self
    }
}

impl<S, K> Cursor<S, K>
where
    S: SelectorTrait,
{
    /// Fetch the paginated result
    // [spec:pgorm:sem:exec.cursor.order+2]
    pub async fn all<C>(&mut self, db: &C) -> Result<Vec<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        let (stmt, values) = self.compose()?.build();
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();

        let rows = db.query_all(&stmt, &values).await?;
        let mut buffer = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            buffer.push(S::from_raw_query_result(QueryResult { row })?);
        }
        if self.is_result_reversed {
            buffer.reverse()
        }
        Ok(buffer)
    }
}

impl<S, K> QuerySelect for Cursor<S, K> {
    type QueryStatement = SelectStatement;
    type Projected = Self;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }

    fn into_projected(self) -> Self::Projected {
        self
    }
}

impl<S, K> QueryOrder for Cursor<S, K> {
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }
}

/// A trait for any type that can be turn into a cursor
// [spec:pgorm:def:exec.cursor+4]
pub trait CursorTrait {
    /// Select operation
    type Selector: SelectorTrait + Send + Sync;
}

impl<E, M> CursorTrait for Select<E>
where
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync,
{
    type Selector = SelectModel<M>;
}

impl<E, M> Select<E>
where
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync,
{
    /// Convert into a cursor
    ///
    /// The order columns fix the arity of every boundary later handed to
    /// [`Cursor::before`] / [`Cursor::after`], so a boundary of the wrong
    /// length is rejected before the query is built:
    ///
    /// ```
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake};
    /// cake::Entity::find().cursor_by(cake::Column::Id).after(1);
    /// cake::Entity::find()
    ///     .cursor_by((cake::Column::Id, cake::Column::Name))
    ///     .after((1, "cheese"));
    /// ```
    ///
    /// Two order columns and a single boundary value do not typecheck:
    ///
    /// ```compile_fail,E0277
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake};
    /// cake::Entity::find()
    ///     .cursor_by((cake::Column::Id, cake::Column::Name))
    ///     .after(1);
    /// ```
    ///
    /// Nor do one order column and a pair:
    ///
    /// ```compile_fail,E0277
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake};
    /// cake::Entity::find().cursor_by(cake::Column::Id).after((1, "cheese"));
    /// ```
    // [spec:pgorm:sem:exec.cursor.keyset+3/test]
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectModel<M>, C::ValueType>
    where
        C: IntoIdentity,
    {
        Cursor::new(self.query, SharedIden::new(E::default()), order_columns)
    }
}

/// The root entity's primary key as cursor tiebreaks, qualified with its own
/// table: what a graph ordered on one of its slots falls back to.
fn pk_tiebreaks<T: EntityTrait>() -> Vec<(DynIden, Identity)> {
    qualified_pk_tiebreaks::<T>(&SharedIden::new(T::default()))
}

/// A graph's rows are decoded by [`GraphRow`], so that is what its cursor
/// selects.
// [spec:pgorm:sem:query.graph.cursor]
impl<E, S> CursorTrait for SelectGraph<E, S>
where
    E: EntityTrait,
    GraphRow<E, S>: SelectorTrait + Send + Sync,
{
    type Selector = GraphRow<E, S>;
}

// [spec:pgorm:sem:query.graph.cursor]
impl<E, S> SelectGraph<E, S>
where
    E: EntityTrait,
    S: Slots,
{
    /// Convert into a cursor ordered by columns of the root, with every
    /// decoded slot's primary key installed as a trailing tiebreak.
    ///
    /// The declaration that fixed the joins fixes the tiebreak set: each slot
    /// contributes its primary-key columns as unary secondary order entries,
    /// qualified with the slot's *effective* identifier — its alias when the
    /// slot was declared `_as` — in declaration order. A graph that gains a
    /// slot gains its tiebreak, with no call site naming a column twice.
    ///
    /// A join that repeats a root row therefore still has a total order.
    /// Resume from a page that ended inside such a run with
    /// [`Cursor::after_with`] / [`Cursor::before_with`], which take those
    /// trailing key values too; [`Cursor::after`] alone compares the order
    /// columns and skips the rest of the run.
    ///
    /// ```
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// let mut cursor = cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .cursor_by(cake::Column::Id);
    ///
    /// // The whole key is the root's order column then the slot's primary key.
    /// cursor.after_with((1, 10)).first(2);
    /// ```
    ///
    /// An unmatched `Opt` slot's primary key is NULL, and nothing compares
    /// against NULL: a row whose tiebreak is null is reachable through the
    /// order-column boundary, not an extended one. Resuming with `after_with`
    /// from inside a matched run and then paging past the unmatched roots is
    /// the pattern that works.
    ///
    /// The order columns are the root's; a slot's column is
    /// [`cursor_by_on`](Self::cursor_by_on)'s job.
    ///
    /// ```compile_fail,E0277
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .cursor_by(fruit::Column::Name);
    /// ```
    // [spec:pgorm:sem:query.graph.cursor]
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<GraphRow<E, S>, C::ValueType>
    where
        C: IdentityOf<E>,
    {
        let tiebreaks = S::tiebreaks(self.slot_qualifiers(), 0);
        let table = self
            .qualifier(0)
            .unwrap_or_else(|| SharedIden::new(E::default()));

        let mut cursor = Cursor::new(self.query, table, order_columns);
        cursor.set_secondary_order_by(tiebreaks);
        cursor
    }

    /// Convert into a cursor ordered by columns of one slot, selected by its
    /// position at compile time: `1` is the first declared slot, as `s1_` is
    /// the prefix it projects under.
    ///
    /// The order columns are typed against that slot's entity and qualified
    /// with its effective identifier; the tiebreaks are the root's primary key
    /// first, then the remaining decoded slots' in declaration order.
    /// Everything else — the keyset, the boundaries, the direction — is
    /// [`cursor_by`](Self::cursor_by)'s, unchanged.
    ///
    /// ```
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// let mut cursor = cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .cursor_by_on::<1, _>(fruit::Column::Name);
    ///
    /// // The whole key is the slot's order column then the root's primary key.
    /// cursor.after_with(("Cherry", 1)).first(2);
    /// ```
    ///
    /// A position no slot occupies has no implementation, so it is a compile
    /// error rather than a silently mis-qualified column:
    ///
    /// ```compile_fail,E0277
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .cursor_by_on::<2, _>(fruit::Column::Name);
    /// ```
    ///
    /// So is ordering a slot by a column of another entity:
    ///
    /// ```compile_fail,E0277
    /// # use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    /// cake::Entity::graph()
    ///     .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
    ///     .cursor_by_on::<1, _>(cake::Column::Name);
    /// ```
    // [spec:pgorm:sem:query.graph.cursor]
    pub fn cursor_by_on<const I: usize, C>(
        self,
        order_columns: C,
    ) -> Cursor<GraphRow<E, S>, C::ValueType>
    where
        S: SlotAt<I>,
        C: IdentityOf<<<S as SlotAt<I>>::Slot as Slot>::Entity>,
    {
        let mut tiebreaks = pk_tiebreaks::<E>();
        tiebreaks.extend(S::tiebreaks(self.slot_qualifiers(), I));
        let table = self.qualifier(I).unwrap_or_else(|| {
            SharedIden::new(<<S as SlotAt<I>>::Slot as Slot>::Entity::default())
        });

        let mut cursor = Cursor::new(self.query, table, order_columns);
        cursor.set_secondary_order_by(tiebreaks);
        cursor
    }
}

// [spec:pgorm:sem:query.build.modifiers+7]
impl<E> SelectProjected<E>
where
    E: EntityTrait,
{
    /// Convert into a cursor whose rows have no decode target yet — the
    /// projection is the caller's, not `E::Model`'s, so
    /// [`Cursor::into_model`] or [`Cursor::into_partial_model`] has to name
    /// one before the cursor can be fetched.
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectUndecoded, C::ValueType>
    where
        C: IntoIdentity,
    {
        Cursor::new(self.query, SharedIden::new(E::default()), order_columns)
    }
}

#[path = "cursor_bind.rs"]
mod cursor_bind;
pub use cursor_bind::ValueHolder;

// [spec:pgorm:sem:query.graph.cursor/test]    a graph's cursor orders on the
// root and tiebreaks on every decoded slot's primary key — each qualified by
// the slot's effective identifier, in declaration order — while
// `cursor_by_on` orders on one slot chosen by position and tiebreaks on the
// root first; the keyset machinery underneath is untouched
#[cfg(test)]
mod tests {
    use super::*;
    use crate::RelationTrait;
    use crate::tests_cfg::{cake, cake_filling, filling, fruit};
    use pgorm_query::alias;
    use pretty_assertions::assert_eq;

    /// The statement a cursor executes: window, order and boundary applied.
    #[track_caller]
    fn composed<S, K>(cursor: &mut Cursor<S, K>) -> String {
        cursor.compose().expect("the cursor composes").to_string()
    }

    /// Everything from `ORDER BY` on, which is where the tiebreaks land.
    #[track_caller]
    fn order_by<S, K>(cursor: &mut Cursor<S, K>) -> String {
        let sql = composed(cursor);
        let (_, order) = sql.split_once("ORDER BY ").expect("an ORDER BY clause");
        order.to_owned()
    }

    #[test]
    fn every_decoded_slot_contributes_its_primary_key() {
        let mut cursor = cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .cursor_by(cake::Column::Id);

        assert_eq!(
            order_by(&mut cursor),
            r#""cake"."id" ASC, "fruit"."id" ASC"#
        );

        // A composite key contributes one entry per column, and a `via` hop —
        // which is never decoded — contributes none.
        let mut composite = cake::Entity::graph()
            .join_one::<cake_filling::Entity>(cake_filling::Relation::Cake.def().rev())
            .via(cake_filling::Relation::Filling.def())
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .cursor_by(cake::Column::Id);

        assert_eq!(
            order_by(&mut composite),
            [
                r#""cake"."id" ASC"#,
                r#""cake_filling"."cake_id" ASC"#,
                r#""cake_filling"."filling_id" ASC"#,
                r#""fruit"."id" ASC"#,
            ]
            .join(", ")
        );
    }

    #[test]
    fn a_slotless_graph_has_no_tiebreak() {
        let mut cursor = cake::Entity::graph().cursor_by(cake::Column::Id);

        assert_eq!(order_by(&mut cursor), r#""cake"."id" ASC"#);
    }

    #[test]
    fn an_aliased_slot_is_tiebroken_by_its_alias() {
        let mut cursor = cake::Entity::graph()
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("tropical"))
            .cursor_by(cake::Column::Id);

        assert_eq!(
            order_by(&mut cursor),
            r#""cake"."id" ASC, "tropical"."id" ASC"#
        );

        // The boundary reads the same key, so the alias qualifies the keyset
        // predicate too — the un-aliased table is not in the query at all.
        let sql = composed(cursor.after_with((1, 10)));
        assert!(
            sql.contains(r#""tropical"."id" > 10"#),
            "the extended boundary is qualified by the alias: {sql}"
        );
        assert!(
            !sql.contains(r#""fruit"."#),
            "the bare table is never named: {sql}"
        );
    }

    #[test]
    fn cursor_by_on_orders_on_the_named_slot() {
        let mut first = cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .related_maybe::<filling::Entity>()
            .cursor_by_on::<1, _>(fruit::Column::Name);

        // The root's primary key first, then the slots that are not the one
        // being ordered on, in declaration order.
        assert_eq!(
            order_by(&mut first),
            [
                r#""fruit"."name" ASC"#,
                r#""cake"."id" ASC"#,
                r#""filling"."id" ASC"#,
            ]
            .join(", ")
        );

        // The order columns are qualified by the slot's effective identifier
        // too, so an aliased slot is ordered on under its alias.
        let mut second = cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .join_maybe_as::<fruit::Entity>(cake::Relation::Fruit.def(), alias("tropical"))
            .cursor_by_on::<2, _>(fruit::Column::Name);

        assert_eq!(
            order_by(&mut second),
            [
                r#""tropical"."name" ASC"#,
                r#""cake"."id" ASC"#,
                r#""fruit"."id" ASC"#,
            ]
            .join(", ")
        );
    }

    #[test]
    fn the_boundary_arity_error_is_the_keyset_machinery() {
        let mut cursor = cake::Entity::graph()
            .join_maybe::<fruit::Entity>(cake::Relation::Fruit.def())
            .cursor_by(cake::Column::Id);

        let err = cursor
            .after_with((1, 2, 3))
            .compose()
            .expect_err("three values match neither arity");

        assert_eq!(
            err.to_string(),
            "Query Error: cursor boundary of arity 3 does not match 1 or 2 order column(s)"
        );
    }
}
