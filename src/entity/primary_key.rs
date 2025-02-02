use super::{ColumnTrait, IdenStatic, Iterable};
use crate::{TryFromU64, TryGetableMany};
use pgorm_query::{FromValueTuple, IntoValueTuple};
use std::fmt::Debug;

//LINT: composite primary key cannot auto increment
/// A Trait for to be used to define a Primary Key.
///
/// A primary key can be derived manually
///
/// ### Example
/// ```text
/// use pgorm::entity::prelude::*;
///
/// #[derive(Copy, Clone, Debug, EnumIter)]
/// pub enum PrimaryKey {
///     Id,
/// }
/// impl PrimaryKeyTrait for PrimaryKey {
///     type ValueType = i32;
///
///     fn auto_increment() -> bool {
///         true
///     }
/// }
/// ```
///
/// Alternatively, use derive macros to automatically implement the trait for a Primary Key
///
/// ### Example
/// ```text
/// use pgorm::entity::prelude::*;
///
/// #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
/// pub enum PrimaryKey {
///     Id,
/// }
/// ```
/// See module level docs [crate::entity] for a full example
pub trait PrimaryKeyTrait: IdenStatic + Iterable {
    #[allow(missing_docs)]
    type ValueType: Sized
        + Send
        + Debug
        + PartialEq
        + IntoValueTuple
        + FromValueTuple
        + TryGetableMany
        + TryFromU64
        + PrimaryKeyArity;

    /// Method to call to perform `AUTOINCREMENT` operation on a Primary Key
    fn auto_increment() -> bool;
}

/// How to map a Primary Key to a column
pub trait PrimaryKeyToColumn {
    #[allow(missing_docs)]
    type Column: ColumnTrait;

    /// Method to map a primary key to a column in an Entity
    fn into_column(self) -> Self::Column;

    /// Method to map a primary key from a column in an Entity
    fn from_column(col: Self::Column) -> Option<Self>
    where
        Self: Sized;
}

/// How many columns this Primary Key comprises
pub trait PrimaryKeyArity {
    /// Arity of the Primary Key
    const ARITY: usize;
}

impl<V> PrimaryKeyArity for V
where
    V: crate::TryGetable,
{
    const ARITY: usize = 1;
}

macro_rules! impl_pk_arity {
    ($len:expr, $($tuple_arg:ident),*) => {
        impl<$($tuple_arg: crate::TryGetableMany,)*> PrimaryKeyArity for ($($tuple_arg,)*) {
            const ARITY: usize = $len;
        }
    }
}

impl_pk_arity!(1, T1);
impl_pk_arity!(2, T1, T2);
impl_pk_arity!(3, T1, T2, T3);
impl_pk_arity!(4, T1, T2, T3, T4);
impl_pk_arity!(5, T1, T2, T3, T4, T5);
impl_pk_arity!(6, T1, T2, T3, T4, T5, T6);
impl_pk_arity!(7, T1, T2, T3, T4, T5, T6, T7);
impl_pk_arity!(8, T1, T2, T3, T4, T5, T6, T7, T8);
impl_pk_arity!(9, T1, T2, T3, T4, T5, T6, T7, T8, T9);
impl_pk_arity!(10, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
impl_pk_arity!(11, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
impl_pk_arity!(12, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12);

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(feature = "macros")]
    fn test_composite_primary_key() {
        mod primary_key_of_1 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_1")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        mod primary_key_of_2 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_2")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id_1: i32,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_2: String,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        mod primary_key_of_3 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_3")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id_1: i32,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_2: String,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_3: Uuid,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        mod primary_key_of_4 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_4")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id_1: TimeDateTimeWithTimeZone,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_2: Uuid,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_3: Json,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_4: Decimal,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        mod primary_key_of_11 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
            #[pgorm(
                rs_type = "String",
                db_type = "String(StringLen::N(1))",
                enum_name = "category"
            )]
            pub enum DeriveCategory {
                #[pgorm(string_value = "B")]
                Big,
                #[pgorm(string_value = "S")]
                Small,
            }

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_11")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id_1: Vec<u8>,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_2: DeriveCategory,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_3: Date,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_4: DateTime,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_5: Time,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_6: TimeTime,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_7: DateTime,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_8: TimeDateTime,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_9: DateTimeLocal,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_10: DateTimeUtc,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_11: DateTimeWithTimeZone,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }

        mod primary_key_of_12 {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "primary_key_of_12")]
            pub struct Model {
                #[pgorm(primary_key, auto_increment = false)]
                pub id_1: String,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_2: i8,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_3: u8,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_4: i16,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_5: u16,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_6: i32,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_7: u32,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_8: i64,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_9: u64,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_10: f32,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_11: f64,
                #[pgorm(primary_key, auto_increment = false)]
                pub id_12: bool,
                pub owner: String,
                pub name: String,
                pub description: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }
    }
}
