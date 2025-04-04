#[test]
fn when_user_import_nothing_macro_still_works_test() {
    #[derive(pgorm::DeriveValueType)]
    struct MyString(String);
}

#[test]
fn when_user_alias_result_macro_still_works_test() {
    #[allow(dead_code)]
    type Result<T> = std::result::Result<T, ()>;
    #[derive(pgorm::DeriveValueType)]
    struct MyString(String);
}
