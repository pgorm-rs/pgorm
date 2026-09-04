//! What a file that talks to the database needs in scope.
//!
//! The set is chosen from what code actually writes: the entity trait family
//! and the derives that produce it, the query-builder traits whose methods
//! are otherwise unreachable, the `ActiveValue` vocabulary, the CRUD entry
//! points, the decode targets, and the connection types.
//!
//! Two names deliberately stay out. `Order` — the `ASC`/`DESC` enum — is a
//! plausible table name, and an entity aliased `Order` next to a glob of this
//! module makes every mention of it ambiguous; `order_by_asc` / `order_by_desc`
//! cover the common case and the enum is one import away. `IdenStatic` is
//! here, but `pgorm_query::IdenStatic` is a different trait of the same name,
//! so glob-importing both modules is an error waiting for its first use.

// [spec:pgorm:def:entity.prelude]
pub use crate::{
    ActiveEnum, ActiveModelBehavior, ActiveModelTrait, ActiveValue,
    ActiveValue::{NotSet, Set, Unchanged},
    ColumnDef, ColumnTrait, ColumnType, ColumnTypeTrait, Condition, ConnectionTrait, CursorTrait,
    DatabaseConnection, DatabasePool, DatabaseTransaction, DecodeRaw, DecodeSelect, Delete,
    EntityName, EntityTrait, EnumIter, FromQueryResult, Iden, IdenStatic, Insert, IntoActiveModel,
    IntoActiveValue, Iterable, JoinType, Linked, LoaderTrait, ModelTrait, PaginatorTrait,
    PrimaryKeyArity, PrimaryKeyToColumn, PrimaryKeyTrait, QueryFilter, QueryOrder, QueryResult,
    QuerySelect, QueryTrait, Related, RelationDef, RelationTrait, Select, TransactionTrait,
    TryInsert, TryIntoModel, Update, Value,
    error::*,
    pgorm_query::{DynIden, Expr, ForeignKeyAction, RcOrArc, SeaRc, StringLen},
    set,
};

#[cfg(feature = "macros")]
pub use crate::{
    DeriveActiveEnum, DeriveActiveModel, DeriveActiveModelBehavior, DeriveColumn,
    DeriveCustomColumn, DeriveDisplay, DeriveEntity, DeriveEntityModel, DeriveIden,
    DeriveIntoActiveModel, DeriveModel, DerivePartialModel, DerivePrimaryKey, DeriveRelatedEntity,
    DeriveRelation, DeriveValueType,
};

pub use async_trait;

#[cfg(feature = "with-json")]
pub use serde_json::Value as Json;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveDate as Date;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveTime as Time;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveDateTime as DateTime;

/// Date time with fixed offset
#[cfg(feature = "with-chrono")]
pub type DateTimeWithTimeZone = chrono::DateTime<chrono::FixedOffset>;

/// Date time represented in UTC
#[cfg(feature = "with-chrono")]
pub type DateTimeUtc = chrono::DateTime<chrono::Utc>;

/// Date time represented in local time
#[cfg(feature = "with-chrono")]
pub type DateTimeLocal = chrono::DateTime<chrono::Local>;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveDate as ChronoDate;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveTime as ChronoTime;

#[cfg(feature = "with-chrono")]
pub use chrono::NaiveDateTime as ChronoDateTime;

/// Date time with fixed offset
#[cfg(feature = "with-chrono")]
pub type ChronoDateTimeWithTimeZone = chrono::DateTime<chrono::FixedOffset>;

/// Date time represented in UTC
#[cfg(feature = "with-chrono")]
pub type ChronoDateTimeUtc = chrono::DateTime<chrono::Utc>;

/// Date time represented in local time
#[cfg(feature = "with-chrono")]
pub type ChronoDateTimeLocal = chrono::DateTime<chrono::Local>;

#[cfg(feature = "with-rust_decimal")]
pub use rust_decimal::Decimal;

#[cfg(feature = "with-uuid")]
pub use uuid::Uuid;

// [spec:pgorm:def:exec.decode.types+1]
pub use crate::pgorm_query::{IpNetwork, MacAddress, Vector};
