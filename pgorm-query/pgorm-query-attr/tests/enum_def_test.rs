//! Verification for the `#[enum_def]` attribute macro.

#![allow(dead_code)]

use pgorm_query::Iden;
use pgorm_query_attr::enum_def;
use std::collections::HashSet;

/// Defaults: empty prefix, `Iden` suffix, table name = snake_case struct name.
#[enum_def]
pub struct Hello {
    pub name: String,
    pub other_field: i32,
}

#[enum_def(prefix = "Enum", suffix = "")]
pub struct Prefixed {
    pub name: String,
}

#[enum_def(suffix = "Def")]
pub struct Suffixed {
    pub name: String,
}

/// `table_name` overrides what the `Table` variant renders. It is re-parsed as
/// an identifier, so it has to be a legal one.
#[enum_def(table_name = "hello_table")]
pub struct Renamed {
    pub name: String,
}

/// `crate_name` rewrites the path of the `Iden` trait in the generated impl.
#[enum_def(crate_name = "pgorm_query")]
pub struct Crated {
    pub name: String,
}

// [spec:pgorm:sem:macros.derive.enum-def/test]    the input is re-emitted, plus a `{Struct}Iden` enum
#[test]
fn the_struct_survives_and_gains_an_iden_enum() {
    // The annotated struct is re-emitted unchanged.
    let hello = Hello {
        name: "world".to_owned(),
        other_field: 7,
    };
    assert_eq!(hello.name, "world");
    assert_eq!(hello.other_field, 7);

    // One `Table` variant plus one PascalCase variant per field.
    let variants = [HelloIden::Table, HelloIden::Name, HelloIden::OtherField];
    assert_eq!(variants.len(), 3);
}

// [spec:pgorm:sem:macros.derive.enum-def/test]    what `Iden::unquoted` writes
#[test]
fn iden_renders_table_name_and_field_identifiers() {
    // `Table` defaults to the snake_case of the struct name...
    assert_eq!(HelloIden::Table.to_string(), "hello");
    // ...and each field variant renders the *original* field identifier, not
    // the PascalCase variant name.
    assert_eq!(HelloIden::Name.to_string(), "name");
    assert_eq!(HelloIden::OtherField.to_string(), "other_field");

    // `table_name` overrides the `Table` rendering only.
    assert_eq!(RenamedIden::Table.to_string(), "hello_table");
    assert_eq!(RenamedIden::Name.to_string(), "name");

    // `crate_name` only moves the trait path; behaviour is unchanged.
    assert_eq!(CratedIden::Table.to_string(), "crated");
}

// [spec:pgorm:sem:macros.derive.enum-def/test]    prefix / suffix control the generated enum's name
#[test]
fn prefix_and_suffix_name_the_generated_enum() {
    assert_eq!(EnumPrefixed::Table.to_string(), "prefixed");
    assert_eq!(SuffixedDef::Table.to_string(), "suffixed");
}

// [spec:pgorm:sem:macros.derive.enum-def/test]    the generated enum's derives
#[test]
fn the_generated_enum_derives_the_documented_set() {
    // Debug
    assert_eq!(format!("{:?}", HelloIden::Name), "Name");
    // Copy + Clone
    let a = HelloIden::Name;
    let b = a;
    #[allow(clippy::clone_on_copy)]
    let c = a.clone();
    // PartialEq + Eq
    assert_eq!(a, b);
    assert_eq!(a, c);
    assert_ne!(a, HelloIden::Table);
    // Hash
    let set: HashSet<HelloIden> = [HelloIden::Table, HelloIden::Name, HelloIden::Name]
        .into_iter()
        .collect();
    assert_eq!(set.len(), 2);
}
