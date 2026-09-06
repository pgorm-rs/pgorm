//! Copied from <https://github.com/Peternator7/strum/blob/master/strum_macros/src/helpers/case_style.rs>
use heck::{
    ToKebabCase, ToLowerCamelCase, ToShoutySnakeCase, ToSnakeCase, ToTitleCase, ToUpperCamelCase,
};
use std::str::FromStr;
use syn::{
    Ident, LitStr,
    meta::ParseNestedMeta,
    parse::{Parse, ParseStream},
};

#[allow(clippy::enum_variant_names)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaseStyle {
    CamelCase,
    KebabCase,
    MixedCase,
    ShoutySnakeCase,
    SnakeCase,
    TitleCase,
    UpperCase,
    LowerCase,
    ScreamingKebabCase,
    PascalCase,
}

const VALID_CASE_STYLES: &[&str] = &[
    "camelCase",
    "PascalCase",
    "kebab-case",
    "snake_case",
    "SCREAMING_SNAKE_CASE",
    "SCREAMING-KEBAB-CASE",
    "lowercase",
    "UPPERCASE",
    "title_case",
    "mixed_case",
];

impl Parse for CaseStyle {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let text = input.parse::<LitStr>()?;
        let val = text.value();

        val.as_str().parse().map_err(|_| {
            syn::Error::new_spanned(
                &text,
                format!(
                    "Unexpected case style for serialize_all: `{}`. Valid values are: `{:?}`",
                    val, VALID_CASE_STYLES
                ),
            )
        })
    }
}

impl FromStr for CaseStyle {
    type Err = ();

    fn from_str(text: &str) -> Result<Self, ()> {
        Ok(match text {
            "camel_case" | "PascalCase" => CaseStyle::PascalCase,
            "camelCase" => CaseStyle::CamelCase,
            "snake_case" | "snek_case" => CaseStyle::SnakeCase,
            "kebab_case" | "kebab-case" => CaseStyle::KebabCase,
            "SCREAMING-KEBAB-CASE" => CaseStyle::ScreamingKebabCase,
            "shouty_snake_case" | "shouty_snek_case" | "SCREAMING_SNAKE_CASE" => {
                CaseStyle::ShoutySnakeCase
            }
            "title_case" => CaseStyle::TitleCase,
            "mixed_case" => CaseStyle::MixedCase,
            "lowercase" => CaseStyle::LowerCase,
            "UPPERCASE" => CaseStyle::UpperCase,
            _ => return Err(()),
        })
    }
}

pub trait CaseStyleHelpers {
    fn convert_case(&self, case_style: Option<CaseStyle>) -> String;
}

impl CaseStyleHelpers for Ident {
    fn convert_case(&self, case_style: Option<CaseStyle>) -> String {
        convert_case_str(&self.to_string(), case_style)
    }
}

/// The same conversion for a name already in hand as a string, which a field's
/// identifier is once its raw-identifier prefix has been trimmed off.
pub fn convert_case_str(text: &str, case_style: Option<CaseStyle>) -> String {
    let Some(case_style) = case_style else {
        return text.to_owned();
    };
    match case_style {
        CaseStyle::PascalCase => text.to_upper_camel_case(),
        CaseStyle::KebabCase => text.to_kebab_case(),
        CaseStyle::MixedCase => text.to_lower_camel_case(),
        CaseStyle::ShoutySnakeCase => text.to_shouty_snake_case(),
        CaseStyle::SnakeCase => text.to_snake_case(),
        CaseStyle::TitleCase => text.to_title_case(),
        CaseStyle::UpperCase => text.to_uppercase(),
        CaseStyle::LowerCase => text.to_lowercase(),
        CaseStyle::ScreamingKebabCase => text.to_kebab_case().to_uppercase(),
        CaseStyle::CamelCase => {
            let camel_case = text.to_upper_camel_case();
            let mut pascal = String::with_capacity(camel_case.len());
            let mut it = camel_case.chars();
            if let Some(ch) = it.next() {
                pascal.extend(ch.to_lowercase());
            }
            pascal.extend(it);
            pascal
        }
    }
}

impl<'meta> TryFrom<&ParseNestedMeta<'meta>> for CaseStyle {
    type Error = syn::Error;

    fn try_from(value: &ParseNestedMeta) -> Result<Self, Self::Error> {
        let meta_string_literal: LitStr = value.value()?.parse()?;
        let value_string = meta_string_literal.value();
        match CaseStyle::from_str(value_string.as_str()) {
            Ok(rule) => Ok(rule),
            Err(()) => Err(value.error(format!(
                "Unknown value for attribute parameter: `{value_string}`. Valid values are: `{VALID_CASE_STYLES:?}`"
            ))),
        }
    }
}

#[test]
fn test_convert_case() {
    let id = Ident::new("test_me", proc_macro2::Span::call_site());
    assert_eq!("testMe", id.convert_case(Some(CaseStyle::CamelCase)));
    assert_eq!("TestMe", id.convert_case(Some(CaseStyle::PascalCase)));
}
