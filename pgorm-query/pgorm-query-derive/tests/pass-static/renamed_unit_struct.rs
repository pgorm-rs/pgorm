use pgorm_query::{SqlName, StaticName};

#[derive(Copy, Clone, Debug, StaticName)]
#[iden = "another_name"]
pub struct CustomName;

fn main() {
    assert_eq!(CustomName.to_string(), "another_name");
    assert_eq!(CustomName.as_str(), "another_name")
}
