// [spec:pgorm:req:python.transactions+1]
pub(crate) fn operations() -> serde_json::Map<String, serde_json::Value> {
    [
        ("transaction.begin", "DatabaseConnection::transaction_with"),
        (
            "transaction.savepoint",
            "TransactionTrait::transaction for DatabaseTransaction",
        ),
        (
            "transaction.commit",
            "TransactionTrait::transaction, the closure returning Ok",
        ),
        (
            "transaction.rollback",
            "TransactionTrait::transaction, the closure returning Err",
        ),
        (
            "transaction.close",
            "TransactionTrait::transaction, the closure returning Err / DatabaseConnection::discard",
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
