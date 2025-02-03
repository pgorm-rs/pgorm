use pgorm_query::Iden;

#[derive(Iden)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[iden]
    Creation,
}

fn main() {}
