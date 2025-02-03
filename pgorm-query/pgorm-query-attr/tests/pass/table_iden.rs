use pgorm_query::Iden;
use pgorm_query_attr::enum_def;

#[enum_def(table_name = "HelloTable")]
pub struct Hello {
    pub name: String,
}

fn main() {
    assert_eq!("HelloTable".to_string(), HelloIden::Table.to_string());
}
