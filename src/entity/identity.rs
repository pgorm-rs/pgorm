use crate::{ColumnTrait, EntityTrait, IdenStatic};
use pgorm_query::{Alias, DynIden, Iden, IntoIden, IntoValueTuple, SeaRc, Value, ValueTuple};
use std::fmt;

/// List of column identifier
// [spec:pgorm:def:entity.relation.def+2]
#[derive(Debug, Clone)]
pub enum Identity {
    /// Column identifier consists of 1 column
    Unary(DynIden),
    /// Column identifier consists of 2 columns
    Binary(DynIden, DynIden),
    /// Column identifier consists of 3 columns
    Ternary(DynIden, DynIden, DynIden),
    /// Column identifier consists of more than 3 columns
    Many(Vec<DynIden>),
}

impl IntoIterator for Identity {
    type Item = DynIden;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            Identity::Unary(ident1) => vec![ident1].into_iter(),
            Identity::Binary(ident1, ident2) => vec![ident1, ident2].into_iter(),
            Identity::Ternary(ident1, ident2, ident3) => vec![ident1, ident2, ident3].into_iter(),
            Identity::Many(vec) => vec.into_iter(),
        }
    }
}

impl Iden for Identity {
    fn unquoted(&self, s: &mut dyn fmt::Write) {
        match self {
            Identity::Unary(iden) => {
                write!(s, "{}", iden.to_string()).expect("write to sql sink");
            }
            Identity::Binary(iden1, iden2) => {
                write!(s, "{}", iden1.to_string()).expect("write to sql sink");
                write!(s, "{}", iden2.to_string()).expect("write to sql sink");
            }
            Identity::Ternary(iden1, iden2, iden3) => {
                write!(s, "{}", iden1.to_string()).expect("write to sql sink");
                write!(s, "{}", iden2.to_string()).expect("write to sql sink");
                write!(s, "{}", iden3.to_string()).expect("write to sql sink");
            }
            Identity::Many(vec) => {
                for iden in vec.iter() {
                    write!(s, "{}", iden.to_string()).expect("write to sql sink");
                }
            }
        }
    }
}

/// The columns a relation joins on, held as `(from, to)` pairs so the two
/// sides of the join cannot disagree in length.
///
/// The only constructor takes the first pair, and every extension takes a pair,
/// so a set of join columns is non-empty and balanced by construction: there is
/// no unbalanced value to build, pass around, or truncate.
// [spec:pgorm:def:entity.relation.def+2]
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
        self.side_identity(|pair| SeaRc::clone(&pair.0))
    }

    /// The `to` side of every pair, as an [`Identity`].
    pub fn to_identity(&self) -> Identity {
        self.side_identity(|pair| SeaRc::clone(&pair.1))
    }

    fn side_identity<F>(&self, col: F) -> Identity
    where
        F: Fn(&(DynIden, DynIden)) -> DynIden,
    {
        match self.rest.as_slice() {
            [] => Identity::Unary(col(&self.first)),
            [second] => Identity::Binary(col(&self.first), col(second)),
            [second, third] => Identity::Ternary(col(&self.first), col(second), col(third)),
            rest => Identity::Many(
                std::iter::once(col(&self.first))
                    .chain(rest.iter().map(col))
                    .collect(),
            ),
        }
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
// [spec:pgorm:def:entity.relation.def+2]
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
// [spec:pgorm:def:entity.relation.def+2]
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
        Identity::Unary(SeaRc::new(Alias::new(self)))
    }
}

impl<T> IntoIdentity for T
where
    T: IdenStatic,
{
    type ValueType = Value;

    fn into_identity(self) -> Identity {
        Identity::Unary(self.into_iden())
    }
}

impl<T, C> IntoIdentity for (T, C)
where
    T: IdenStatic,
    C: IdenStatic,
{
    type ValueType = (Value, Value);

    fn into_identity(self) -> Identity {
        Identity::Binary(self.0.into_iden(), self.1.into_iden())
    }
}

impl<T, C> IntoBoundary<(Value, Value)> for (T, C)
where
    T: Into<Value>,
    C: Into<Value>,
{
}

impl<T, C, R> IntoIdentity for (T, C, R)
where
    T: IdenStatic,
    C: IdenStatic,
    R: IdenStatic,
{
    type ValueType = (Value, Value, Value);

    fn into_identity(self) -> Identity {
        Identity::Ternary(self.0.into_iden(), self.1.into_iden(), self.2.into_iden())
    }
}

impl<T, C, R> IntoBoundary<(Value, Value, Value)> for (T, C, R)
where
    T: Into<Value>,
    C: Into<Value>,
    R: Into<Value>,
{
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
            $($T: IdenStatic),+
        {
            type ValueType = ( $(boundary_element!($T)),+ );

            fn into_identity(self) -> Identity {
                Identity::Many(vec![
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
