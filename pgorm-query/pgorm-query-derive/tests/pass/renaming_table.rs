use pgorm_query::SqlName;
use strum::{EnumIter, IntoEnumIterator};

#[derive(Copy, Clone, SqlName, EnumIter)]
enum Something {
    // ...the Table can also be overwritten like this
    #[iden = "something_else"]
    Table,
    Id,
    AssetName,
    UserId,
}

fn main() {
    let expected = ["something_else", "id", "asset_name", "user_id"];
    Something::iter().zip(expected).for_each(|(var, exp)| {
        assert_eq!(var.to_string(), exp);
    })
}
