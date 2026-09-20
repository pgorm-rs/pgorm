use pgorm_query::SqlName;

#[derive(SqlName)]
enum User {
    Table,
    #[iden = 123]
    Id,
    FirstName,
    LastName,
    Email,
}

fn main() {}
