#![allow(unused_imports, dead_code)]

pub mod common;
pub use common::{TestContext, features::*, setup::*};
use pgorm::entity::prelude::*;

#[derive(DeriveIden)]
pub enum ClassName {
    Table,
    Id,
    Title,
    Text,
}

#[derive(DeriveIden)]
pub enum Book {
    #[pgorm(iden = "book_table")]
    Table,
    Id,
    #[pgorm(iden = "turtle")]
    Title,
    #[pgorm(iden = "TeXt")]
    Text,
    #[pgorm(iden = "ty_pe")]
    Type,
}

#[derive(DeriveIden)]
struct GlyphToken;

#[derive(DeriveIden)]
#[pgorm(iden = "weRd")]
struct Word;

// Every rendered name here is a "valid iden", so the `prepare` override is
// emitted and quotes the name itself.
#[derive(DeriveIden)]
pub enum AllValid {
    Table,
    Id,
}

// One variant renders a name that is *not* a valid iden, which suppresses the
// `prepare` override for the whole enum and leaves the trait default (which
// doubles any embedded right-quote) to do the quoting.
#[derive(DeriveIden)]
pub enum SomeInvalid {
    Table,
    #[pgorm(iden = "we`ird")]
    Weird,
}

// An empty enum expands to nothing, which is the only reason this hand-written
// `Iden` impl compiles rather than colliding with a generated one.
#[derive(DeriveIden)]
pub enum EmptyIden {}

impl Iden for EmptyIden {
    fn unquoted(&self, _s: &mut dyn std::fmt::Write) {
        match *self {}
    }
}

// [spec:pgorm:sem:macros.derive.iden/test]    unit structs, enums, `Table`, and the `iden` override
#[test]
fn main() -> Result<(), DbErr> {
    assert_eq!(ClassName::Table.to_string(), "class_name");
    assert_eq!(ClassName::Id.to_string(), "id");
    assert_eq!(ClassName::Title.to_string(), "title");
    assert_eq!(ClassName::Text.to_string(), "text");

    assert_eq!(Book::Id.to_string(), "id");
    assert_eq!(Book::Table.to_string(), "book_table");
    assert_eq!(Book::Title.to_string(), "turtle");
    assert_eq!(Book::Text.to_string(), "TeXt");
    assert_eq!(Book::Type.to_string(), "ty_pe");

    assert_eq!(GlyphToken.to_string(), "glyph_token");

    assert_eq!(Word.to_string(), "weRd");
    Ok(())
}

// [spec:pgorm:sem:macros.derive.iden/test]    `prepare` is emitted only for statically-valid idens
#[test]
fn prepare_override_is_conditional() {
    // Emitted: the name is written between the quote pair verbatim.
    let mut s = String::new();
    AllValid::Table.prepare(&mut s, '"'.into());
    assert_eq!(s, "\"all_valid\"");

    // Not emitted: the trait default is used, which doubles an embedded
    // right-quote. `AllValid` never shows that behaviour, `SomeInvalid` does.
    let mut s = String::new();
    SomeInvalid::Weird.prepare(&mut s, b'`'.into());
    assert_eq!(s, "`we``ird`");

    // The suppression is per-enum, not per-variant.
    let mut s = String::new();
    SomeInvalid::Table.prepare(&mut s, b'`'.into());
    assert_eq!(s, "`some_invalid`");

    let mut s = String::new();
    AllValid::Id.prepare(&mut s, b'`'.into());
    assert_eq!(s, "`id`");
}

// [spec:pgorm:sem:macros.derive.iden/test]    an empty enum expands to nothing
#[test]
fn empty_enum_expands_to_nothing() {
    fn assert_iden<T: Iden>() {}
    assert_iden::<EmptyIden>();
}
