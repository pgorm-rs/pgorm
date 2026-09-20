//! What a file that talks to the database needs in scope.
//!
//! The set is chosen from what code actually writes: the entity trait family
//! and the derives that produce it, the query-builder traits whose methods
//! are otherwise unreachable, the `ActiveValue` vocabulary, the CRUD entry
//! points, the decode targets, and the connection types.
//!
//! One name deliberately stays out. `Order` — the `ASC`/`DESC` enum — is a
//! plausible table name, and an entity aliased `Order` next to a glob of this
//! module makes every mention of it ambiguous; `order_by_asc` / `order_by_desc`
//! cover the common case and the enum is one import away.
//!
//! `StaticName` is here — pgorm_query's, re-exported. The entity layer used to
//! declare a second trait of its own with the same method, so this module and
//! `pgorm_query`'s could not both be glob-imported without ambiguity; there is
//! now one trait and the question does not arise.
//!
//! `alias` and its `AliasName` are here for the same reason `Expr` is: a name
//! the query introduces is written at the point the query is written, and
//! writing it as a token instead of a string is only cheaper than
//! `Name::runtime` if the token is already in scope.

// [spec:pgorm:def:entity.prelude+4]
// [spec:pgorm:sem:query.build.alias+2]
pub use crate::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ActiveValue,
    ActiveValue::{NotSet, Set, Unchanged},
    AliasName, ColumnDef, ColumnTrait, ColumnType, ColumnTypeTrait, Condition, ConnectionTrait,
    CursorTrait, DatabaseConnection, DatabasePool, DatabaseTransaction, DecodeRaw, DecodeSelect,
    Delete, EntityName, EntityTrait, EnumIter, FromQueryResult, Insert, IntoActiveModel,
    IntoActiveValue, Iterable, JoinType, Linked, LoaderTrait, ModelTrait, PaginatorTrait,
    PrimaryKeyArity, PrimaryKeyToColumn, PrimaryKeyTrait, QueryFilter, QueryOrder, QueryResult,
    QuerySelect, QueryTrait, Related, RelatedLink, RelationDef, RelationTrait, Select, SqlName,
    StaticName, TransactionTrait, TryInsert, TryIntoModel, Update, Value, alias,
    error::*,
    pgorm_query::{Expr, ForeignKeyAction, Name, StringLen},
    set,
};
pub use std::sync::Arc;

#[cfg(feature = "macros")]
pub use crate::{
    DeriveActiveEnum, DeriveActiveModel, DeriveActiveModelBehavior, DeriveColumn,
    DeriveCustomColumn, DeriveDisplay, DeriveEntity, DeriveEntityModel, DeriveIntoActiveModel,
    DeriveModel, DerivePartialModel, DerivePrimaryKey, DeriveRelation, DeriveSqlName,
    DeriveValueType,
};

pub use async_trait;

#[cfg(feature = "with-json")]
pub use serde_json::Value as Json;

#[cfg(feature = "with-jiff")]
pub use jiff::civil::{Date, DateTime, Time};

/// An absolute instant, carried by a `timestamptz` column.
///
/// Spelled this way rather than re-exporting `jiff::Timestamp` under its own
/// name: `ColumnType::Timestamp` is PostgreSQL's *naive* `timestamp`, which is
/// what [`DateTime`] maps to, so a field written `pub at: Timestamp` inferring
/// `TimestampWithTimeZone` would read backwards at every call site.
#[cfg(feature = "with-jiff")]
pub type DateTimeWithTimeZone = jiff::Timestamp;

pub use rust_decimal::Decimal;

#[cfg(feature = "with-uuid")]
pub use uuid::Uuid;

// [spec:pgorm:def:exec.decode.types+2]
pub use crate::pgorm_query::{IpNetwork, MacAddress, Vector};
