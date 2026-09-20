use pgorm_query::SqlName;
use strum::{EnumIter, IntoEnumIterator};

#[derive(Copy, Clone, SqlName, EnumIter)]
enum User {
    Table,
    Id,
    FirstName,
    LastName,
    Email,
}

fn main() {
    let expected = ["user", "id", "first_name", "last_name", "email"];
    User::iter().zip(expected).for_each(|(var, exp)| {
        assert_eq!(var.to_string(), exp);
    });
}
