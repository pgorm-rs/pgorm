use serde_json::{Map, Value, json};

pub(crate) fn operations() -> Map<String, Value> {
    [
        ("table", "pgorm_query::NamedTable, pgorm_query::TableName"),
        (
            "table.col",
            "pgorm_query::Expr::col, pgorm_query::ColumnRef",
        ),
        ("table.star", "pgorm_query::ColumnRef::TableAsterisk"),
        ("table.as_", "pgorm_query::NamedTable::alias"),
        ("select", "pgorm_query::Query::select"),
        (
            "select.select",
            "pgorm_query::SelectStatement::clear_selects, expr, expr_as",
        ),
        ("select.from_", "pgorm_query::SelectStatement::from"),
        ("select.where_", "pgorm_query::SelectStatement::cond_where"),
        ("select.join", "pgorm_query::SelectStatement::join"),
        (
            "select.cross_join",
            "pgorm_query::SelectStatement::cross_join",
        ),
        (
            "select.group_by",
            "pgorm_query::SelectStatement::add_group_by",
        ),
        ("select.having", "pgorm_query::SelectStatement::cond_having"),
        (
            "select.order_by",
            "pgorm_query::SelectStatement::order_by_expr, order_by_expr_with_nulls",
        ),
        (
            "select.limit",
            "pgorm_query::SelectStatement::limit, reset_limit",
        ),
        (
            "select.offset",
            "pgorm_query::SelectStatement::offset, reset_offset",
        ),
        ("select.distinct", "pgorm_query::SelectStatement::distinct"),
        ("select.inspect", "pgorm_query::SelectStatement::build"),
        (
            "insert",
            "pgorm_query::Query::insert, InsertStatement::into_table",
        ),
        ("insert.columns", "pgorm_query::InsertStatement::columns"),
        ("insert.values", "pgorm_query::InsertStatement::values"),
        (
            "insert.default_values",
            "pgorm_query::InsertStatement::or_default_values",
        ),
        (
            "insert.on_conflict",
            "pgorm_query::InsertStatement::on_conflict",
        ),
        (
            "insert.returning",
            "pgorm_query::InsertStatement::returning",
        ),
        ("insert.inspect", "pgorm_query::InsertStatement::build"),
        (
            "update",
            "pgorm_query::Query::update, UpdateStatement::table",
        ),
        ("update.set", "pgorm_query::UpdateStatement::value"),
        ("update.where_", "pgorm_query::UpdateStatement::cond_where"),
        (
            "update.all_rows",
            "pgorm_query::UpdateStatement with explicit unrestricted intent",
        ),
        (
            "update.returning",
            "pgorm_query::UpdateStatement::returning",
        ),
        ("update.inspect", "pgorm_query::UpdateStatement::build"),
        (
            "delete",
            "pgorm_query::Query::delete, DeleteStatement::from_table",
        ),
        ("delete.where_", "pgorm_query::DeleteStatement::cond_where"),
        (
            "delete.all_rows",
            "pgorm_query::DeleteStatement with explicit unrestricted intent",
        ),
        (
            "delete.returning",
            "pgorm_query::DeleteStatement::returning",
        ),
        ("delete.inspect", "pgorm_query::DeleteStatement::build"),
        ("conflict.ignore", "pgorm_query::OnConflict::do_nothing"),
        (
            "conflict.target",
            "pgorm_query::OnConflict::column, ConflictTarget::and_column",
        ),
        (
            "conflict.target.where_",
            "pgorm_query::ConflictTarget::cond_where",
        ),
        (
            "conflict.target.ignore",
            "pgorm_query::ConflictTarget::do_nothing",
        ),
        (
            "conflict.target.update",
            "pgorm_query::ConflictTarget::update_column, ConflictUpdate::update_column",
        ),
        ("conflict.target.set", "pgorm_query::ConflictTarget::value"),
        ("conflict.update.set", "pgorm_query::ConflictUpdate::value"),
        (
            "conflict.update.update",
            "pgorm_query::ConflictUpdate::update_column",
        ),
        (
            "conflict.update.where_",
            "pgorm_query::ConflictUpdate::cond_where",
        ),
        ("raw_sql", "pgorm::SqlText with pgorm_query::Values"),
        ("raw_sql.inline_sql", "pgorm_query::inject_parameters"),
    ]
    .into_iter()
    .map(|(name, rust_api)| (name.into(), json!({"rust_api": rust_api, "features": []})))
    .collect()
}
