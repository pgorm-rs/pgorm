use pgorm::DeriveValueType;

#[derive(DeriveValueType)]
#[pgorm(no_such_key = "not read by this derive")]
struct MyInt(i32);

fn main() {}
