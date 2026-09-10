// [spec:pgorm:req:python.transactions]
pub(crate) fn operations() -> serde_json::Map<String, serde_json::Value> {
    [
        ("transaction.begin", "DatabaseConnection::begin_with"),
        (
            "transaction.savepoint",
            "TransactionTrait::begin for DatabaseTransaction",
        ),
        ("transaction.commit", "DatabaseTransaction::commit"),
        ("transaction.rollback", "DatabaseTransaction::rollback"),
        (
            "transaction.close",
            "DatabaseTransaction::rollback / DatabaseConnection::discard",
        ),
        (
            "transaction.execute",
            "ConnectionTrait::execute for DatabaseTransaction",
        ),
        (
            "transaction.fetch_all",
            "ConnectionTrait::query_all for DatabaseTransaction",
        ),
        (
            "transaction.fetch_one",
            "ConnectionTrait::query_all for DatabaseTransaction",
        ),
        (
            "transaction.fetch_optional",
            "ConnectionTrait::query_all for DatabaseTransaction",
        ),
        (
            "transaction.registered",
            "Select / ActiveModel / SelectGraph / SelectedSources over DatabaseTransaction",
        ),
    ]
    .into_iter()
    .map(|(name, api)| {
        (
            name.to_owned(),
            serde_json::json!({
                "rust_api": format!("pgorm::{api}"), "features": ["runtime-tokio"]
            }),
        )
    })
    .collect()
}
