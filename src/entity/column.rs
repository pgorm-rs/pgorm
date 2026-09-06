use crate::{EntityName, Iden, IdenStr, IntoSimpleExpr, Iterable};
use pgorm_query::{
    Alias, BinOper, DynIden, Expr, Func, IntoColumnRef, IntoIden, SeaRc, SelectStatement,
    SimpleExpr, Value, ValueType,
};
use std::str::FromStr;

// The original `pgorm::ColumnType` enum was dropped since 0.11.0
// It was replaced by `pgorm_query::ColumnType`, we reexport it here to keep the `ColumnType` symbol
pub use pgorm_query::ColumnType;
#[path = "column_def.rs"]
mod column_def;
pub use column_def::*;

/// Defines a Column for an Entity
// [spec:pgorm:req:entity.traits.column-def]
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnDef {
    pub(crate) col_type: ColumnType,
    pub(crate) null: bool,
    pub(crate) unique: bool,
    pub(crate) indexed: bool,
    pub(crate) default: Option<SimpleExpr>,
    pub(crate) comment: Option<String>,
}

macro_rules! bind_oper {
    ( $op: ident, $bin_op: ident ) => {
        #[allow(missing_docs)]
        fn $op<V>(&self, v: V) -> SimpleExpr
        where
            V: Into<Value>,
        {
            let expr = self.save_as(Expr::val(v));
            Expr::col((self.entity_name(), *self)).binary(BinOper::$bin_op, expr)
        }
    };
}

macro_rules! bind_oper_col {
    ( $op: ident, $bin_op: ident, $doc: expr ) => {
        #[doc = $doc]
        ///
        /// Both operands name their own entity, so the comparison needs neither
        /// the `Expr::col` escape nor a respelling of either table. The operand
        /// is a column rather than a value, so it does not pass through
        /// `save_as`: an enum column compared against another column of the same
        /// enum type needs no cast. See [`ColumnTrait::eq_col`] for an example.
        fn $op<C>(&self, other: C) -> SimpleExpr
        where
            C: ColumnTrait,
        {
            Expr::col((self.entity_name(), *self))
                .binary(BinOper::$bin_op, other.as_column_ref().into_column_ref())
        }
    };
}

macro_rules! bind_func_no_params {
    ( $func: ident ) => {
        /// See also SeaQuery's method with same name.
        fn $func(&self) -> SimpleExpr {
            Expr::col((self.entity_name(), *self)).$func()
        }
    };
}

macro_rules! bind_vec_func {
    ( $func: ident ) => {
        #[allow(missing_docs)]
        #[allow(clippy::wrong_self_convention)]
        fn $func<V, I>(&self, v: I) -> SimpleExpr
        where
            V: Into<Value>,
            I: IntoIterator<Item = V>,
        {
            let v_with_enum_cast = v.into_iter().map(|v| self.save_as(Expr::val(v)));
            Expr::col((self.entity_name(), *self)).$func(v_with_enum_cast)
        }
    };
}

macro_rules! bind_subquery_func {
    ( $func: ident ) => {
        #[allow(clippy::wrong_self_convention)]
        #[allow(missing_docs)]
        fn $func(&self, s: SelectStatement) -> SimpleExpr {
            Expr::col((self.entity_name(), *self)).$func(s)
        }
    };
}

// LINT: when the operand value does not match column type
/// API for working with a `Column`. Mostly a wrapper of the identically named methods in [`pgorm_query::Expr`]
// [spec:pgorm:def:entity.traits.column+4]
pub trait ColumnTrait: IdenStr + Iterable + FromStr {
    #[allow(missing_docs)]
    type EntityName: EntityName;

    /// Define a column for an Entity
    fn def(&self) -> ColumnDef;

    /// Get the name of the entity the column belongs to
    fn entity_name(&self) -> DynIden {
        SeaRc::new(Self::EntityName::default()) as DynIden
    }

    /// get the name of the entity the column belongs to
    fn as_column_ref(&self) -> (DynIden, DynIden) {
        (self.entity_name(), SeaRc::new(*self) as DynIden)
    }

    /// The key this column occupies in a JSON object, as `serde` names it.
    ///
    /// The `with-json` conversions on `ActiveModelTrait` deserialize through the
    /// entity's `Model`, so the keys they read are `serde`'s — the model field's
    /// name, spelled the way any `serde` rename spells it — and not the SQL
    /// column name. `DeriveEntityModel` computes this from the field the column
    /// was derived from; a hand-written `Column` has no field to read, so it
    /// falls back to the SQL name, which is what the two agree on when nothing
    /// is renamed.
    fn json_key(&self) -> &str {
        self.as_str()
    }

    bind_oper!(eq, Equal);
    bind_oper!(ne, NotEqual);
    bind_oper!(gt, GreaterThan);
    bind_oper!(gte, GreaterThanOrEqual);
    bind_oper!(lt, SmallerThan);
    bind_oper!(lte, SmallerThanOrEqual);

    /// Express an equality between this column and another column.
    ///
    /// The value-taking [`eq`][ColumnTrait::eq] cannot express this: its operand
    /// is bound by `Into<Value>` and is run through
    /// [`save_as`][ColumnTrait::save_as], which is what casts an enum operand.
    /// The column-taking siblings are separate methods rather than a widening of
    /// `eq` so that cast is never silently dropped.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::{cake, fruit}};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.eq_col(fruit::Column::CakeId))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = "fruit"."cake_id""#
    /// );
    /// ```
    ///
    /// Both operands name their own entity, so the comparison needs neither the
    /// `Expr::col` escape nor a respelling of either table. The operand is a
    /// column rather than a value, so it does not pass through `save_as`: an
    /// enum column compared against another column of the same enum type needs
    /// no cast.
    fn eq_col<C>(&self, other: C) -> SimpleExpr
    where
        C: ColumnTrait,
    {
        Expr::col((self.entity_name(), *self))
            .binary(BinOper::Equal, other.as_column_ref().into_column_ref())
    }

    bind_oper_col!(
        ne_col,
        NotEqual,
        "Express an inequality between this column and another column."
    );
    bind_oper_col!(
        gt_col,
        GreaterThan,
        "Express a greater-than between this column and another column."
    );
    bind_oper_col!(
        gte_col,
        GreaterThanOrEqual,
        "Express a greater-than-or-equal between this column and another column."
    );
    bind_oper_col!(
        lt_col,
        SmallerThan,
        "Express a less-than between this column and another column."
    );
    bind_oper_col!(
        lte_col,
        SmallerThanOrEqual,
        "Express a less-than-or-equal between this column and another column."
    );

    /// Express an equality between this column and an arbitrary expression.
    ///
    /// The general form of [`eq_col`][ColumnTrait::eq_col], for comparing
    /// against something computed rather than against a plain column. Like
    /// `eq_col`, the operand does not pass through
    /// [`save_as`][ColumnTrait::save_as] — an expression carries whatever cast
    /// it was built with.
    ///
    /// ```
    /// use pgorm::{entity::*, pgorm_query::Expr, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.eq_expr(Expr::col((cake::Entity, cake::Column::Id)).add(1)))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = "cake"."id" + 1"#
    /// );
    /// ```
    fn eq_expr<T>(&self, other: T) -> SimpleExpr
    where
        T: Into<SimpleExpr>,
    {
        Expr::col((self.entity_name(), *self)).binary(BinOper::Equal, other.into())
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.between(2, 3))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" BETWEEN 2 AND 3"#
    /// );
    /// ```
    fn between<V>(&self, a: V, b: V) -> SimpleExpr
    where
        V: Into<Value>,
    {
        Expr::col((self.entity_name(), *self)).between(a, b)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.not_between(2, 3))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" NOT BETWEEN 2 AND 3"#
    /// );
    /// ```
    fn not_between<V>(&self, a: V, b: V) -> SimpleExpr
    where
        V: Into<Value>,
    {
        Expr::col((self.entity_name(), *self)).not_between(a, b)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Name.like("cheese"))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."name" LIKE 'cheese'"#
    /// );
    /// ```
    fn like<T>(&self, s: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        Expr::col((self.entity_name(), *self)).like(s)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Name.not_like("cheese"))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."name" NOT LIKE 'cheese'"#
    /// );
    /// ```
    fn not_like<T>(&self, s: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        Expr::col((self.entity_name(), *self)).not_like(s)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Name.starts_with("cheese"))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."name" LIKE 'cheese%'"#
    /// );
    /// ```
    fn starts_with<T>(&self, s: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        let pattern = format!("{}%", s.into());
        Expr::col((self.entity_name(), *self)).like(pattern)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Name.ends_with("cheese"))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."name" LIKE '%cheese'"#
    /// );
    /// ```
    fn ends_with<T>(&self, s: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        let pattern = format!("%{}", s.into());
        Expr::col((self.entity_name(), *self)).like(pattern)
    }

    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Name.contains("cheese"))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."name" LIKE '%cheese%'"#
    /// );
    /// ```
    fn contains<T>(&self, s: T) -> SimpleExpr
    where
        T: Into<String>,
    {
        let pattern = format!("%{}%", s.into());
        Expr::col((self.entity_name(), *self)).like(pattern)
    }

    bind_func_no_params!(max);
    bind_func_no_params!(min);
    bind_func_no_params!(sum);
    bind_func_no_params!(count);
    bind_func_no_params!(is_null);
    bind_func_no_params!(is_not_null);

    /// Perform an operation if the column is null
    fn if_null<V>(&self, v: V) -> SimpleExpr
    where
        V: Into<Value>,
    {
        Expr::col((self.entity_name(), *self)).if_null(v)
    }

    bind_vec_func!(is_in);
    bind_vec_func!(is_not_in);

    /// Membership against one array parameter, the PostgreSQL-idiomatic
    /// counterpart of [`ColumnTrait::is_in`]. See [`pgorm_query::Expr::eq_any`]
    /// for when each applies; the short of it is that `is_in` suits a short
    /// literal list and `eq_any` a list whose length varies at runtime, because
    /// `eq_any` renders the same SQL text — and so reuses the same cached
    /// statement — for every length.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.eq_any([2, 3]))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = ANY(ARRAY [2,3])"#
    /// );
    /// ```
    fn eq_any<V, I>(&self, v: I) -> SimpleExpr
    where
        V: Into<Value> + ValueType,
        I: IntoIterator<Item = V>,
    {
        let array = self.save_array_as(Expr::val(Value::array(v)));
        Expr::col((self.entity_name(), *self)).eq(Func::any(array))
    }

    /// Non-membership against one array parameter, the complement of
    /// [`ColumnTrait::eq_any`]. See [`pgorm_query::Expr::ne_all`] for why the
    /// negation is spelled `<> ALL` rather than `NOT (… = ANY …)`.
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .filter(cake::Column::Id.ne_all([2, 3]))
    ///         .as_query()
    ///         .to_string(),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" <> ALL(ARRAY [2,3])"#
    /// );
    /// ```
    fn ne_all<V, I>(&self, v: I) -> SimpleExpr
    where
        V: Into<Value> + ValueType,
        I: IntoIterator<Item = V>,
    {
        let array = self.save_array_as(Expr::val(Value::array(v)));
        Expr::col((self.entity_name(), *self)).ne(Func::all(array))
    }

    bind_subquery_func!(in_subquery);
    bind_subquery_func!(not_in_subquery);

    /// Construct a [`SimpleExpr::Column`] wrapped in [`Expr`].
    fn into_expr(self) -> Expr {
        Expr::expr(self.into_simple_expr())
    }

    /// Construct a returning [`Expr`].
    #[allow(clippy::match_single_binding)]
    fn into_returning_expr(self) -> Expr {
        Expr::col(self)
    }

    /// Cast column expression used in select statement.
    /// It only cast database enum as text if it's an enum column.
    fn select_as(&self, expr: Expr) -> SimpleExpr {
        self.select_enum_as(expr)
    }

    /// Cast enum column as text; do nothing if `self` is not an enum.
    // [spec:pgorm:sem:entity.traits.column.enum-cast+1]
    fn select_enum_as(&self, expr: Expr) -> SimpleExpr {
        cast_enum_as(expr, self, |col, _, col_type| {
            let type_name = match col_type {
                ColumnType::Array(_) => TextArray.into_iden(),
                _ => Text.into_iden(),
            };
            col.as_enum(type_name)
        })
    }

    /// Cast value of a column into the correct type for database storage.
    /// It only cast text as enum type if it's an enum column.
    fn save_as(&self, val: Expr) -> SimpleExpr {
        self.save_enum_as(val)
    }

    /// Cast an array of values into the column's array type for database
    /// storage, the array counterpart of [`ColumnTrait::save_as`]: an enum
    /// column takes `{enum_name}[]`, and every other column is left untouched.
    ///
    /// A column that overrides `save_as` with a cast of its own — what
    /// `#[pgorm(save_as = "…")]` generates — should override this too, with the
    /// array spelling of the same type.
    // [spec:pgorm:sem:entity.traits.column.enum-cast+1]
    fn save_array_as(&self, val: Expr) -> SimpleExpr {
        let col_def = self.def();
        match col_def.get_enum_name() {
            Some(enum_name) => {
                val.as_enum(Alias::new(format!("{}[]", enum_name.to_string())).into_iden())
            }
            None => val.into(),
        }
    }

    /// Cast value of an enum column as enum type; do nothing if `self` is not an enum.
    /// Will also transform `Array(Vec<Json>)` into `Json(Vec<Json>)` if the column type is `Json`.
    // [spec:pgorm:sem:entity.traits.column.enum-cast+1]
    fn save_enum_as(&self, val: Expr) -> SimpleExpr {
        cast_enum_as(val, self, |col, enum_name, col_type| {
            let type_name = match col_type {
                ColumnType::Array(_) => {
                    Alias::new(format!("{}[]", enum_name.to_string())).into_iden()
                }
                _ => enum_name,
            };
            col.as_enum(type_name)
        })
    }
}

struct Text;
struct TextArray;

impl Iden for Text {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "text").expect("write to sql sink");
    }
}

impl Iden for TextArray {
    fn unquoted(&self, s: &mut dyn std::fmt::Write) {
        write!(s, "text[]").expect("write to sql sink");
    }
}

#[cfg(test)]
mod tests {
    use crate::{ColumnTrait, Condition, EntityTrait, QueryFilter, QueryTrait, tests_cfg::*};
    use pgorm_query::Query;

    #[test]
    fn test_in_subquery_1() {
        assert_eq!(
            cake::Entity::find()
                .filter(
                    Condition::any().add(
                        cake::Column::Id.in_subquery(
                            Query::select()
                                .expr(cake::Column::Id.max())
                                .from(cake::Entity)
                                .to_owned()
                        )
                    )
                )
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
                r#"WHERE "cake"."id" IN (SELECT MAX("cake"."id") FROM "cake")"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn test_in_subquery_2() {
        assert_eq!(
            cake::Entity::find()
                .filter(
                    Condition::any().add(
                        cake::Column::Id.in_subquery(
                            Query::select()
                                .column(cake_filling::Column::CakeId)
                                .from(cake_filling::Entity)
                                .to_owned()
                        )
                    )
                )
                .as_query()
                .to_string(),
            [
                r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
                r#"WHERE "cake"."id" IN (SELECT "cake_id" FROM "cake_filling")"#,
            ]
            .join(" ")
        );
    }

    #[test]
    fn test_col_from_str() {
        use std::str::FromStr;

        assert!(matches!(
            fruit::Column::from_str("id"),
            Ok(fruit::Column::Id)
        ));
        assert!(matches!(
            fruit::Column::from_str("name"),
            Ok(fruit::Column::Name)
        ));
        assert!(matches!(
            fruit::Column::from_str("cake_id"),
            Ok(fruit::Column::CakeId)
        ));
        assert!(matches!(
            fruit::Column::from_str("cakeId"),
            Ok(fruit::Column::CakeId)
        ));
        assert!(matches!(
            fruit::Column::from_str("does_not_exist"),
            Err(crate::ColumnFromStrError(_))
        ));
    }

    #[test]
    #[cfg(feature = "macros")]
    fn entity_model_column_1() {
        use crate::prelude::*;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                pub one: i32,
                #[pgorm(unique)]
                pub two: i16,
                #[pgorm(indexed)]
                pub three: i16,
                #[pgorm(nullable)]
                pub four: i32,
                #[pgorm(unique, indexed, nullable)]
                pub five: i64,
                #[pgorm(nullable)]
                pub eight: i64,
                #[pgorm(default_expr = "Expr::current_timestamp()")]
                pub ten: DateTimeUtc,
                #[pgorm(default_value = 7)]
                pub eleven: i16,
                #[pgorm(default_value = "twelve_value")]
                pub twelve: String,
                #[pgorm(default_expr = "\"twelve_value\"")]
                pub twelve_two: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::One.def(), ColumnType::Integer.def());
        assert_eq!(
            hello::Column::Two.def(),
            ColumnType::SmallInteger.def().unique()
        );
        assert_eq!(
            hello::Column::Three.def(),
            ColumnType::SmallInteger.def().indexed()
        );
        assert_eq!(
            hello::Column::Four.def(),
            ColumnType::Integer.def().nullable()
        );
        assert_eq!(
            hello::Column::Five.def(),
            ColumnType::BigInteger.def().unique().indexed().nullable()
        );
        assert_eq!(
            hello::Column::Eight.def(),
            ColumnType::BigInteger.def().nullable()
        );
        assert_eq!(
            hello::Column::Ten.def(),
            ColumnType::TimestampWithTimeZone
                .def()
                .default(Expr::current_timestamp())
        );
        assert_eq!(
            hello::Column::Eleven.def(),
            ColumnType::SmallInteger.def().default(7)
        );
        assert_eq!(
            hello::Column::Twelve.def(),
            ColumnType::String(StringLen::None)
                .def()
                .default("twelve_value")
        );
        assert_eq!(
            hello::Column::TwelveTwo.def(),
            ColumnType::String(StringLen::None)
                .def()
                .default("twelve_value")
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn column_name_1() {
        use pgorm_query::Iden;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(column_name = "ONE")]
                pub one: i32,
                pub two: i32,
                #[pgorm(column_name = "3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::One.to_string().as_str(), "ONE");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three.to_string().as_str(), "3");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn column_name_2() {
        use pgorm_query::Iden;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                pub one: i32,
                pub two: i32,
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                #[pgorm(column_name = "ONE")]
                One,
                Two,
                #[pgorm(column_name = "3")]
                Three,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::Id => ColumnType::Integer.def(),
                        Column::One => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three => ColumnType::Integer.def(),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::One.to_string().as_str(), "ONE");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three.to_string().as_str(), "3");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn enum_name_1() {
        use pgorm_query::Iden;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::One1.to_string().as_str(), "one1");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three3.to_string().as_str(), "three3");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn enum_name_2() {
        use pgorm_query::Iden;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                One1,
                Two,
                Three3,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::Id => ColumnType::Integer.def(),
                        Column::One1 => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three3 => ColumnType::Integer.def(),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::One1.to_string().as_str(), "one1");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three3.to_string().as_str(), "three3");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn column_name_enum_name_1() {
        use pgorm_query::Iden;

        #[allow(clippy::enum_variant_names)]
        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key, column_name = "ID", enum_name = "IdentityColumn")]
                pub id: i32,
                #[pgorm(column_name = "ONE", enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(column_name = "THREE", enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::IdentityColumn.to_string().as_str(), "ID");
        assert_eq!(hello::Column::One1.to_string().as_str(), "ONE");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three3.to_string().as_str(), "THREE");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn column_name_enum_name_2() {
        use pgorm_query::Iden;

        mod hello {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                #[pgorm(enum_name = "IdentityCol")]
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                #[pgorm(column_name = "ID")]
                IdentityCol,
                #[pgorm(column_name = "ONE")]
                One1,
                Two,
                #[pgorm(column_name = "THREE")]
                Three3,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::IdentityCol => ColumnType::Integer.def(),
                        Column::One1 => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three3 => ColumnType::Integer.def(),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                IdentityCol,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(hello::Column::IdentityCol.to_string().as_str(), "ID");
        assert_eq!(hello::Column::One1.to_string().as_str(), "ONE");
        assert_eq!(hello::Column::Two.to_string().as_str(), "two");
        assert_eq!(hello::Column::Three3.to_string().as_str(), "THREE");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn column_name_enum_name_3() {
        use pgorm_query::Iden;

        mod my_entity {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "my_entity")]
            pub struct Model {
                #[pgorm(primary_key, enum_name = "IdentityColumn", column_name = "id")]
                pub id: i32,
                #[pgorm(column_name = "type")]
                pub type_: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(my_entity::Column::IdentityColumn.to_string().as_str(), "id");
        assert_eq!(my_entity::Column::Type.to_string().as_str(), "type");
    }

    #[test]
    #[cfg(feature = "macros")]
    fn select_as_1() {
        use crate::{ActiveModelTrait, ActiveValue, Update};

        mod hello_expanded {
            use crate as pgorm;
            use crate::entity::prelude::*;
            use crate::pgorm_query::{Expr, SimpleExpr};

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                One1,
                Two,
                Three3,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::Id => ColumnType::Integer.def(),
                        Column::One1 => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three3 => ColumnType::Integer.def(),
                    }
                }

                fn select_as(&self, expr: Expr) -> SimpleExpr {
                    match self {
                        Self::Two => expr.cast_as(alias("integer")),
                        _ => self.select_enum_as(expr),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        #[allow(clippy::enum_variant_names)]
        mod hello_compact {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                #[pgorm(select_as = "integer")]
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        fn assert_it<E, A>(active_model: A)
        where
            E: EntityTrait,
            A: ActiveModelTrait<Entity = E>,
        {
            assert_eq!(
                E::find().as_query().to_string(),
                r#"SELECT "hello"."id", "hello"."one1", CAST("hello"."two" AS integer), "hello"."three3" FROM "hello""#,
            );
            assert_eq!(
                Update::one(active_model)
                    .expect("the primary key is set")
                    .as_query()
                    .to_string(),
                r#"UPDATE "hello" SET "one1" = 1, "two" = 2, "three3" = 3 WHERE "hello"."id" = 1"#,
            );
        }

        assert_it(hello_expanded::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
        assert_it(hello_compact::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
    }

    #[test]
    #[cfg(feature = "macros")]
    fn save_as_1() {
        use crate::{ActiveModelTrait, ActiveValue, Update};

        mod hello_expanded {
            use crate as pgorm;
            use crate::entity::prelude::*;
            use crate::pgorm_query::{Expr, SimpleExpr};

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                One1,
                Two,
                Three3,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::Id => ColumnType::Integer.def(),
                        Column::One1 => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three3 => ColumnType::Integer.def(),
                    }
                }

                fn save_as(&self, val: Expr) -> SimpleExpr {
                    match self {
                        Self::Two => val.cast_as(alias("text")),
                        _ => self.save_enum_as(val),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        #[allow(clippy::enum_variant_names)]
        mod hello_compact {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                #[pgorm(save_as = "text")]
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        fn assert_it<E, A>(active_model: A)
        where
            E: EntityTrait,
            A: ActiveModelTrait<Entity = E>,
        {
            assert_eq!(
                E::find().as_query().to_string(),
                r#"SELECT "hello"."id", "hello"."one1", "hello"."two", "hello"."three3" FROM "hello""#,
            );
            assert_eq!(
                Update::one(active_model)
                    .expect("the primary key is set")
                    .as_query()
                    .to_string(),
                r#"UPDATE "hello" SET "one1" = 1, "two" = CAST(2 AS text), "three3" = 3 WHERE "hello"."id" = 1"#,
            );
        }

        assert_it(hello_expanded::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
        assert_it(hello_compact::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
    }

    #[test]
    #[cfg(feature = "macros")]
    fn select_as_and_value_1() {
        use crate::{ActiveModelTrait, ActiveValue, Update};

        mod hello_expanded {
            use crate as pgorm;
            use crate::entity::prelude::*;
            use crate::pgorm_query::{Expr, SimpleExpr};

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "hello"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                One1,
                Two,
                Three3,
            }

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Column::Id => ColumnType::Integer.def(),
                        Column::One1 => ColumnType::Integer.def(),
                        Column::Two => ColumnType::Integer.def(),
                        Column::Three3 => ColumnType::Integer.def(),
                    }
                }

                fn select_as(&self, expr: Expr) -> SimpleExpr {
                    match self {
                        Self::Two => expr.cast_as(alias("integer")),
                        _ => self.select_enum_as(expr),
                    }
                }

                fn save_as(&self, val: Expr) -> SimpleExpr {
                    match self {
                        Self::Two => val.cast_as(alias("text")),
                        _ => self.save_enum_as(val),
                    }
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    true
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        #[allow(clippy::enum_variant_names)]
        mod hello_compact {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
            #[pgorm(table_name = "hello")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(enum_name = "One1")]
                pub one: i32,
                #[pgorm(select_as = "integer", save_as = "text")]
                pub two: i32,
                #[pgorm(enum_name = "Three3")]
                pub three: i32,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        fn assert_it<E, A>(active_model: A)
        where
            E: EntityTrait,
            A: ActiveModelTrait<Entity = E>,
        {
            assert_eq!(
                E::find().as_query().to_string(),
                r#"SELECT "hello"."id", "hello"."one1", CAST("hello"."two" AS integer), "hello"."three3" FROM "hello""#,
            );
            assert_eq!(
                Update::one(active_model)
                    .expect("the primary key is set")
                    .as_query()
                    .to_string(),
                r#"UPDATE "hello" SET "one1" = 1, "two" = CAST(2 AS text), "three3" = 3 WHERE "hello"."id" = 1"#,
            );
        }

        assert_it(hello_expanded::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
        assert_it(hello_compact::ActiveModel {
            id: ActiveValue::set(1),
            one: ActiveValue::set(1),
            two: ActiveValue::set(2),
            three: ActiveValue::set(3),
        });
    }
}
