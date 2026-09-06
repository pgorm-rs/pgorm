use pgorm::FromQueryResult;

#[derive(FromQueryResult)]
struct Row {
    #[pgorm(skp)]
    id: i32,
}

fn main() {}
