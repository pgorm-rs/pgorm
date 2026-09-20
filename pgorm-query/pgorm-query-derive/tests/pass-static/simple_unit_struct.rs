use pgorm_query::{SqlName, StaticName};

#[derive(Copy, Clone, Debug, StaticName)]
pub struct SomeType;

#[derive(Copy, Clone, Debug, StaticName)]
#[iden(rename = "Hel\"lo")]
pub struct SomeTypeWithRename;

fn main() {
    assert_eq!(SomeType.to_string(), "some_type");
    assert_eq!(SomeTypeWithRename.to_string(), "Hel\"lo");

    let mut string = String::new();
    SomeType.prepare(&mut string);
    assert_eq!(string, "\"some_type\"");

    // The name is not a valid iden, so the derive emits no `prepare` override
    // and the trait default quotes it — doubling the embedded quote.
    let mut string = String::new();
    SomeTypeWithRename.prepare(&mut string);
    assert_eq!(string, "\"Hel\"\"lo\"");
}
