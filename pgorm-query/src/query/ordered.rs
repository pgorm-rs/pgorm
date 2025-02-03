use crate::{expr::*, types::*};

pub trait OrderedStatement {
    #[doc(hidden)]
    // Implementation for the trait.
    fn add_order_by(&mut self, order: OrderExpr) -> &mut Self;

    /// Clear order expressions
    fn clear_order_by(&mut self) -> &mut Self;

    /// Order by column.
    ///
    /// # Examples
    ///
    /// Order by ASC and DESC
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .column(Character::Character)
    ///         .from(Character::Table)
    ///         .and_where(Expr::col(Character::Id).gt(2))
    ///         .order_by(Character::Character, Order::Desc)
    ///         .order_by((Character::Table, Character::Id), Order::Asc)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT "character" FROM "character" WHERE "id" > 2 ORDER BY "character" DESC, "character"."id" ASC"#
    /// );
    /// ```
    ///
    /// Order by custom field ordering
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::select()
    ///     .columns([Character::Character])
    ///     .from(Character::Table)
    ///     .order_by(
    ///         Character::Id,
    ///         Order::Field(Values(vec![4.into(), 5.into(), 1.into(), 3.into()])),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(QueryBuilder),
    ///     [
    ///         r#"SELECT "character""#,
    ///         r#"FROM "character""#,
    ///         r#"ORDER BY CASE"#,
    ///         r#"WHEN "id"=4 THEN 0"#,
    ///         r#"WHEN "id"=5 THEN 1"#,
    ///         r#"WHEN "id"=1 THEN 2"#,
    ///         r#"WHEN "id"=3 THEN 3"#,
    ///         r#"ELSE 4 END"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    fn order_by<T>(&mut self, col: T, order: Order) -> &mut Self
    where
        T: IntoColumnRef,
    {
        self.add_order_by(OrderExpr {
            expr: SimpleExpr::Column(col.into_column_ref()),
            order,
            nulls: None,
        })
    }

    /// Order by [`SimpleExpr`].
    fn order_by_expr(&mut self, expr: SimpleExpr, order: Order) -> &mut Self {
        self.add_order_by(OrderExpr {
            expr,
            order,
            nulls: None,
        })
    }

    /// Order by custom string.
    fn order_by_customs<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: ToString,
        I: IntoIterator<Item = (T, Order)>,
    {
        cols.into_iter().for_each(|(c, order)| {
            self.add_order_by(OrderExpr {
                expr: SimpleExpr::Custom(c.to_string()),
                order,
                nulls: None,
            });
        });
        self
    }

    /// Order by vector of columns.
    fn order_by_columns<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: IntoColumnRef,
        I: IntoIterator<Item = (T, Order)>,
    {
        cols.into_iter().for_each(|(c, order)| {
            self.add_order_by(OrderExpr {
                expr: SimpleExpr::Column(c.into_column_ref()),
                order,
                nulls: None,
            });
        });
        self
    }

    /// Order by column with nulls order option.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .column(Character::Character)
    ///         .from(Character::Table)
    ///         .order_by_with_nulls(Character::Character, Order::Desc, NullOrdering::Last)
    ///         .order_by_with_nulls((Character::Table, Character::Id), Order::Asc, NullOrdering::First)
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT "character" FROM "character" ORDER BY "character" DESC NULLS LAST, "character"."id" ASC NULLS FIRST"#
    /// );
    /// ```
    fn order_by_with_nulls<T>(&mut self, col: T, order: Order, nulls: NullOrdering) -> &mut Self
    where
        T: IntoColumnRef,
    {
        self.add_order_by(OrderExpr {
            expr: SimpleExpr::Column(col.into_column_ref()),
            order,
            nulls: Some(nulls),
        })
    }

    /// Order by [`SimpleExpr`] with nulls order option.
    fn order_by_expr_with_nulls(
        &mut self,
        expr: SimpleExpr,
        order: Order,
        nulls: NullOrdering,
    ) -> &mut Self {
        self.add_order_by(OrderExpr {
            expr,
            order,
            nulls: Some(nulls),
        })
    }

    /// Order by custom string with nulls order option.
    fn order_by_customs_with_nulls<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: ToString,
        I: IntoIterator<Item = (T, Order, NullOrdering)>,
    {
        cols.into_iter().for_each(|(c, order, nulls)| {
            self.add_order_by(OrderExpr {
                expr: SimpleExpr::Custom(c.to_string()),
                order,
                nulls: Some(nulls),
            });
        });
        self
    }

    /// Order by vector of columns with nulls order option.
    fn order_by_columns_with_nulls<I, T>(&mut self, cols: I) -> &mut Self
    where
        T: IntoColumnRef,
        I: IntoIterator<Item = (T, Order, NullOrdering)>,
    {
        cols.into_iter().for_each(|(c, order, nulls)| {
            self.add_order_by(OrderExpr {
                expr: SimpleExpr::Column(c.into_column_ref()),
                order,
                nulls: Some(nulls),
            });
        });
        self
    }
}
