use pgorm_query::SqlName;

#[derive(SqlName)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[iden()]
    Creation(CreationInfo),
}

#[derive(SqlName)]
enum CreationInfo {
    UserId,
    #[iden = "creation_date"]
    Date,
}

fn main() {}
