use crate::{
    ConnectionTrait, DeleteResult, EntityTrait, Iterable, PrimaryKeyArity, PrimaryKeyToColumn,
    PrimaryKeyTrait, Value, error::*,
};
use async_trait::async_trait;
use pgorm_query::{Nullable, ValueTuple};
use std::fmt::Debug;

pub use ActiveValue::NotSet;

/// Defines a stateful value used in ActiveModel.
///
/// There are three possible state represented by three enum variants.
/// - [ActiveValue::Set]: A defined [Value] actively being set
/// - [ActiveValue::Unchanged]: A defined [Value] remain unchanged
/// - [ActiveValue::NotSet]: An undefined [Value]
///
/// The stateful value is useful when constructing UPDATE SQL statement,
/// see an example below.
///
/// # Examples
///
/// ```
/// use pgorm::tests_cfg::{cake, fruit};
/// use pgorm::{entity::*, query::*};
///
/// // The code snipped below does an UPDATE operation on a `ActiveValue`
/// assert_eq!(
///     Update::one(fruit::ActiveModel {
///         id: ActiveValue::set(1),
///         name: ActiveValue::set("Orange".to_owned()),
///         cake_id: ActiveValue::not_set(),
///     })
///     .expect("the primary key is set")
///     .as_query()
///     .to_string(),
///     r#"UPDATE "fruit" SET "name" = 'Orange' WHERE "fruit"."id" = 1"#
/// );
/// ```
// [spec:pgorm:def:entity.active-model.active-value]
#[derive(Clone, Debug)]
pub enum ActiveValue<V>
where
    V: Into<Value>,
{
    /// A defined [Value] actively being set
    Set(V),
    /// A defined [Value] remain unchanged
    Unchanged(V),
    /// An undefined [Value]
    NotSet,
}

// [spec:pgorm:req:entity.active-model.from-sugar]
impl<V: Into<Value>> From<V> for ActiveValue<V> {
    fn from(value: V) -> Self {
        ActiveValue::Set(value)
    }
}

/// A Trait for ActiveModel to perform Create, Update or Delete operation.
/// The type must also implement the [EntityTrait].
///
/// Writes state their intent: pick [`ActiveModelTrait::insert`] or
/// [`ActiveModelTrait::update`]. There is no `save` that guesses between the two
/// from the primary-key state, because `Set` and `Unchanged` keys are
/// indistinguishable to such a guess. For insert-or-update in one statement,
/// attach an `ON CONFLICT` clause with [`Insert::on_conflict`](crate::Insert::on_conflict).
///
/// See module level docs [crate::entity] for a full example
// [spec:pgorm:req:entity.active-model+2]
// [spec:pgorm:req:entity.active-model.save+1]
#[async_trait]
pub trait ActiveModelTrait: Clone + Debug {
    /// The Entity this ActiveModel belongs to
    type Entity: EntityTrait;

    /// Get a mutable [ActiveValue] from an ActiveModel
    fn take(&mut self, c: <Self::Entity as EntityTrait>::Column) -> ActiveValue<Value>;

    /// Get a immutable [ActiveValue] from an ActiveModel
    fn get(&self, c: <Self::Entity as EntityTrait>::Column) -> ActiveValue<Value>;

    /// Set the Value into an ActiveModel, reporting a column this model does not
    /// carry, or a value of the wrong type for it, as [`Error::Type`].
    fn set(&mut self, c: <Self::Entity as EntityTrait>::Column, v: Value) -> Result<(), Error>;

    /// Set the state of an [ActiveValue] to the not set state
    fn not_set(&mut self, c: <Self::Entity as EntityTrait>::Column);

    /// Check the state of a [ActiveValue]
    fn is_not_set(&self, c: <Self::Entity as EntityTrait>::Column) -> bool;

    /// The default implementation of the ActiveModel
    fn default() -> Self;

    /// Reset the value from [ActiveValue::Unchanged] to [ActiveValue::Set],
    /// leaving [ActiveValue::NotSet] untouched.
    fn reset(&mut self, c: <Self::Entity as EntityTrait>::Column);

    /// Reset all values from [ActiveValue::Unchanged] to [ActiveValue::Set],
    /// leaving [ActiveValue::NotSet] untouched.
    fn reset_all(mut self) -> Self {
        for col in <Self::Entity as EntityTrait>::Column::iter() {
            self.reset(col);
        }
        self
    }

    /// Get the primary key of the ActiveModel
    #[allow(clippy::question_mark)]
    fn get_primary_key_value(&self) -> Option<ValueTuple> {
        let mut cols = <Self::Entity as EntityTrait>::PrimaryKey::iter();
        macro_rules! next {
            () => {
                if let Some(col) = cols.next() {
                    if let Some(val) = self.get(col.into_column()).into_value() {
                        val
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            };
        }
        match <<<Self::Entity as EntityTrait>::PrimaryKey as PrimaryKeyTrait>::ValueType as PrimaryKeyArity>::ARITY {
            1 => {
                let s1 = next!();
                Some(ValueTuple::One(s1))
            }
            2 => {
                let s1 = next!();
                let s2 = next!();
                Some(ValueTuple::Two(s1, s2))
            }
            3 => {
                let s1 = next!();
                let s2 = next!();
                let s3 = next!();
                Some(ValueTuple::Three(s1, s2, s3))
            }
            len => {
                let mut vec = Vec::with_capacity(len);
                for _ in 0..len {
                    let s = next!();
                    vec.push(s);
                }
                Some(ValueTuple::Many(vec))
            }
        }
    }

    /// Perform an `INSERT` operation on the ActiveModel
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let apple = cake::ActiveModel {
    ///     name: ActiveValue::Set("Apple Pie".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// // The insert carries `RETURNING` over every column, so the fully
    /// // populated model comes back without a second round-trip.
    /// let apple: cake::Model = apple.insert(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:req:entity.active-model.persistence+1]
    async fn insert<'a, C>(self, db: &'a C) -> Result<<Self::Entity as EntityTrait>::Model, Error>
    where
        <Self::Entity as EntityTrait>::Model: IntoActiveModel<Self>,
        Self: ActiveModelBehavior + 'a,
        C: ConnectionTrait,
    {
        // tracing::debug!("Insert ActiveModel: {:?}", self);
        let am = ActiveModelBehavior::before_save(self, db, true).await?;
        // tracing::debug!("before save");
        let model = <Self::Entity as EntityTrait>::insert(am)
            .exec_with_returning(db)
            .await?;
        // tracing::debug!("before save");
        Self::after_save(model, db, true).await
    }

    /// Perform the `UPDATE` operation on an ActiveModel
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(1),
    ///     name: ActiveValue::Set("Orange".to_owned()),
    ///     ..Default::default()
    /// };
    ///
    /// // Fails with `Error::RecordNotFound` when the primary key matches no row.
    /// let orange: fruit::Model = orange.update(&db).await?;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:req:entity.active-model.persistence+1]
    async fn update<'a, C>(self, db: &'a C) -> Result<<Self::Entity as EntityTrait>::Model, Error>
    where
        <Self::Entity as EntityTrait>::Model: IntoActiveModel<Self>,
        Self: ActiveModelBehavior + 'a,
        C: ConnectionTrait,
    {
        let am = ActiveModelBehavior::before_save(self, db, false).await?;
        let model: <Self::Entity as EntityTrait>::Model =
            Self::Entity::update(am)?.exec(db).await?;
        Self::after_save(model, db, false).await
    }

    /// Delete an active model by its primary key
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::fruit, DatabasePool};
    /// #
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let orange = fruit::ActiveModel {
    ///     id: ActiveValue::Set(3),
    ///     ..Default::default()
    /// };
    ///
    /// let delete_result = orange.delete(&db).await?;
    /// let affected: u64 = delete_result.rows_affected;
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:req:entity.active-model.hooks]
    async fn delete<'a, C>(self, db: &'a C) -> Result<DeleteResult, Error>
    where
        Self: ActiveModelBehavior + 'a,
        C: ConnectionTrait,
    {
        let am = ActiveModelBehavior::before_delete(self, db).await?;
        let am_clone = am.clone();
        let delete_res = Self::Entity::delete(am)?.exec(db).await?;
        ActiveModelBehavior::after_delete(am_clone, db).await?;
        Ok(delete_res)
    }

    /// Set the corresponding attributes in the ActiveModel from a JSON value
    ///
    /// Note that this method will not alter the primary key values in ActiveModel.
    // [spec:pgorm:req:entity.active-model.json+2]
    #[cfg(feature = "with-json")]
    fn set_from_json(&mut self, json: serde_json::Value) -> Result<(), Error>
    where
        <<Self as ActiveModelTrait>::Entity as EntityTrait>::Model: IntoActiveModel<Self>,
        for<'de> <<Self as ActiveModelTrait>::Entity as EntityTrait>::Model:
            serde::de::Deserialize<'de>,
    {
        use crate::Iterable;

        // Backup primary key values
        let primary_key_values: Vec<(<Self::Entity as EntityTrait>::Column, ActiveValue<Value>)> =
            <<Self::Entity as EntityTrait>::PrimaryKey>::iter()
                .map(|pk| (pk.into_column(), self.take(pk.into_column())))
                .collect();

        // Replace all values in ActiveModel
        *self = Self::from_json(json)?;

        // Restore primary key values
        for (col, active_value) in primary_key_values {
            match active_value {
                ActiveValue::Unchanged(v) | ActiveValue::Set(v) => self.set(col, v)?,
                NotSet => self.not_set(col),
            }
        }

        Ok(())
    }

    /// Create ActiveModel from a JSON value
    // [spec:pgorm:req:entity.active-model.json+2]
    #[cfg(feature = "with-json")]
    fn from_json(json: serde_json::Value) -> Result<Self, Error>
    where
        <<Self as ActiveModelTrait>::Entity as EntityTrait>::Model: IntoActiveModel<Self>,
        for<'de> <<Self as ActiveModelTrait>::Entity as EntityTrait>::Model:
            serde::de::Deserialize<'de>,
    {
        use crate::{Iden, Iterable};

        // Mark down which attribute exists in the JSON object
        let json_keys: Vec<(<Self::Entity as EntityTrait>::Column, bool)> =
            <<Self::Entity as EntityTrait>::Column>::iter()
                .map(|col| (col, json.get(col.to_string()).is_some()))
                .collect();

        // Convert JSON object into ActiveModel via Model
        let model: <Self::Entity as EntityTrait>::Model =
            serde_json::from_value(json).map_err(json_err)?;
        let mut am = model.into_active_model();

        // Transform attribute that exists in JSON object into ActiveValue::Set, otherwise ActiveValue::NotSet
        for (col, json_key_exists) in json_keys {
            match (json_key_exists, am.get(col)) {
                (true, ActiveValue::Set(value) | ActiveValue::Unchanged(value)) => {
                    am.set(col, value)?;
                }
                _ => {
                    am.not_set(col);
                }
            }
        }

        Ok(am)
    }

    /// Return `true` if any attribute of `ActiveModel` is `Set`
    fn is_changed(&self) -> bool {
        <Self::Entity as EntityTrait>::Column::iter()
            .any(|col| self.get(col).is_set() && !self.get(col).is_unchanged())
    }
}

/// A Trait for overriding the ActiveModel behavior
///
/// ### Example
/// ```
/// # #[cfg(feature = "macros")]
/// # {
/// use pgorm::entity::prelude::*;
///
/// // `DeriveEntityModel` derives the Entity, Column, PrimaryKey and ActiveModel
/// #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
/// #[pgorm(table_name = "cake")]
/// pub struct Model {
///     #[pgorm(primary_key)]
///     pub id: i32,
///     pub name: String,
/// }
///
/// #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
/// pub enum Relation {}
///
/// // Every hook has a no-op default; override only what you need
/// impl ActiveModelBehavior for ActiveModel {}
/// # }
/// ```
/// See module level docs [crate::entity] for a full example
// [spec:pgorm:req:entity.active-model.hooks]
#[allow(unused_variables)]
#[async_trait]
pub trait ActiveModelBehavior: ActiveModelTrait {
    /// Create a new ActiveModel with default values. Also used by `Default::default()`.
    fn new() -> Self {
        <Self as ActiveModelTrait>::default()
    }

    /// Will be called before `ActiveModel::insert` (`insert: true`) and
    /// `ActiveModel::update` (`insert: false`)
    async fn before_save<C>(self, db: &C, insert: bool) -> Result<Self, Error>
    where
        C: ConnectionTrait,
    {
        Ok(self)
    }

    /// Will be called after `ActiveModel::insert` (`insert: true`) and
    /// `ActiveModel::update` (`insert: false`)
    async fn after_save<C>(
        model: <Self::Entity as EntityTrait>::Model,
        db: &C,
        insert: bool,
    ) -> Result<<Self::Entity as EntityTrait>::Model, Error>
    where
        C: ConnectionTrait,
    {
        Ok(model)
    }

    /// Will be called before `ActiveModel::delete`
    async fn before_delete<C>(self, db: &C) -> Result<Self, Error>
    where
        C: ConnectionTrait,
    {
        Ok(self)
    }

    /// Will be called after `ActiveModel::delete`
    async fn after_delete<C>(self, db: &C) -> Result<Self, Error>
    where
        C: ConnectionTrait,
    {
        Ok(self)
    }
}

/// A Trait for any type that can be converted into an ActiveModel
// [spec:pgorm:req:entity.active-model.into+1]
pub trait IntoActiveModel<A>
where
    A: ActiveModelTrait,
{
    /// Method to call to perform the conversion
    fn into_active_model(self) -> A;
}

impl<A> IntoActiveModel<A> for A
where
    A: ActiveModelTrait,
{
    fn into_active_model(self) -> A {
        self
    }
}

/// Any type that can be converted into an [ActiveValue]
// [spec:pgorm:req:entity.active-model.into+1]
pub trait IntoActiveValue<V>
where
    V: Into<Value>,
{
    /// Method to perform the conversion
    fn into_active_value(self) -> ActiveValue<V>;
}

impl<V> IntoActiveValue<Option<V>> for Option<V>
where
    V: IntoActiveValue<V> + Into<Value> + Nullable,
{
    fn into_active_value(self) -> ActiveValue<Option<V>> {
        match self {
            Some(value) => ActiveValue::Set(Some(value)),
            None => NotSet,
        }
    }
}

impl<V> IntoActiveValue<Option<V>> for Option<Option<V>>
where
    V: IntoActiveValue<V> + Into<Value> + Nullable,
{
    fn into_active_value(self) -> ActiveValue<Option<V>> {
        match self {
            Some(value) => ActiveValue::Set(value),
            None => NotSet,
        }
    }
}

macro_rules! impl_into_active_value {
    ($ty: ty) => {
        impl IntoActiveValue<$ty> for $ty {
            fn into_active_value(self) -> ActiveValue<$ty> {
                ActiveValue::Set(self)
            }
        }
    };
}

impl_into_active_value!(bool);
impl_into_active_value!(i8);
impl_into_active_value!(i16);
impl_into_active_value!(i32);
impl_into_active_value!(i64);
impl_into_active_value!(u32);
impl_into_active_value!(u64);
impl_into_active_value!(f32);
impl_into_active_value!(f64);
impl_into_active_value!(&'static str);
impl_into_active_value!(String);
impl_into_active_value!(Vec<u8>);

#[cfg(feature = "with-json")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-json")))]
impl_into_active_value!(crate::prelude::Json);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::Date);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::Time);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::DateTime);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::DateTimeWithTimeZone);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::DateTimeUtc);

#[cfg(feature = "with-chrono")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-chrono")))]
impl_into_active_value!(crate::prelude::DateTimeLocal);

#[cfg(feature = "with-rust_decimal")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-rust_decimal")))]
impl_into_active_value!(crate::prelude::Decimal);

#[cfg(feature = "with-uuid")]
#[cfg_attr(docsrs, doc(cfg(feature = "with-uuid")))]
impl_into_active_value!(crate::prelude::Uuid);

impl<V> Default for ActiveValue<V>
where
    V: Into<Value>,
{
    /// Create an [ActiveValue::NotSet]
    fn default() -> Self {
        Self::NotSet
    }
}

// [spec:pgorm:sem:entity.active-model.active-value.ops]
impl<V> ActiveValue<V>
where
    V: Into<Value>,
{
    /// Create an [ActiveValue::Set]
    pub fn set(value: V) -> Self {
        Self::Set(value)
    }

    /// Check if the [ActiveValue] is [ActiveValue::Set]
    pub fn is_set(&self) -> bool {
        matches!(self, Self::Set(_))
    }

    /// Create an [ActiveValue::Unchanged]
    pub fn unchanged(value: V) -> Self {
        Self::Unchanged(value)
    }

    /// Check if the [ActiveValue] is [ActiveValue::Unchanged]
    pub fn is_unchanged(&self) -> bool {
        matches!(self, Self::Unchanged(_))
    }

    /// Create an [ActiveValue::NotSet]
    pub fn not_set() -> Self {
        Self::default()
    }

    /// Check if the [ActiveValue] is [ActiveValue::NotSet]
    pub fn is_not_set(&self) -> bool {
        matches!(self, Self::NotSet)
    }

    /// Get the mutable value an [ActiveValue]
    /// also setting itself to [ActiveValue::NotSet]
    pub fn take(&mut self) -> Option<V> {
        match std::mem::take(self) {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) => Some(value),
            ActiveValue::NotSet => None,
        }
    }

    /// Get an owned value of the [ActiveValue]
    ///
    /// # Panics
    ///
    /// Panics if it is [ActiveValue::NotSet]
    pub fn unwrap(self) -> V {
        match self {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) => value,
            ActiveValue::NotSet => panic!("Cannot unwrap ActiveValue::NotSet"),
        }
    }

    /// Check if a [Value] exists or not
    pub fn into_value(self) -> Option<Value> {
        match self {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) => Some(value.into()),
            ActiveValue::NotSet => None,
        }
    }

    /// Wrap the [Value] into a `ActiveValue<Value>`
    pub fn into_wrapped_value(self) -> ActiveValue<Value> {
        match self {
            Self::Set(value) => ActiveValue::set(value.into()),
            Self::Unchanged(value) => ActiveValue::unchanged(value.into()),
            Self::NotSet => ActiveValue::not_set(),
        }
    }

    /// Reset the value from [ActiveValue::Unchanged] to [ActiveValue::Set],
    /// leaving [ActiveValue::NotSet] untouched.
    pub fn reset(&mut self) {
        *self = match self.take() {
            Some(value) => ActiveValue::Set(value),
            None => ActiveValue::NotSet,
        };
    }

    /// `Set(value)`, except when [`self.is_unchanged()`][ActiveValue#method.is_unchanged]
    /// and `value` equals the current [Unchanged][ActiveValue::Unchanged] value.
    ///
    /// This is useful when you have an [Unchanged][ActiveValue::Unchanged] value from the database,
    /// then update it using this method,
    /// and then use [`.is_unchanged()`][ActiveValue#method.is_unchanged] to see whether it has *actually* changed.
    ///
    /// The same nice effect applies to the entire `ActiveModel`.
    /// You can now meaningfully use [ActiveModelTrait::is_changed][ActiveModelTrait#method.is_changed]
    /// to see whether are any changes that need to be saved to the database.
    ///
    /// ## Examples
    ///
    /// ```
    /// # use pgorm::ActiveValue;
    /// #
    /// let mut value = ActiveValue::Unchanged("old");
    ///
    /// // This wouldn't be the case if we used plain `value = Set("old");`
    /// value.set_if_not_equals("old");
    /// assert!(value.is_unchanged());
    ///
    /// // Only when we change the actual `&str` value, it becomes `Set`
    /// value.set_if_not_equals("new");
    /// assert_eq!(value.is_unchanged(), false);
    /// assert_eq!(value, ActiveValue::Set("new"));
    /// ```
    pub fn set_if_not_equals(&mut self, value: V)
    where
        V: PartialEq,
    {
        match self {
            ActiveValue::Unchanged(current) if &value == current => {}
            _ => *self = ActiveValue::Set(value),
        }
    }

    /// Get the inner value, unless `self` is [NotSet][ActiveValue::NotSet].
    ///
    /// There's also a panicking version: [ActiveValue::as_ref].
    ///
    /// ## Examples
    ///
    /// ```
    /// # use pgorm::ActiveValue;
    /// #
    /// assert_eq!(ActiveValue::Unchanged(42).try_as_ref(), Some(&42));
    /// assert_eq!(ActiveValue::Set(42).try_as_ref(), Some(&42));
    /// assert_eq!(ActiveValue::NotSet.try_as_ref(), None::<&i32>);
    /// ```
    pub fn try_as_ref(&self) -> Option<&V> {
        match self {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) => Some(value),
            ActiveValue::NotSet => None,
        }
    }
}

// [spec:pgorm:sem:entity.active-model.active-value.ops]
impl<V> std::convert::AsRef<V> for ActiveValue<V>
where
    V: Into<Value>,
{
    /// # Panics
    ///
    /// Panics if it is [ActiveValue::NotSet].
    ///
    /// See [ActiveValue::try_as_ref] for a fallible non-panicking version.
    fn as_ref(&self) -> &V {
        match self {
            ActiveValue::Set(value) | ActiveValue::Unchanged(value) => value,
            ActiveValue::NotSet => panic!("Cannot borrow ActiveValue::NotSet"),
        }
    }
}

impl<V> PartialEq for ActiveValue<V>
where
    V: Into<Value> + std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ActiveValue::Set(l), ActiveValue::Set(r)) => l == r,
            (ActiveValue::Unchanged(l), ActiveValue::Unchanged(r)) => l == r,
            (ActiveValue::NotSet, ActiveValue::NotSet) => true,
            _ => false,
        }
    }
}

// [spec:pgorm:req:entity.active-model.from-sugar]
impl<V> From<ActiveValue<V>> for ActiveValue<Option<V>>
where
    V: Into<Value> + Nullable,
{
    fn from(value: ActiveValue<V>) -> Self {
        match value {
            ActiveValue::Set(value) => ActiveValue::set(Some(value)),
            ActiveValue::Unchanged(value) => ActiveValue::unchanged(Some(value)),
            ActiveValue::NotSet => ActiveValue::not_set(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::ActiveValue::{NotSet, Set, Unchanged};
    use crate::{Error, entity::*, tests_cfg::*};
    use pretty_assertions::assert_eq;

    #[cfg(feature = "with-json")]
    use serde_json::json;

    #[test]
    #[cfg(feature = "macros")]
    fn test_derive_into_active_model_1() {
        mod my_fruit {
            pub use super::fruit::*;
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(DeriveIntoActiveModel)]
            pub struct NewFruit {
                // id is omitted
                pub name: String,
                // it is required as opposed to optional in Model
                pub cake_id: i32,
            }
        }

        assert_eq!(
            my_fruit::NewFruit {
                name: "Apple".to_owned(),
                cake_id: 1,
            }
            .into_active_model(),
            fruit::ActiveModel {
                id: NotSet,
                name: Set("Apple".to_owned()),
                cake_id: Set(Some(1)),
            }
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn test_derive_into_active_model_2() {
        mod my_fruit {
            pub use super::fruit::*;
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(DeriveIntoActiveModel)]
            pub struct UpdateFruit {
                pub cake_id: Option<Option<i32>>,
            }
        }

        assert_eq!(
            my_fruit::UpdateFruit {
                cake_id: Some(Some(1)),
            }
            .into_active_model(),
            fruit::ActiveModel {
                id: NotSet,
                name: NotSet,
                cake_id: Set(Some(1)),
            }
        );

        assert_eq!(
            my_fruit::UpdateFruit {
                cake_id: Some(None),
            }
            .into_active_model(),
            fruit::ActiveModel {
                id: NotSet,
                name: NotSet,
                cake_id: Set(None),
            }
        );

        assert_eq!(
            my_fruit::UpdateFruit { cake_id: None }.into_active_model(),
            fruit::ActiveModel {
                id: NotSet,
                name: NotSet,
                cake_id: NotSet,
            }
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn test_derive_try_into_model_1() {
        mod my_fruit {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "fruit")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                pub name: String,
                pub cake_id: Option<i32>,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }
        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(1),
                name: Set("Pineapple".to_owned()),
                cake_id: Set(None),
            }
            .try_into_model()
            .unwrap(),
            my_fruit::Model {
                id: 1,
                name: "Pineapple".to_owned(),
                cake_id: None,
            }
        );

        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(2),
                name: Set("Apple".to_owned()),
                cake_id: Set(Some(1)),
            }
            .try_into_model()
            .unwrap(),
            my_fruit::Model {
                id: 2,
                name: "Apple".to_owned(),
                cake_id: Some(1),
            }
        );

        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(1),
                name: NotSet,
                cake_id: Set(None),
            }
            .try_into_model(),
            Err(Error::AttrNotSet(String::from("name")))
        );

        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(1),
                name: Set("Pineapple".to_owned()),
                cake_id: NotSet,
            }
            .try_into_model(),
            Err(Error::AttrNotSet(String::from("cake_id")))
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn test_derive_try_into_model_2() {
        mod my_fruit {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "fruit")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                pub name: String,
                #[pgorm(ignore)]
                pub cake_id: Option<i32>,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }
        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(1),
                name: Set("Pineapple".to_owned()),
            }
            .try_into_model()
            .unwrap(),
            my_fruit::Model {
                id: 1,
                name: "Pineapple".to_owned(),
                cake_id: None,
            }
        );
    }

    #[test]
    #[cfg(feature = "macros")]
    fn test_derive_try_into_model_3() {
        mod my_fruit {
            use crate as pgorm;
            use crate::entity::prelude::*;

            #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
            #[pgorm(table_name = "fruit")]
            pub struct Model {
                #[pgorm(primary_key)]
                pub id: i32,
                #[pgorm(ignore)]
                pub name: String,
                pub cake_id: Option<i32>,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ActiveModelBehavior for ActiveModel {}
        }
        assert_eq!(
            my_fruit::ActiveModel {
                id: Set(1),
                cake_id: Set(Some(1)),
            }
            .try_into_model()
            .unwrap(),
            my_fruit::Model {
                id: 1,
                name: "".to_owned(),
                cake_id: Some(1),
            }
        );
    }

    #[test]
    #[cfg(feature = "with-json")]
    #[should_panic(
        expected = r#"called `Result::unwrap()` on an `Err` value: Json("missing field `id`")"#
    )]
    fn test_active_model_set_from_json_1() {
        let mut cake: cake::ActiveModel = Default::default();

        cake.set_from_json(json!({
            "name": "Apple Pie",
        }))
        .unwrap();
    }

    #[test]
    #[cfg(feature = "with-json")]
    fn test_active_model_set_from_json_2() -> Result<(), Error> {
        let mut fruit: fruit::ActiveModel = Default::default();

        fruit.set_from_json(json!({
            "name": "Apple",
        }))?;
        assert_eq!(
            fruit,
            fruit::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::NotSet,
            }
        );

        assert_eq!(
            fruit::ActiveModel::from_json(json!({
                "name": "Apple",
            }))?,
            fruit::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::NotSet,
            }
        );

        fruit.set_from_json(json!({
            "name": "Apple",
            "cake_id": null,
        }))?;
        assert_eq!(
            fruit,
            fruit::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::Set(None),
            }
        );

        fruit.set_from_json(json!({
            "id": null,
            "name": "Apple",
            "cake_id": 1,
        }))?;
        assert_eq!(
            fruit,
            fruit::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::Set(Some(1)),
            }
        );

        fruit.set_from_json(json!({
            "id": 2,
            "name": "Apple",
            "cake_id": 1,
        }))?;
        assert_eq!(
            fruit,
            fruit::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::Set(Some(1)),
            }
        );

        let mut fruit = fruit::ActiveModel {
            id: ActiveValue::Set(1),
            name: ActiveValue::NotSet,
            cake_id: ActiveValue::NotSet,
        };
        fruit.set_from_json(json!({
            "id": 8,
            "name": "Apple",
            "cake_id": 1,
        }))?;
        assert_eq!(
            fruit,
            fruit::ActiveModel {
                id: ActiveValue::Set(1),
                name: ActiveValue::Set("Apple".to_owned()),
                cake_id: ActiveValue::Set(Some(1)),
            }
        );

        Ok(())
    }

    #[test]
    fn test_active_model_is_changed() {
        let mut fruit: fruit::ActiveModel = Default::default();
        assert!(!fruit.is_changed());

        fruit
            .set(fruit::Column::Name, "apple".into())
            .expect("Name is a column of fruit::ActiveModel");
        assert!(fruit.is_changed());
    }

    #[test]
    fn test_reset_1() {
        assert_eq!(
            fruit::Model {
                id: 1,
                name: "Apple".into(),
                cake_id: None,
            }
            .into_active_model(),
            fruit::ActiveModel {
                id: Unchanged(1),
                name: Unchanged("Apple".into()),
                cake_id: Unchanged(None)
            },
        );

        assert_eq!(
            fruit::Model {
                id: 1,
                name: "Apple".into(),
                cake_id: None,
            }
            .into_active_model()
            .reset_all(),
            fruit::ActiveModel {
                id: Set(1),
                name: Set("Apple".into()),
                cake_id: Set(None)
            },
        );

        assert_eq!(
            fruit::Model {
                id: 1,
                name: "Apple".into(),
                cake_id: Some(2),
            }
            .into_active_model(),
            fruit::ActiveModel {
                id: Unchanged(1),
                name: Unchanged("Apple".into()),
                cake_id: Unchanged(Some(2)),
            },
        );

        assert_eq!(
            fruit::Model {
                id: 1,
                name: "Apple".into(),
                cake_id: Some(2),
            }
            .into_active_model()
            .reset_all(),
            fruit::ActiveModel {
                id: Set(1),
                name: Set("Apple".into()),
                cake_id: Set(Some(2)),
            },
        );
    }
}
