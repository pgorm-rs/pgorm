use pgorm_query::Iden;

#[derive(Iden)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[method]
    Creation,
}

fn main() {}
