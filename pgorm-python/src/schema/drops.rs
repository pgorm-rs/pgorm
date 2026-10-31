//! `IF EXISTS` and the behavior every `DROP` statement takes, and the list of
//! objects most of them drop, said once for all of them.

use super::args::behavior;
use pgorm::pgorm_query::{
    Domain, DomainDropStatement, DropBehavior, MaterializedView, MaterializedViewDropStatement,
    Name, Schema, SchemaDropStatement, Sequence, SequenceDropStatement, Table, TableDropStatement,
    TableName, View, ViewDropStatement,
    extension::{ExtensionDropStatement, Type, TypeDropStatement, TypeRef},
};
use pyo3::prelude::*;

pub(super) trait DropStatement {
    fn if_exists(&mut self);
    fn cascade(&mut self);
    fn restrict(&mut self);
}

/// A `DROP` statement over a list of objects, never empty: it starts at the
/// first and appends the rest.
pub(super) trait DropList: DropStatement + Sized {
    type Name;
    fn start(first: Self::Name) -> Self;
    fn add(&mut self, name: Self::Name);
}

macro_rules! drops {
    ($($kind:ty),* $(,)?) => {
        $(
            impl DropStatement for $kind {
                fn if_exists(&mut self) {
                    <$kind>::if_exists(self);
                }

                fn cascade(&mut self) {
                    <$kind>::cascade(self);
                }

                fn restrict(&mut self) {
                    <$kind>::restrict(self);
                }
            }
        )*
    };
}

drops!(
    TableDropStatement,
    ViewDropStatement,
    MaterializedViewDropStatement,
    SequenceDropStatement,
    TypeDropStatement,
    DomainDropStatement,
    SchemaDropStatement,
    ExtensionDropStatement,
);

macro_rules! drop_lists {
    ($($kind:ty: $name:ty = $start:expr, $add:ident);* $(;)?) => {
        $(
            impl DropList for $kind {
                type Name = $name;

                fn start(first: $name) -> Self {
                    $start(first)
                }

                fn add(&mut self, name: $name) {
                    self.$add(name);
                }
            }
        )*
    };
}

drop_lists! {
    TableDropStatement: TableName = Table::drop, table;
    ViewDropStatement: TableName = View::drop, name;
    MaterializedViewDropStatement: TableName = MaterializedView::drop, name;
    SequenceDropStatement: TableName = Sequence::drop, name;
    TypeDropStatement: TypeRef = Type::drop, name;
    DomainDropStatement: TypeRef = Domain::drop, name;
    SchemaDropStatement: Name = Schema::drop, name;
}

/// `drop` with `IF EXISTS` when asked and the behavior given, `"cascade"` or
/// `"restrict"`, written only when it is.
pub(super) fn dropping<D: DropStatement>(
    mut drop: D,
    if_exists: bool,
    behavior: Option<&str>,
) -> PyResult<D> {
    if if_exists {
        drop.if_exists();
    }
    match self::behavior(behavior)? {
        Some(DropBehavior::Cascade) => drop.cascade(),
        Some(DropBehavior::Restrict) => drop.restrict(),
        None => {}
    }
    Ok(drop)
}

/// A drop of the first object and the rest in order, with [`dropping`]'s
/// `IF EXISTS` and behavior.
pub(super) fn drop_list<D: DropList>(
    (first, rest): (D::Name, Vec<D::Name>),
    if_exists: bool,
    behavior: Option<&str>,
) -> PyResult<D> {
    let mut drop = D::start(first);
    for name in rest {
        drop.add(name);
    }
    dropping(drop, if_exists, behavior)
}
