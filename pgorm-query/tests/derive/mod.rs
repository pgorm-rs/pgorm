use pgorm_query::*;

// [spec:pgorm:sem:macros.derive.iden.query/test]    default snake_case names; `Table` renders the container
#[test]
fn derive_1() {
    #[derive(Debug, Iden)]
    enum User {
        Table,
        Id,
        FirstName,
        LastName,
        Email,
    }

    println!("Default field names");
    assert_eq!(Iden::to_string(&User::Table), "user");
    assert_eq!(Iden::to_string(&User::Id), "id");
    assert_eq!(Iden::to_string(&User::FirstName), "first_name");
    assert_eq!(Iden::to_string(&User::LastName), "last_name");
    assert_eq!(Iden::to_string(&User::Email), "email");
}

// [spec:pgorm:sem:macros.derive.iden.query/test]    container and variant `#[iden = "..."]` renaming
#[test]
fn derive_2() {
    #[derive(Debug, Iden)]
    // Outer iden attributes overrides what's used for "Table"...
    #[iden = "user"]
    enum Custom {
        Table,
        #[iden = "my_id"]
        Id,
        #[iden = "name"]
        FirstName,
        #[iden = "surname"]
        LastName,
        // Custom casing if needed
        #[iden = "EMail"]
        Email,
    }

    println!("Custom field names");
    assert_eq!(Iden::to_string(&Custom::Table), "user");
    assert_eq!(Iden::to_string(&Custom::Id), "my_id");
    assert_eq!(Iden::to_string(&Custom::FirstName), "name");
    assert_eq!(Iden::to_string(&Custom::LastName), "surname");
    assert_eq!(Iden::to_string(&Custom::Email), "EMail");
}

// [spec:pgorm:sem:macros.derive.iden.query/test]    renaming only the `Table` variant
#[test]
fn derive_3() {
    #[derive(Debug, Iden)]
    enum Something {
        // ...the Table can also be overwritten like this
        #[iden = "something_else"]
        Table,
        Id,
        AssetName,
        UserId,
    }

    println!("Single custom field name");
    assert_eq!(Iden::to_string(&Something::Table), "something_else");
    assert_eq!(Iden::to_string(&Something::Id), "id");
    assert_eq!(Iden::to_string(&Something::AssetName), "asset_name");
    assert_eq!(Iden::to_string(&Something::UserId), "user_id");
}

// [spec:pgorm:sem:macros.derive.iden.query/test]    unit structs, renamed and not
#[test]
fn derive_4() {
    #[derive(Debug, Iden)]
    pub struct SomeType;

    #[derive(Debug, Iden)]
    #[iden = "another_name"]
    pub struct CustomName;

    println!("Unit structs");
    assert_eq!(Iden::to_string(&SomeType), "some_type");
    assert_eq!(Iden::to_string(&CustomName), "another_name");
}

// [spec:pgorm:sem:macros.derive.iden.query/test]    IdenStatic adds as_str and AsRef<str>
#[test]
fn derive_5_iden_static() {
    #[derive(Copy, Clone, Debug, IdenStatic)]
    enum User {
        Table,
        Id,
        FirstName,
        #[iden = "e_mail"]
        Email,
    }

    // `IdenStatic` generates the `Iden` impl as well...
    assert_eq!(Iden::to_string(&User::Table), "user");
    assert_eq!(Iden::to_string(&User::FirstName), "first_name");

    // ...plus `as_str` returning a `&'static str` from the same naming rules...
    assert_eq!(User::Table.as_str(), "user");
    assert_eq!(User::Id.as_str(), "id");
    assert_eq!(User::FirstName.as_str(), "first_name");
    assert_eq!(User::Email.as_str(), "e_mail");

    // ...and `AsRef<str>`.
    fn take_str<S: AsRef<str>>(s: S) -> String {
        s.as_ref().to_owned()
    }
    assert_eq!(take_str(User::FirstName), "first_name");
}

// [spec:pgorm:sem:macros.derive.iden.query/test]    an empty enum expands to nothing
#[test]
fn derive_6_empty_enum_expands_to_nothing() {
    #[derive(Copy, Clone, Debug, Iden)]
    enum Empty {}

    // The derive produced no `Iden` impl at all, which is the only reason this
    // hand-written one compiles instead of colliding with a generated one.
    impl Iden for Empty {
        fn unquoted(&self, _s: &mut dyn std::fmt::Write) {
            match *self {}
        }
    }

    fn assert_iden<T: Iden>() {}
    assert_iden::<Empty>();
}
