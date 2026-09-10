// [spec:pgorm:req:python.schema]
pub(crate) fn operations() -> serde_json::Map<String, serde_json::Value> {
    [
        ("schema.data_type", "pgorm_query::ColumnType"),
        ("schema.column", "pgorm_query::ColumnDef"),
        ("schema.create_table", "pgorm_query::TableCreateStatement"),
        ("schema.drop_table", "pgorm_query::TableDropStatement"),
        ("schema.rename_table", "pgorm_query::TableRenameStatement"),
        ("schema.rename_column", "pgorm_query::ColumnRenameStatement"),
        ("schema.truncate", "pgorm_query::TableTruncateStatement"),
        ("schema.add_column", "pgorm_query::PendingTableAlter::add_column"),
        ("schema.modify_column", "pgorm_query::PendingTableAlter::modify_column"),
        ("schema.drop_column", "pgorm_query::PendingTableAlter::drop_column"),
        ("schema.create_index", "pgorm_query::IndexCreateStatement"),
        ("schema.drop_index", "pgorm_query::IndexDropStatement"),
        ("schema.create_enum", "pgorm_query::extension::TypeCreateStatement"),
        ("schema.add_enum_value", "pgorm_query::extension::PendingTypeAlter::add_value"),
        ("schema.rename_enum_value", "pgorm_query::extension::PendingTypeAlter::rename_value"),
        ("schema.rename_enum", "pgorm_query::extension::PendingTypeAlter::rename_to"),
        ("schema.drop_enum", "pgorm_query::extension::TypeDropStatement"),
        ("schema.from_entity", "Schema::{create_table_from_entity,create_enum_from_entity,create_index_from_entity,create_comments_from_entity}"),
    ].into_iter().map(|(name, api)| (name.to_owned(), serde_json::json!({
        "rust_api": format!("pgorm::{api}"), "features": [],
        "registration_required": name == "schema.from_entity"
    }))).collect()
}
