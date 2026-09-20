use pgorm_query::SqlName;

#[derive(SqlName)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[iden]
    Creation,
}

fn main() {}
