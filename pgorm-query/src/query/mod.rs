//! Query statements (select, insert, update, delete & merge).
//!
//! # Usage
//!
//! - Query Select, see [`SelectStatement`]
//! - Query Insert, see [`InsertStatement`]
//! - Query Update, see [`UpdateStatement`]
//! - Query Delete, see [`DeleteStatement`]
//! - Query Merge, see [`MergeStatement`]

use crate::{IntoFromItem, IntoNamedTable};

mod case;
mod condition;
mod delete;
mod frame;
mod grouping;
mod insert;
mod merge;
mod on_conflict;
mod ordered;
mod returning;
mod select;
mod select_expr;
mod traits;
mod update;
mod window;
mod with;

pub use case::*;
pub use condition::*;
pub use delete::*;
pub use frame::*;
pub use grouping::*;
pub use insert::*;
pub use merge::*;
pub use on_conflict::*;
pub use ordered::*;
pub use returning::*;
pub use select::*;
pub use select_expr::*;
pub use traits::*;
pub use update::*;
pub use window::*;

pub(crate) use frame::FrameBound;
pub(crate) use grouping::GroupingKind;
pub use with::*;

/// Shorthand for constructing any table query
// [spec:pgorm:req:sql.ast+3]
#[derive(Debug, Clone)]
pub struct Query;

/// All available types of table query
#[derive(Debug, Clone)]
pub enum QueryStatement {
    Select(SelectStatement),
    Insert(InsertStatement),
    Update(UpdateStatement),
    Delete(DeleteStatement),
}

// [spec:pgorm:req:sql.ast+3]
#[derive(Debug, Clone, PartialEq)]
pub enum SubQueryStatement {
    SelectStatement(SelectStatement),
    InsertStatement(InsertStatement),
    UpdateStatement(UpdateStatement),
    DeleteStatement(DeleteStatement),
    MergeStatement(Box<MergeStatement>),
}

impl Query {
    /// Construct table [`SelectStatement`]
    pub fn select() -> SelectStatement {
        SelectStatement::new()
    }

    /// Construct table [`InsertStatement`]
    pub fn insert() -> InsertStatement {
        InsertStatement::new()
    }

    /// Construct table [`UpdateStatement`]
    pub fn update() -> UpdateStatement {
        UpdateStatement::new()
    }

    /// Construct table [`DeleteStatement`]
    pub fn delete() -> DeleteStatement {
        DeleteStatement::new()
    }

    /// Begin a `MERGE` into `target` from `source`, pairing their rows where
    /// `on` holds. The result is a [`PendingMerge`], which becomes a
    /// [`MergeStatement`] when its first `WHEN` arm is added.
    ///
    /// The target is a table name, optionally schema-qualified and aliased,
    /// and the source is any relation a `FROM` clause takes: a table, a
    /// subquery, a values list, a function call or a validated fragment.
    // [spec:pgorm:req:sql.ast.merge+1]
    pub fn merge<T, S, C>(target: T, source: S, on: C) -> PendingMerge
    where
        T: IntoNamedTable,
        S: IntoFromItem,
        C: IntoCondition,
    {
        PendingMerge {
            target: target.into_named_table(),
            source: source.into_from_item(),
            on: on.into_condition(),
        }
    }

    /// Construct [`WithClause`] around its first common table expression
    pub fn with(cte: CommonTableExpression) -> WithClause {
        WithClause::new(cte)
    }

    /// Construct [`RecursiveWithClause`] around the one common table expression it may hold
    pub fn with_recursive(cte: CommonTableExpression) -> RecursiveWithClause {
        RecursiveWithClause::new(cte)
    }

    /// Construct [`Returning`]
    pub fn returning() -> Returning {
        Returning::new()
    }
}
