use super::select::ensure_select_list;
use crate::{
    ConnectionTrait, EntityTrait, Error, FromQueryResult, Identity, IdentityOf, IntoBoundary,
    IntoIdentity, PartialModelTrait, PrimaryKeyToColumn, QueryOrder, QuerySelect, Select,
    SelectModel, SelectProjected, SelectTwo, SelectTwoModel, SelectTwoProjected, SelectorTrait,
    error::query_err,
};
use pgorm_query::{
    Condition, DynIden, Expr, Order, SeaRc, SelectStatement, SimpleExpr, Value, ValueTuple,
};
use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};
// use uuid::Uuid;
use std::marker::PhantomData;
use strum::IntoEnumIterator as Iterable;

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
// [spec:pgorm:sem:query.build.modifiers+6]
#[derive(Clone, Copy, Debug)]
pub struct SelectUndecoded;

/// Cursor pagination
// [spec:pgorm:def:exec.cursor+2]
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

// [spec:pgorm:sem:exec.cursor.keyset+2]
fn identity_arity(columns: &Identity) -> usize {
    match columns {
        Identity::Unary(..) => 1,
        Identity::Binary(..) => 2,
        Identity::Ternary(..) => 3,
        Identity::Many(columns) => columns.len(),
    }
}

// [spec:pgorm:sem:exec.cursor.keyset+2]
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

    // [spec:pgorm:sem:exec.cursor.keyset+2]
    fn apply_filters(&mut self) -> Result<(), Error> {
        if let Some(values) = self.after.clone() {
            let condition = self.apply_filter(values, |c, v| {
                let exp = Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c)));
                if self.sort_asc { exp.gt(v) } else { exp.lt(v) }
            })?;
            self.query.cond_where(condition);
        }

        if let Some(values) = self.before.clone() {
            let condition = self.apply_filter(values, |c, v| {
                let exp = Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c)));
                if self.sort_asc { exp.lt(v) } else { exp.gt(v) }
            })?;
            self.query.cond_where(condition);
        }

        Ok(())
    }

    // [spec:pgorm:sem:exec.cursor.keyset+2]
    fn apply_filter<F>(&self, values: ValueTuple, f: F) -> Result<Condition, Error>
    where
        F: Fn(&DynIden, Value) -> SimpleExpr,
    {
        let condition = match (&self.order_columns, values) {
            (Identity::Unary(c1), ValueTuple::One(v1)) => Condition::all().add(f(c1, v1)),
            (Identity::Binary(c1, c2), ValueTuple::Two(v1, v2)) => Condition::any()
                .add(
                    Condition::all()
                        .add(
                            Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c1))).eq(v1.clone()),
                        )
                        .add(f(c2, v2)),
                )
                .add(f(c1, v1)),
            (Identity::Ternary(c1, c2, c3), ValueTuple::Three(v1, v2, v3)) => Condition::any()
                .add(
                    Condition::all()
                        .add(
                            Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c1))).eq(v1.clone()),
                        )
                        .add(
                            Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c2))).eq(v2.clone()),
                        )
                        .add(f(c3, v3)),
                )
                .add(
                    Condition::all()
                        .add(
                            Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c1))).eq(v1.clone()),
                        )
                        .add(f(c2, v2)),
                )
                .add(f(c1, v1)),
            (Identity::Many(col_vec), ValueTuple::Many(val_vec))
                if col_vec.len() == val_vec.len() =>
            {
                // The length of `col_vec` and `val_vec` should be equal and is denoted by "n".
                //
                // The elements of `col_vec` and `val_vec` are denoted by:
                //   - `col_vec`: "col_1", "col_2", ..., "col_n-1", "col_n"
                //   - `val_vec`: "val_1", "val_2", ..., "val_n-1", "val_n"
                //
                // The general form of the where condition should have "n" number of inner-AND-condition chained by an outer-OR-condition.
                // The "n"-th inner-AND-condition should have exactly "n" number of column value expressions,
                // to construct the expression we take the first "n" number of column and value from the respected vector.
                //   - if it's not the last element, then we construct a "col_1 = val_1" equal expression
                //   - otherwise, for the last element, we should construct a "col_n > val_n" greater than or "col_n < val_n" less than expression.
                // i.e.
                // WHERE
                //   (col_1 = val_1 AND col_2 = val_2 AND ... AND col_n > val_n)
                //   OR (col_1 = val_1 AND col_2 = val_2 AND ... AND col_n-1 > val_n-1)
                //   OR (col_1 = val_1 AND col_2 = val_2 AND ... AND col_n-2 > val_n-2)
                //   OR ...
                //   OR (col_1 = val_1 AND col_2 > val_2)
                //   OR (col_1 > val_1)

                // Counting from 1 to "n" (inclusive) but in reverse, i.e. n, n-1, ..., 2, 1
                (1..=col_vec.len())
                    .rev()
                    .fold(Condition::any(), |cond_any, n| {
                        // Construct the inner-AND-condition
                        let inner_cond_all =
                            // Take the first "n" elements from the column and value vector respectively
                            col_vec.iter().zip(val_vec.iter()).enumerate().take(n).fold(
                                Condition::all(),
                                |inner_cond_all, (i, (col, val))| {
                                    let val = val.clone();
                                    // Construct a equal expression,
                                    // except for the last one being greater than or less than expression
                                    let expr = if i != (n - 1) {
                                        Expr::col((SeaRc::clone(&self.table), SeaRc::clone(col)))
                                            .eq(val)
                                    } else {
                                        f(col, val)
                                    };
                                    // Chain it with AND operator
                                    inner_cond_all.add(expr)
                                },
                            );
                        // Chain inner-AND-condition with OR operator
                        cond_any.add(inner_cond_all)
                    })
            }
            (columns, values) => {
                return Err(query_err(format!(
                    "cursor boundary of arity {} does not match {} order column(s)",
                    value_tuple_arity(&values),
                    identity_arity(columns),
                )));
            }
        };

        Ok(condition)
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
    fn apply_limit(&mut self) -> &mut Self {
        if let Some(window) = self.window {
            self.query.limit(window.rows());
        }

        self
    }

    // [spec:pgorm:sem:exec.cursor.order]
    fn apply_order_by(&mut self) -> &mut Self {
        self.query.clear_order_by();
        let ord = self.resolve_sort_order();

        let query = &mut self.query;
        let order = |query: &mut SelectStatement, col| {
            query.order_by((SeaRc::clone(&self.table), SeaRc::clone(col)), ord.clone());
        };
        match &self.order_columns {
            Identity::Unary(c1) => {
                order(query, c1);
            }
            Identity::Binary(c1, c2) => {
                order(query, c1);
                order(query, c2);
            }
            Identity::Ternary(c1, c2, c3) => {
                order(query, c1);
                order(query, c2);
                order(query, c3);
            }
            Identity::Many(vec) => {
                for col in vec.iter() {
                    order(query, col);
                }
            }
        }

        for (tbl, col) in self.secondary_order_by.iter().cloned() {
            if let Identity::Unary(c1) = col {
                query.order_by((tbl, c1), ord.clone());
            };
        }

        self
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

    /// Set the cursor ordering for another table when dealing with SelectTwo
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
    // [spec:pgorm:sem:exec.cursor.order]
    pub async fn all<C>(&mut self, db: &C) -> Result<Vec<S::Item>, Error>
    where
        C: ConnectionTrait,
    {
        self.apply_limit();
        self.apply_order_by();
        self.apply_filters()?;
        ensure_select_list(&self.query)?;

        let (stmt, values) = self.query.build();
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
// [spec:pgorm:def:exec.cursor+2]
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
    // [spec:pgorm:sem:exec.cursor.keyset+2/test]
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectModel<M>, C::ValueType>
    where
        C: IntoIdentity,
    {
        Cursor::new(self.query, SeaRc::new(E::default()), order_columns)
    }
}

impl<E, F, M, N> CursorTrait for SelectTwo<E, F>
where
    E: EntityTrait<Model = M>,
    F: EntityTrait<Model = N>,
    M: FromQueryResult + Sized + Send + Sync,
    N: FromQueryResult + Sized + Send + Sync,
{
    type Selector = SelectTwoModel<M, N>;
}

impl<E, F, M, N> SelectTwo<E, F>
where
    E: EntityTrait<Model = M>,
    F: EntityTrait<Model = N>,
    M: FromQueryResult + Sized + Send + Sync,
    N: FromQueryResult + Sized + Send + Sync,
{
    /// Convert into a cursor using column of first entity
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectTwoModel<M, N>, C::ValueType>
    where
        C: IdentityOf<E>,
    {
        let primary_keys: Vec<(DynIden, Identity)> = <F::PrimaryKey as Iterable>::iter()
            .map(|pk| {
                (
                    SeaRc::new(F::default()),
                    Identity::Unary(SeaRc::new(pk.into_column())),
                )
            })
            .collect();
        let mut cursor = Cursor::new(self.query, SeaRc::new(E::default()), order_columns);
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }

    /// Convert into a cursor using column of second entity
    pub fn cursor_by_other<C>(self, order_columns: C) -> Cursor<SelectTwoModel<M, N>, C::ValueType>
    where
        C: IdentityOf<F>,
    {
        let primary_keys: Vec<(DynIden, Identity)> = <E::PrimaryKey as Iterable>::iter()
            .map(|pk| {
                (
                    SeaRc::new(E::default()),
                    Identity::Unary(SeaRc::new(pk.into_column())),
                )
            })
            .collect();
        let mut cursor = Cursor::new(self.query, SeaRc::new(F::default()), order_columns);
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }
}

// [spec:pgorm:sem:query.build.modifiers+6]
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
        Cursor::new(self.query, SeaRc::new(E::default()), order_columns)
    }
}

// [spec:pgorm:sem:query.build.modifiers+6]
impl<E, F> SelectTwoProjected<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    /// Convert into a cursor using a column of the first entity. As with
    /// [`SelectProjected::cursor_by`], the decode target is still open.
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectUndecoded, C::ValueType>
    where
        C: IdentityOf<E>,
    {
        let primary_keys: Vec<(DynIden, Identity)> = <F::PrimaryKey as Iterable>::iter()
            .map(|pk| {
                (
                    SeaRc::new(F::default()),
                    Identity::Unary(SeaRc::new(pk.into_column())),
                )
            })
            .collect();
        let mut cursor = Cursor::new(self.query, SeaRc::new(E::default()), order_columns);
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }

    /// Convert into a cursor using a column of the second entity.
    pub fn cursor_by_other<C>(self, order_columns: C) -> Cursor<SelectUndecoded, C::ValueType>
    where
        C: IdentityOf<F>,
    {
        let primary_keys: Vec<(DynIden, Identity)> = <E::PrimaryKey as Iterable>::iter()
            .map(|pk| {
                (
                    SeaRc::new(E::default()),
                    Identity::Unary(SeaRc::new(pk.into_column())),
                )
            })
            .collect();
        let mut cursor = Cursor::new(self.query, SeaRc::new(F::default()), order_columns);
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }
}

/// Adapter binding a [`Value`] as a query parameter, converting it to the
/// Postgres type inferred for that placeholder.
// [spec:pgorm:def:exec.cursor.binding+2]
pub struct ValueHolder(pub Value);

impl std::fmt::Debug for ValueHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

use bytes::BytesMut;
use rust_decimal::Decimal;

use super::QueryResult;

type BindResult = Result<IsNull, Box<dyn std::error::Error + Sync + Send>>;

fn out_of_range(
    value: impl std::fmt::Display,
    ty: &Type,
) -> Box<dyn std::error::Error + Sync + Send> {
    format!("value `{value}` is out of range for Postgres type `{ty}`").into()
}

/// Bind an integer against the numeric type Postgres inferred for the
/// placeholder, or `None` when the inferred type is outside the numeric family
/// and the value should be written in its own format.
// [spec:pgorm:req:exec.cursor.binding-coerce]
fn integer_to_sql(value: i64, ty: &Type, out: &mut BytesMut) -> Option<BindResult> {
    let result = if *ty == Type::INT2 {
        match i16::try_from(value) {
            Ok(value) => value.to_sql(ty, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *ty == Type::INT4 {
        match i32::try_from(value) {
            Ok(value) => value.to_sql(ty, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *ty == Type::INT8 {
        value.to_sql(ty, out)
    } else if *ty == Type::FLOAT4 {
        (value as f32).to_sql(ty, out)
    } else if *ty == Type::FLOAT8 {
        (value as f64).to_sql(ty, out)
    } else if *ty == Type::NUMERIC {
        Decimal::from(value).to_sql(ty, out)
    } else {
        return None;
    };
    Some(result)
}

/// The floating-point counterpart of [`integer_to_sql`]. Narrowing to `float4`
/// rounds the way Postgres' own `float8 -> float4` cast does, but a conversion
/// that would silently drop the fractional part or overflow is an error rather
/// than a lie.
// [spec:pgorm:req:exec.cursor.binding-coerce]
fn float_to_sql(value: f64, ty: &Type, out: &mut BytesMut) -> Option<BindResult> {
    let result = if *ty == Type::FLOAT4 {
        let narrowed = value as f32;
        if value.is_finite() && !narrowed.is_finite() {
            Err(out_of_range(value, ty))
        } else {
            narrowed.to_sql(ty, out)
        }
    } else if *ty == Type::FLOAT8 {
        value.to_sql(ty, out)
    } else if *ty == Type::NUMERIC {
        match Decimal::try_from(value) {
            Ok(value) => value.to_sql(ty, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *ty == Type::INT2 || *ty == Type::INT4 || *ty == Type::INT8 {
        Err(format!(
            "cannot bind floating-point value `{value}` to Postgres type `{ty}` without loss"
        )
        .into())
    } else {
        return None;
    };
    Some(result)
}

// [spec:pgorm:req:exec.cursor.binding-coerce]
fn bind_integer<T>(value: Option<T>, ty: &Type, out: &mut BytesMut) -> BindResult
where
    T: ToSql + Copy + Into<i64>,
{
    match value {
        None => Ok(IsNull::Yes),
        Some(value) => match integer_to_sql(value.into(), ty, out) {
            Some(result) => result,
            None => value.to_sql(ty, out),
        },
    }
}

// [spec:pgorm:req:exec.cursor.binding-coerce]
fn bind_float<T>(value: Option<T>, ty: &Type, out: &mut BytesMut) -> BindResult
where
    T: ToSql + Copy + Into<f64>,
{
    match value {
        None => Ok(IsNull::Yes),
        Some(value) => match float_to_sql(value.into(), ty, out) {
            Some(result) => result,
            None => value.to_sql(ty, out),
        },
    }
}

// [spec:pgorm:def:exec.cursor.binding+2]
impl ToSql for ValueHolder {
    // [spec:pgorm:req:exec.cursor.binding-gaps+2]
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        match &self.0 {
            Value::Bool(x) => x.to_sql(ty, out),
            Value::TinyInt(x) => bind_integer(*x, ty, out),
            Value::SmallInt(x) => bind_integer(*x, ty, out),
            Value::Int(x) => bind_integer(*x, ty, out),
            Value::BigInt(x) => bind_integer(*x, ty, out),
            Value::Unsigned(x) => bind_integer(*x, ty, out),
            Value::BigUnsigned(x) => bind_integer(x.map(|x| x as i64), ty, out),
            Value::Float(x) => bind_float(*x, ty, out),
            Value::Double(x) => bind_float(*x, ty, out),
            Value::String(x) => match x.as_ref() {
                Some(x) => x.to_sql(ty, out),
                None => Ok(IsNull::Yes),
            },
            Value::Char(x) => x.map(|x| x.to_string()).to_sql(ty, out),
            Value::Bytes(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::Json(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoDate(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoTime(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoDateTime(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoDateTimeUtc(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoDateTimeLocal(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::ChronoDateTimeWithTimeZone(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            // Value::TimeDate(x) => x.map(|x| &**x).to_sql(ty, out),
            // Value::TimeTime(x) => x.map(|x| &**x).to_sql(ty, out),
            // Value::TimeDateTime(x) => x.map(|x| &**x).to_sql(ty, out),
            // Value::TimeDateTimeWithTimeZone(x) => x.map(|x| &**x).to_sql(ty, out),
            Value::Uuid(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)), // x.map(|x| &*x).to_sql(ty, out),
            Value::Decimal(x) => x.as_ref().map(|x| &**x).to_sql(ty, out),
            // Value::BigDecimal(x) => x.map(|x| &**x).to_sql(ty, out),
            Value::Array(_, Some(x)) => x
                .iter()
                .map(|x| ValueHolder(x.clone()))
                .collect::<Vec<_>>()
                .to_sql(ty, out),
            Value::Array(_, None) => Ok(IsNull::Yes),
            Value::Vector(x) => x
                .as_ref()
                .map(|x| x.to_sql(ty, out))
                .unwrap_or(Ok(IsNull::Yes)),
            Value::IpNetwork(x) => match x.as_ref() {
                Some(x) => {
                    postgres_protocol::types::inet_to_sql(x.ip(), x.prefix(), out);
                    Ok(IsNull::No)
                }
                None => Ok(IsNull::Yes),
            },
            Value::MacAddress(x) => match x.as_ref() {
                Some(x) => {
                    postgres_protocol::types::macaddr_to_sql(x.bytes(), out);
                    Ok(IsNull::No)
                }
                None => Ok(IsNull::Yes),
            },
        }
    }

    /// Every Postgres type is accepted: a `Value` carries no target type, and
    /// the types it legitimately binds against include ones no client-side
    /// check can enumerate (enums, domains, and every other type whose binary
    /// format is the text of its label). `to_sql` converts within the numeric
    /// family and errors on conversions it cannot make exactly; every other
    /// mismatch is still reported by the server rather than here.
    // [spec:pgorm:req:exec.cursor.binding-gaps+2]
    fn accepts(_ty: &Type) -> bool
    where
        Self: Sized,
    {
        true
    }

    to_sql_checked!();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode(value: Value) -> Option<Vec<u8>> {
        encode_as(value, &Type::BYTEA).unwrap()
    }

    fn encode_as(value: Value, ty: &Type) -> Result<Option<Vec<u8>>, String> {
        let mut out = BytesMut::new();
        match ValueHolder(value).to_sql(ty, &mut out) {
            Ok(IsNull::Yes) => Ok(None),
            Ok(IsNull::No) => Ok(Some(out.to_vec())),
            Err(err) => Err(err.to_string()),
        }
    }

    fn bytes(value: Value, ty: &Type) -> Vec<u8> {
        encode_as(value, ty).unwrap().unwrap()
    }

    fn error(value: Value, ty: &Type) -> String {
        encode_as(value, ty).unwrap_err()
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn binds_integer_as_float() {
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::FLOAT8),
            2.0f64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::FLOAT4),
            2.0f32.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::BigInt(Some(-3)), &Type::FLOAT8),
            (-3.0f64).to_be_bytes()
        );
        assert_eq!(encode_as(Value::Int(None), &Type::FLOAT8), Ok(None));
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn binds_integer_across_widths() {
        assert_eq!(
            bytes(Value::BigInt(Some(300)), &Type::INT2),
            300i16.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::SmallInt(Some(7)), &Type::INT8),
            7i64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::TinyInt(Some(-1)), &Type::INT4),
            (-1i32).to_be_bytes()
        );
        assert_eq!(
            bytes(Value::BigUnsigned(Some(9)), &Type::INT2),
            9i16.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Unsigned(Some(9)), &Type::OID),
            9u32.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::TinyInt(Some(65)), &Type::CHAR),
            65i8.to_be_bytes()
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn binds_integer_as_numeric() {
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::NUMERIC),
            vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 2]
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn rejects_out_of_range_integer() {
        assert_eq!(
            error(Value::BigInt(Some(i64::from(i32::MAX) + 1)), &Type::INT4),
            "value `2147483648` is out of range for Postgres type `int4`"
        );
        assert_eq!(
            error(Value::Int(Some(40_000)), &Type::INT2),
            "value `40000` is out of range for Postgres type `int2`"
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn binds_float_across_widths() {
        assert_eq!(
            bytes(Value::Float(Some(1.5)), &Type::FLOAT8),
            1.5f64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Double(Some(1.5)), &Type::FLOAT4),
            1.5f32.to_be_bytes()
        );
        assert!(
            error(Value::Double(Some(1e300)), &Type::FLOAT4)
                .ends_with("is out of range for Postgres type `float4`")
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce/test]
    #[test]
    fn rejects_float_bound_to_integer() {
        assert_eq!(
            error(Value::Double(Some(1.5)), &Type::INT8),
            "cannot bind floating-point value `1.5` to Postgres type `int8` without loss"
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+2/test]
    #[test]
    fn binds_vector() {
        assert_eq!(
            encode(Value::Vector(Some(Box::new(pgvector::Vector::from(vec![
                1.0f32, 2.0
            ]))))),
            Some(
                [
                    &[0, 2, 0, 0][..],
                    &1.0f32.to_be_bytes()[..],
                    &2.0f32.to_be_bytes()[..],
                ]
                .concat()
            )
        );
        assert_eq!(encode(Value::Vector(None)), None);
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+2/test]
    #[test]
    fn binds_ip_network() {
        assert_eq!(
            encode(Value::IpNetwork(Some(Box::new(
                "10.0.0.1/24".parse().unwrap()
            )))),
            Some(vec![2, 24, 0, 4, 10, 0, 0, 1])
        );
        assert_eq!(encode(Value::IpNetwork(None)), None);
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+2/test]
    #[test]
    fn binds_mac_address() {
        assert_eq!(
            encode(Value::MacAddress(Some(Box::new(
                "00:11:22:33:44:55".parse().unwrap()
            )))),
            Some(vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55])
        );
        assert_eq!(encode(Value::MacAddress(None)), None);
    }
}
