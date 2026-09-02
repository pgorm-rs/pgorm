use pgorm::DeriveValueType;

#[derive(DeriveValueType)]
struct MyInt {
    inner: i32,
}

fn main() {}
