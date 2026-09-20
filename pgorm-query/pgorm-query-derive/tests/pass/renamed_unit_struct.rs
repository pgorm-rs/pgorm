use pgorm_query::SqlName;

#[derive(Copy, Clone, SqlName)]
#[iden = "another_name"]
pub struct CustomName;

fn main() {
    assert_eq!(CustomName.to_string(), "another_name");
}
