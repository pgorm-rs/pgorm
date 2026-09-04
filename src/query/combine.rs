use crate::{
    ColumnTrait, EntityTrait, IdenStr, Iterable, QueryTrait, Select, SelectTwo, SelectTwoMany,
};
use core::marker::PhantomData;
use pgorm_query::{
    Alias, ColumnRef, DynIden, Iden, Order, SeaRc, SelectExpr, SelectStatement, SimpleExpr,
};

macro_rules! select_def {
    ( $ident: ident, $str: expr ) => {
        /// Implements the traits [Iden] and [IdenStr] for a type
        #[derive(Debug, Clone, Copy)]
        pub struct $ident;

        impl Iden for $ident {
            fn unquoted(&self, s: &mut dyn std::fmt::Write) {
                write!(s, "{}", self.as_str()).expect("write to sql sink");
            }
        }

        impl IdenStr for $ident {
            fn as_str(&self) -> &str {
                $str
            }
        }
    };
}

select_def!(SelectA, "A_");
select_def!(SelectB, "B_");

/// The identifier an unaliased column reference can be renamed after; an
/// asterisk names no single column and so has none.
// [spec:pgorm:sem:query.build.combine+1]
fn named_column(col_ref: &ColumnRef) -> Option<&DynIden> {
    match col_ref {
        ColumnRef::Column(col)
        | ColumnRef::TableColumn(_, col)
        | ColumnRef::SchemaTableColumn(_, _, col) => Some(col),
        ColumnRef::Asterisk | ColumnRef::TableAsterisk(_) => None,
    }
}

// [spec:pgorm:sem:query.build.combine+1]
impl<E> Select<E>
where
    E: EntityTrait,
{
    pub(crate) fn apply_alias(mut self, pre: &str) -> Self {
        self.query().exprs_mut_for_each(|sel| {
            match &sel.alias {
                Some(alias) => {
                    let alias = format!("{}{}", pre, alias.to_string().as_str());
                    sel.alias = Some(SeaRc::new(Alias::new(alias)));
                }
                None => {
                    // An entry with neither an alias nor a column to name
                    // itself after has no `A_`/`B_` name to take, so it is
                    // left as written: it belongs to neither model, and
                    // neither model's decode looks for it.
                    let col = match sel.expr() {
                        SimpleExpr::Column(col_ref) => named_column(col_ref),
                        SimpleExpr::AsEnum(_, simple_expr) => match simple_expr.as_ref() {
                            SimpleExpr::Column(col_ref) => named_column(col_ref),
                            _ => None,
                        },
                        _ => None,
                    };
                    if let Some(col) = col {
                        let alias = format!("{}{}", pre, col.to_string().as_str());
                        sel.alias = Some(SeaRc::new(Alias::new(alias)));
                    }
                }
            };
        });
        self
    }

    /// Selects and Entity and returns it together with the Entity from `Self`
    pub fn select_also<F>(mut self, _: F) -> SelectTwo<E, F>
    where
        F: EntityTrait,
    {
        self = self.apply_alias(SelectA.as_str());
        SelectTwo::new(self.into_query())
    }

    /// Makes a SELECT operation in conjunction to another relation
    pub fn select_with<F>(mut self, _: F) -> SelectTwoMany<E, F>
    where
        F: EntityTrait,
    {
        self = self.apply_alias(SelectA.as_str());
        SelectTwoMany::new(self.into_query())
    }
}

impl<E, F> SelectTwo<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) fn new(query: SelectStatement) -> Self {
        Self::new_without_prepare(query).prepare_select()
    }

    pub(crate) fn new_without_prepare(query: SelectStatement) -> Self {
        Self {
            query,
            entity: PhantomData,
        }
    }

    fn prepare_select(mut self) -> Self {
        prepare_select_two::<F, Self>(&mut self);
        self
    }
}

impl<E, F> SelectTwoMany<E, F>
where
    E: EntityTrait,
    F: EntityTrait,
{
    pub(crate) fn new(query: SelectStatement) -> Self {
        Self::new_without_prepare(query)
            .prepare_select()
            .prepare_order_by()
    }

    pub(crate) fn new_without_prepare(query: SelectStatement) -> Self {
        Self {
            query,
            entity: PhantomData,
        }
    }

    fn prepare_select(mut self) -> Self {
        prepare_select_two::<F, Self>(&mut self);
        self
    }

    fn prepare_order_by(mut self) -> Self {
        for col in <E::PrimaryKey as Iterable>::iter() {
            self.query.order_by((E::default(), col), Order::Asc);
        }
        self
    }
}

fn prepare_select_two<F, S>(selector: &mut S)
where
    F: EntityTrait,
    S: QueryTrait<QueryStatement = SelectStatement>,
{
    for col in <F::Column as Iterable>::iter() {
        let alias = format!("{}{}", SelectB.as_str(), col.as_str());
        selector.query().expr(SelectExpr::new_as(
            col.select_as(col.into_expr()),
            SeaRc::new(Alias::new(alias)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::tests_cfg::{cake, fruit};
    use crate::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect, QueryTrait};

    #[test]
    fn alias_1() {
        assert_eq!(
            cake::Entity::find()
                .column_as(cake::Column::Id, "B")
                .apply_alias("A_")
                .as_query()
                .to_string(),
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name", "cake"."id" AS "A_B" FROM "cake""#,
        );
    }

    #[test]
    fn select_also_1() {
        assert_eq!(
            cake::Entity::find()
                .left_join(fruit::Entity)
                .select_also(fruit::Entity)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake" LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
            ].join(" ")
        );
    }

    #[test]
    fn select_with_1() {
        assert_eq!(
            cake::Entity::find()
                .left_join(fruit::Entity)
                .select_with(fruit::Entity)
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake" LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"ORDER BY "cake"."id" ASC"#,
            ].join(" ")
        );
    }

    #[test]
    fn select_also_2() {
        assert_eq!(
            cake::Entity::find()
                .left_join(fruit::Entity)
                .select_also(fruit::Entity)
                .filter(cake::Column::Id.eq(1))
                .filter(fruit::Column::Id.eq(2))
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake" LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"WHERE "cake"."id" = 1 AND "fruit"."id" = 2"#,
            ].join(" ")
        );
    }

    #[test]
    fn select_with_2() {
        assert_eq!(
            cake::Entity::find()
                .left_join(fruit::Entity)
                .select_with(fruit::Entity)
                .filter(cake::Column::Id.eq(1))
                .filter(fruit::Column::Id.eq(2))
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
                r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
                r#"FROM "cake" LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
                r#"WHERE "cake"."id" = 1 AND "fruit"."id" = 2"#,
                r#"ORDER BY "cake"."id" ASC"#,
            ].join(" ")
        );
    }
}
