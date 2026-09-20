use pgorm_query::SqlName;

#[derive(SqlName)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[method]
    Creation,
}

fn main() {}
