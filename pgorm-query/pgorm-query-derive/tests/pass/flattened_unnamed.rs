use pgorm_query::Iden;
use strum::{EnumIter, IntoEnumIterator};

#[derive(Copy, Clone, Iden, EnumIter)]
enum Asset {
    Table,
    Id,
    AssetName,
    #[iden(flatten)]
    Creation(CreationInfo),
}

#[derive(Copy, Clone, Iden)]
enum CreationInfo {
    UserId,
    #[iden = "creation_date"]
    Date,
}

impl Default for CreationInfo {
    fn default() -> Self {
        Self::UserId
    }
}

fn main() {
    // custom ends up being default string which is an empty string
    let expected = ["asset", "id", "asset_name", "user_id", "creation_date"];
    Asset::iter()
        .chain(std::iter::once(Asset::Creation(CreationInfo::Date)))
        .zip(expected)
        .for_each(|(var, exp)| {
            assert_eq!(var.to_string(), exp);
        })
}
