use pgorm::DeriveValueType;

// `&str` is in the inference table but `&'static str` is not: it falls through
// to the `ValueType` path, which reports the type as written.
#[derive(DeriveValueType)]
struct MyStr(&'static str);

fn main() {}
