use crate::{ColumnTrait, EntityTrait, IdenStr};
use pgorm_query::{Alias, DynIden, Iden, IntoIden, IntoValueTuple, SharedIden, Value, ValueTuple};
use std::fmt;

/// The columns a lookup keys on, in declared order.
///
/// One arity-agnostic representation, whether the key is a single column or a
/// composite: every consumer walks the columns rather than dispatching on how
/// many there are, and a column set of a given width has exactly one spelling.
// [spec:pgorm:def:entity.relation.def+7]
#[derive(Debug, Clone)]
pub struct Identity(Vec<DynIden>);

impl Identity {
    /// The number of columns.
    // [spec:pgorm:def:entity.relation.def+7]
    pub fn arity(&self) -> usize {
        self.0.len()
    }

    /// Iterate the columns in declared order.
    // [spec:pgorm:def:entity.relation.def+7]
    pub fn iter(&self) -> impl Iterator<Item = &DynIden> {
        self.0.iter()
    }

    /// The one column of a unary set, or `None` when the set is wider: what a
    /// consumer that can only act on one column asks, instead of dispatching on
    /// arity.
    // [spec:pgorm:def:entity.relation.def+7]
    pub fn single(&self) -> Option<&DynIden> {
        match self.0.as_slice() {
            [only] => Some(only),
            _ => None,
        }
    }
}

// [spec:pgorm:def:entity.relation.def+7]
impl From<DynIden> for Identity {
    fn from(iden: DynIden) -> Self {
        Self(vec![iden])
    }
}

// [spec:pgorm:def:entity.relation.def+7]
impl From<Vec<DynIden>> for Identity {
    fn from(idens: Vec<DynIden>) -> Self {
        Self(idens)
    }
}

// [spec:pgorm:def:entity.relation.def+7]
impl FromIterator<DynIden> for Identity {
    fn from_iter<I: IntoIterator<Item = DynIden>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Identity {
    type Item = DynIden;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Iden for Identity {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        for iden in self.iter() {
            write!(s, "{}", iden.to_string()).expect("write to sql sink");
        }
    }
}

/// The columns a relation joins on, held as `(from, to)` pairs so the two
/// sides of the join cannot disagree in length.
///
/// The only constructor takes the first pair, and every extension takes a pair,
/// so a set of join columns is non-empty and balanced by construction: there is
/// no unbalanced value to build, pass around, or truncate.
// [spec:pgorm:def:entity.relation.def+7]
#[derive(Debug, Clone)]
pub struct ColumnPairs {
    first: (DynIden, DynIden),
    rest: Vec<(DynIden, DynIden)>,
}

impl ColumnPairs {
    /// Start a set of join columns from its first `(from, to)` pair.
    pub fn new<F, T>(from: F, to: T) -> Self
    where
        F: IntoIden,
        T: IntoIden,
    {
        Self {
            first: (from.into_iden(), to.into_iden()),
            rest: Vec::new(),
        }
    }

    /// Extend with a further pair, as a composite key requires.
    #[must_use]
    pub fn and<F, T>(mut self, from: F, to: T) -> Self
    where
        F: IntoIden,
        T: IntoIden,
    {
        self.push(from, to);
        self
    }

    /// Append a pair in place.
    pub fn push<F, T>(&mut self, from: F, to: T)
    where
        F: IntoIden,
        T: IntoIden,
    {
        self.rest.push((from.into_iden(), to.into_iden()));
    }

    /// Iterate the pairs in declaration order.
    pub fn iter(&self) -> impl Iterator<Item = &(DynIden, DynIden)> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    /// The first pair, which every set has.
    pub fn first(&self) -> &(DynIden, DynIden) {
        &self.first
    }

    /// The number of pairs, which is at least one.
    pub fn arity(&self) -> usize {
        1 + self.rest.len()
    }

    /// Swap every pair, so the relation reads in the opposite direction.
    #[must_use]
    pub fn rev(self) -> Self {
        Self {
            first: (self.first.1, self.first.0),
            rest: self.rest.into_iter().map(|(f, t)| (t, f)).collect(),
        }
    }

    /// The `from` side of every pair, as an [`Identity`].
    pub fn from_identity(&self) -> Identity {
        self.side_identity(|pair| SharedIden::clone(&pair.0))
    }

    /// The `to` side of every pair, as an [`Identity`].
    pub fn to_identity(&self) -> Identity {
        self.side_identity(|pair| SharedIden::clone(&pair.1))
    }

    fn side_identity<F>(&self, col: F) -> Identity
    where
        F: Fn(&(DynIden, DynIden)) -> DynIden,
    {
        self.iter().map(col).collect()
    }
}

impl IntoIterator for ColumnPairs {
    type Item = (DynIden, DynIden);
    type IntoIter = std::iter::Chain<std::iter::Once<Self::Item>, std::vec::IntoIter<Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self.first).chain(self.rest)
    }
}

/// Performs a conversion into an [Identity]
// [spec:pgorm:def:entity.relation.def+7]
pub trait IntoIdentity {
    /// The shape a boundary value must have to line up with this identity: a
    /// tuple of [`Value`] of the same length, so the arity of a column set and
    /// the arity of the values compared against it are one fact rather than
    /// two. [`Identity`] itself, whose arity is only known at runtime, maps to
    /// [`ValueTuple`].
    type ValueType: IntoValueTuple;

    /// Method to perform the conversion
    fn into_identity(self) -> Identity;
}

/// A value tuple whose arity matches the order-key shape `K`.
///
/// `K` is the [`IntoIdentity::ValueType`] of a column set, so a tuple of the
/// wrong length has no implementation here and is rejected at compile time.
/// The exception is `K = ValueTuple`, the shape of a runtime-built
/// [`Identity`], which accepts any tuple and leaves the arity to be checked
/// when the query runs.
// [spec:pgorm:def:entity.relation.def+7]
pub trait IntoBoundary<K>: IntoValueTuple {}

/// Check the [Identity] of an Entity
pub trait IdentityOf<E>: IntoIdentity
where
    E: EntityTrait,
{
    /// Method to call to perform this check
    fn identity_of(self) -> Identity;
}

impl<T> IntoBoundary<ValueTuple> for T where T: IntoValueTuple {}

impl IntoIdentity for Identity {
    type ValueType = ValueTuple;

    fn into_identity(self) -> Identity {
        self
    }
}

impl<V> IntoBoundary<Value> for V where V: Into<Value> {}

impl IntoIdentity for String {
    type ValueType = Value;

    fn into_identity(self) -> Identity {
        self.as_str().into_identity()
    }
}

impl IntoIdentity for &str {
    type ValueType = Value;

    fn into_identity(self) -> Identity {
        Identity::from(SharedIden::new(Alias::new(self)))
    }
}

impl<T> IntoIdentity for T
where
    T: IdenStr,
{
    type ValueType = Value;

    fn into_identity(self) -> Identity {
        Identity::from(self.into_iden())
    }
}

/// Expands to [`Value`] once per type parameter of a tuple impl, so the
/// boundary shape is built from the same repetition as the tuple itself.
macro_rules! boundary_element {
    ( $T:ident ) => {
        Value
    };
}

macro_rules! impl_into_identity {
    ( $($T:ident : $N:tt),+ $(,)? ) => {
        impl< $($T),+ > IntoIdentity for ( $($T),+ )
        where
            $($T: IdenStr),+
        {
            type ValueType = ( $(boundary_element!($T)),+ );

            fn into_identity(self) -> Identity {
                Identity::from(vec![
                    $(self.$N.into_iden()),+
                ])
            }
        }

        impl< $($T),+ > IntoBoundary<( $(boundary_element!($T)),+ )> for ( $($T),+ )
        where
            $($T: Into<Value>),+
        {
        }
    };
}

#[rustfmt::skip]
mod impl_into_identity {
    use super::*;

    impl_into_identity!(T0:0, T1:1);
    impl_into_identity!(T0:0, T1:1, T2:2);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6, T7:7);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6, T7:7, T8:8);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6, T7:7, T8:8, T9:9);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6, T7:7, T8:8, T9:9, T10:10);
    impl_into_identity!(T0:0, T1:1, T2:2, T3:3, T4:4, T5:5, T6:6, T7:7, T8:8, T9:9, T10:10, T11:11);
}

impl<E, C> IdentityOf<E> for C
where
    E: EntityTrait<Column = C>,
    C: ColumnTrait,
{
    fn identity_of(self) -> Identity {
        self.into_identity()
    }
}

macro_rules! impl_identity_of {
    ( $($T:ident),+ $(,)? ) => {
        impl<E, C> IdentityOf<E> for ( $($T),+ )
        where
            E: EntityTrait<Column = C>,
            C: ColumnTrait,
        {
            fn identity_of(self) -> Identity {
                self.into_identity()
            }
        }
    };
}

#[rustfmt::skip]
mod impl_identity_of {
    use super::*;

    impl_identity_of!(C, C);
    impl_identity_of!(C, C, C);
    impl_identity_of!(C, C, C, C);
    impl_identity_of!(C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C, C, C, C, C);
    impl_identity_of!(C, C, C, C, C, C, C, C, C, C, C, C);
}
