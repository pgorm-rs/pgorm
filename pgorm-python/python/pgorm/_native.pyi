from typing import Any
from asyncio import CancelledError as CancelledError
from ._schema import (
    DataType as DataType, ColumnDef as ColumnDef, DDL as DDL,
    CreateTable as CreateTable, CreateIndex as CreateIndex, EntitySchema as EntitySchema,
    schema_from_entity as schema_from_entity, drop_table as drop_table,
    rename_table as rename_table, rename_column as rename_column, truncate_table as truncate_table,
    add_column as add_column, modify_column as modify_column, drop_column as drop_column,
    set_expression as set_expression, drop_expression as drop_expression,
    add_primary_key as add_primary_key, add_unique as add_unique,
    drop_index as drop_index, create_enum as create_enum, add_enum_value as add_enum_value,
    rename_enum as rename_enum, rename_enum_value as rename_enum_value, drop_enum as drop_enum,
)
from ._schema_constraints import (
    add_not_null as add_not_null, add_check as add_check,
    validate_constraint as validate_constraint, alter_constraint as alter_constraint,
    drop_constraint as drop_constraint, rename_constraint as rename_constraint,
)
from ._schema_views import (
    create_view as create_view, create_recursive_view as create_recursive_view,
    drop_view as drop_view, create_materialized_view as create_materialized_view,
    refresh_materialized_view as refresh_materialized_view,
    drop_materialized_view as drop_materialized_view, create_table_as as create_table_as,
)
from ._schema_schemas import (
    create_schema as create_schema, drop_schema as drop_schema, rename_schema as rename_schema,
)
from ._schema_domains import (
    CreateDomain as CreateDomain, set_domain_default as set_domain_default,
    drop_domain_default as drop_domain_default, set_domain_not_null as set_domain_not_null,
    drop_domain_not_null as drop_domain_not_null, add_domain_check as add_domain_check,
    drop_domain_constraint as drop_domain_constraint,
    rename_domain_constraint as rename_domain_constraint,
    validate_domain_constraint as validate_domain_constraint, rename_domain as rename_domain,
    drop_domain as drop_domain,
)
from ._pipeline_builder import Pipeline as Pipeline, PipelineSource as PipelineSource, PipelineGrouped as PipelineGrouped
from ._pipeline_expr import PipelineExpr as PipelineExpr, PipelineBinder as PipelineBinder, PipelineOver as PipelineOver
from ._pipeline_sources import SourceSelection as SourceSelection, SelectedSources as SelectedSources
from ._pipeline_functions import (
    pipeline_source as pipeline_source, pipeline_literal as pipeline_literal,
    pipeline_alias as pipeline_alias, pipeline_col as pipeline_col,
    pipeline_role as pipeline_role, pipeline_function as pipeline_function,
    pipeline_case as pipeline_case, pipeline_sources as pipeline_sources,
)

__version__: str
__pgorm_version__: str

class PgormError(Exception): ...
class UnsupportedCapabilityError(PgormError): ...
class ConstructionError(PgormError): ...
class ConnectionError(PgormError): ...
class DatabaseError(PgormError):
    sqlstate: str | None
    message: str | None
    severity: str | None
    detail: str | None
    hint: str | None
    schema: str | None
    table: str | None
    column: str | None
    datatype: str | None
    constraint: str | None
class UniqueViolationError(DatabaseError): ...
class ForeignKeyViolationError(DatabaseError): ...
class CheckViolationError(DatabaseError): ...
class NotNullViolationError(DatabaseError): ...
class ExclusionViolationError(DatabaseError): ...
class DecodeError(PgormError): ...
class TimeoutError(PgormError): ...
class LifecycleError(PgormError): ...
class InternalError(PgormError): ...

def capabilities() -> dict[str, Any]: ...
def require_capability(operation: str) -> None: ...

class TypeName:
    def __init__(self, name: str, *, schema: str | None = ...) -> None: ...
    @property
    def name(self) -> str: ...
    @property
    def schema(self) -> str | None: ...

class Value:
    def __init__(self, value: Any, kind: str | TypeName | CreatedRange | CreatedMultirange | None = ...) -> None: ...
    @staticmethod
    def null(kind: str | TypeName | CreatedRange | CreatedMultirange) -> Value: ...
    @staticmethod
    def json(value: Any) -> Value: ...
    @staticmethod
    def array(kind: str | TypeName, values: list[Any] | tuple[Any, ...] | None) -> Value: ...
    @property
    def kind(self) -> str: ...
    @property
    def is_null(self) -> bool: ...
    @property
    def value(self) -> Any: ...
    @property
    def type_name(self) -> TypeName | None: ...
    @property
    def created_type(self) -> CreatedRange | CreatedMultirange | None: ...
    @property
    def element_type(self) -> str | TypeName | None: ...
    def items(self, /) -> list[Value] | None: ...
    def snapshot(self, /) -> dict[str, Any]: ...

from ._intervals import Interval as Interval
from ._ranges import (
    Range as Range, Multirange as Multirange,
    CreatedRange as CreatedRange, CreatedMultirange as CreatedMultirange,
)
from ._query_types import (
    Identifier as Identifier, Direction as Direction, Nulls as Nulls,
    Compiled as Compiled, LikePattern as LikePattern,
    Aliased as Aliased, OrderBy as OrderBy,
)
from ._expressions import (
    Expr as Expr, Condition as Condition, col as col, bind as bind,
    literal as literal, call as call, tuple_expr as tuple_expr, domain_value as domain_value,
)
from ._statements import (
    Table as Table, FromItem as FromItem, Select as Select, Join as Join,
    SampleMethod as SampleMethod, TableFunction as TableFunction,
    GroupingElement as GroupingElement,
)
from ._subqueries import (
    array as array, exists as exists, scalar as scalar,
    Quantified as Quantified, any_ as any_, all_ as all_,
)
from ._windows import (
    FrameType as FrameType, FrameExclusion as FrameExclusion, Frame as Frame,
    FramePrecedingStart as FramePrecedingStart, FrameCurrentRowStart as FrameCurrentRowStart,
    FrameFollowingStart as FrameFollowingStart, Window as Window,
    ExtendedWindow as ExtendedWindow, Windowed as Windowed,
    WindowFunction as WindowFunction, window_function as window_function,
)
from ._json import (
    JsonInput as JsonInput, JsonKind as JsonKind, JsonExistsBehavior as JsonExistsBehavior,
    JsonValueBehavior as JsonValueBehavior, JsonQueryBehavior as JsonQueryBehavior,
    JsonDefault as JsonDefault, JsonTableBehavior as JsonTableBehavior,
    JsonTableColumn as JsonTableColumn, json_table as json_table, json_exists as json_exists, json_value as json_value,
    json_query as json_query, json_object as json_object, json_array as json_array,
    json_array_query as json_array_query, json_objectagg as json_objectagg,
    json_arrayagg as json_arrayagg, json_parse as json_parse, json_scalar as json_scalar,
    json_serialize as json_serialize, format_json as format_json,
    is_json as is_json, is_not_json as is_not_json,
)
from ._results import Record as Record, Field as Field
from ._entities import Entity as Entity, EntityColumn as EntityColumn, EntityQuery as EntityQuery, entity as entity
from ._versions import (
    Change as Change, Upserted as Upserted, EntityUpdate as EntityUpdate,
    EntityUpdateMany as EntityUpdateMany, EntityInsert as EntityInsert,
    EntityInsertMany as EntityInsertMany,
)
from ._graphs import EntityGraph as EntityGraph, EntityGraphQuery as EntityGraphQuery, EntityGraphCursor as EntityGraphCursor, graph as graph
from ._entity_models import EntityModel as EntityModel, ActiveModel as ActiveModel, ActiveValue as ActiveValue, ActiveState as ActiveState
from ._writes import (
    Insert as Insert, Update as Update, Delete as Delete, RawSQL as RawSQL,
    Conflict as Conflict, ConflictTarget as ConflictTarget, ConflictUpdate as ConflictUpdate,
)
from ._merge import (
    MatchedAction as MatchedAction, NotMatchedAction as NotMatchedAction, Overriding as Overriding,
    MergeUpdate as MergeUpdate, MergeInsert as MergeInsert, PendingMerge as PendingMerge,
    Merge as Merge, With as With, ReturningRow as ReturningRow, merge as merge,
)
from ._copy import (
    CopyFrom as CopyFrom, CopyTo as CopyTo, _CopyIn as _CopyIn, _CopyOut as _CopyOut,
    _copy_in as _copy_in, _copy_out as _copy_out,
)
from ._listen import Notification as Notification, _Listener as _Listener
from ._runtime import (
    _Pool as _Pool, _Connection as _Connection, _Stream as _Stream,
    _Transaction as _Transaction, STATEMENT_CACHE_CHANNEL as STATEMENT_CACHE_CHANNEL,
)
