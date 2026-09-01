use crate::{
    ConnectionTrait, DbErr, EntityTrait, FromQueryResult, Identity, IdentityOf, IntoIdentity,
    PartialModelTrait, PrimaryKeyToColumn, QueryOrder, QuerySelect, Select, SelectModel, SelectTwo,
    SelectTwoModel, SelectorTrait,
};
use pgorm_query::{
    ArrayType, Condition, DynIden, Expr, IntoValueTuple, Order, QueryBuilder, SeaRc,
    SelectStatement, SimpleExpr, Value, ValueTuple,
};
use tokio_postgres::types::{IsNull, ToSql, Type, to_sql_checked};
// use uuid::Uuid;
use std::marker::PhantomData;
use strum::IntoEnumIterator as Iterable;

// #[cfg(feature = "with-json")]
// use crate::JsonValue;

/// Cursor pagination
// [spec:pgorm:def:exec.cursor]
#[derive(Debug, Clone)]
pub struct Cursor<S>
where
    S: SelectorTrait,
{
    query: SelectStatement,
    table: DynIden,
    order_columns: Identity,
    secondary_order_by: Vec<(DynIden, Identity)>,
    first: Option<u64>,
    last: Option<u64>,
    before: Option<ValueTuple>,
    after: Option<ValueTuple>,
    sort_asc: bool,
    is_result_reversed: bool,
    phantom: PhantomData<S>,
}

impl<S> Cursor<S>
where
    S: SelectorTrait,
{
    /// Create a new cursor
    pub fn new<C>(query: SelectStatement, table: DynIden, order_columns: C) -> Self
    where
        C: IntoIdentity,
    {
        Self {
            query,
            table,
            order_columns: order_columns.into_identity(),
            last: None,
            first: None,
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
        V: IntoValueTuple,
    {
        self.before = Some(values.into_value_tuple());
        self
    }

    /// Filter paginated result with corresponding column greater than the input value
    pub fn after<V>(&mut self, values: V) -> &mut Self
    where
        V: IntoValueTuple,
    {
        self.after = Some(values.into_value_tuple());
        self
    }

    // [spec:pgorm:sem:exec.cursor.keyset]
    fn apply_filters(&mut self) -> &mut Self {
        if let Some(values) = self.after.clone() {
            let condition = self.apply_filter(values, |c, v| {
                let exp = Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c)));
                if self.sort_asc { exp.gt(v) } else { exp.lt(v) }
            });
            self.query.cond_where(condition);
        }

        if let Some(values) = self.before.clone() {
            let condition = self.apply_filter(values, |c, v| {
                let exp = Expr::col((SeaRc::clone(&self.table), SeaRc::clone(c)));
                if self.sort_asc { exp.lt(v) } else { exp.gt(v) }
            });
            self.query.cond_where(condition);
        }

        self
    }

    // [spec:pgorm:sem:exec.cursor.keyset]
    fn apply_filter<F>(&self, values: ValueTuple, f: F) -> Condition
    where
        F: Fn(&DynIden, Value) -> SimpleExpr,
    {
        match (&self.order_columns, values) {
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
            _ => panic!("column arity mismatch"),
        }
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

    /// Limit result set to only first N rows in ascending order of the order by column
    // [spec:pgorm:sem:exec.cursor.window]
    pub fn first(&mut self, num_rows: u64) -> &mut Self {
        self.last = None;
        self.first = Some(num_rows);
        self
    }

    /// Limit result set to only last N rows in ascending order of the order by column
    pub fn last(&mut self, num_rows: u64) -> &mut Self {
        self.first = None;
        self.last = Some(num_rows);
        self
    }

    // [spec:pgorm:sem:exec.cursor.window]
    fn resolve_sort_order(&mut self) -> Order {
        let should_reverse_order = self.last.is_some();
        self.is_result_reversed = should_reverse_order;

        if (self.sort_asc && !should_reverse_order) || (!self.sort_asc && should_reverse_order) {
            Order::Asc
        } else {
            Order::Desc
        }
    }

    fn apply_limit(&mut self) -> &mut Self {
        if let Some(num_rows) = self.first {
            self.query.limit(num_rows);
        } else if let Some(num_rows) = self.last {
            self.query.limit(num_rows);
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

    /// Fetch the paginated result
    // [spec:pgorm:sem:exec.cursor.order]
    pub async fn all<C>(&mut self, db: &C) -> Result<Vec<S::Item>, DbErr>
    where
        C: ConnectionTrait,
    {
        self.apply_limit();
        self.apply_order_by();
        self.apply_filters();

        let (stmt, values) = self.query.build(QueryBuilder);
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| &*x as _)
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

    /// Construct a [Cursor] that fetch any custom struct
    pub fn into_model<M>(self) -> Cursor<SelectModel<M>>
    where
        M: FromQueryResult,
    {
        Cursor {
            query: self.query,
            table: self.table,
            order_columns: self.order_columns,
            last: self.last,
            first: self.first,
            after: self.after,
            before: self.before,
            sort_asc: self.sort_asc,
            is_result_reversed: self.is_result_reversed,
            phantom: PhantomData,
            secondary_order_by: self.secondary_order_by,
        }
    }

    /// Return a [Selector] from `Self` that wraps a [SelectModel] with a [PartialModel](PartialModelTrait)
    pub fn into_partial_model<M>(self) -> Cursor<SelectModel<M>>
    where
        M: PartialModelTrait,
    {
        M::select_cols(QuerySelect::select_only(self)).into_model::<M>()
    }

    /// Set the cursor ordering for another table when dealing with SelectTwo
    pub fn set_secondary_order_by(&mut self, tbl_col: Vec<(DynIden, Identity)>) -> &mut Self {
        self.secondary_order_by = tbl_col;
        self
    }
}

impl<S> QuerySelect for Cursor<S>
where
    S: SelectorTrait,
{
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }
}

impl<S> QueryOrder for Cursor<S>
where
    S: SelectorTrait,
{
    type QueryStatement = SelectStatement;

    fn query(&mut self) -> &mut SelectStatement {
        &mut self.query
    }
}

/// A trait for any type that can be turn into a cursor
// [spec:pgorm:def:exec.cursor]
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
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectModel<M>>
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
    pub fn cursor_by<C>(self, order_columns: C) -> Cursor<SelectTwoModel<M, N>>
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
        let mut cursor = Cursor::new(
            self.query,
            SeaRc::new(E::default()),
            order_columns.identity_of(),
        );
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }

    /// Convert into a cursor using column of second entity
    pub fn cursor_by_other<C>(self, order_columns: C) -> Cursor<SelectTwoModel<M, N>>
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
        let mut cursor = Cursor::new(
            self.query,
            SeaRc::new(F::default()),
            order_columns.identity_of(),
        );
        cursor.set_secondary_order_by(primary_keys);
        cursor
    }
}

// [spec:pgorm:def:exec.cursor.binding]
pub struct ValueHolder(pub Value);

impl std::fmt::Debug for ValueHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

use bytes::BytesMut;

use super::QueryResult;

#[inline(always)]
fn accepts<T: ToSql>(input: T, ty: &Type) -> bool {
    T::accepts(ty)
}

// [spec:pgorm:def:exec.cursor.binding]
impl ToSql for ValueHolder {
    // [spec:pgorm:req:exec.cursor.binding-gaps]
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
            Value::TinyInt(x) => x.to_sql(ty, out),
            Value::SmallInt(x) => x.to_sql(ty, out),
            Value::Int(x) => x.to_sql(ty, out),
            Value::BigInt(x) => x.to_sql(ty, out),
            Value::TinyUnsigned(_x) => unimplemented!("u8 not supported"), //x.as_ref().map(|x| (*x).to_sql(ty, out))
            Value::SmallUnsigned(_x) => unimplemented!("u16 not supported"), // x.map(|x| x as _).to_sql(ty, out),
            Value::Unsigned(x) => x.to_sql(ty, out),
            Value::BigUnsigned(x) => {
                //unimplemented!("ToSql: {x:?}, {ty:?}, u64 not supported"), // x.map(|x| x as _).to_sql(ty, out),
                x.map(|x| x as i64).to_sql(ty, out)
            }
            Value::Float(x) => x.to_sql(ty, out),
            Value::Double(x) => x.to_sql(ty, out),
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
            Value::Vector(vector) => todo!(),
            Value::IpNetwork(ip_network) => todo!(),
            Value::MacAddress(mac_address) => todo!(),
        }
    }

    fn accepts(_ty: &Type) -> bool
    where
        Self: Sized,
    {
        true
    }

    to_sql_checked!();
}
