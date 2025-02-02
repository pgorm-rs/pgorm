use heck::ToUpperCamelCase;
use pgorm_query::DynIden;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::fmt::Write;

use crate::WithSerde;

#[derive(Clone, Debug)]
pub struct ActiveEnum {
    pub(crate) enum_name: DynIden,
    pub(crate) values: Vec<DynIden>,
}

impl ActiveEnum {
    pub fn impl_active_enum(
        &self,
        with_serde: &WithSerde,
        with_copy_enums: bool,
        extra_derives: &TokenStream,
        extra_attributes: &TokenStream,
    ) -> TokenStream {
        let enum_name = &self.enum_name.to_string();
        let enum_iden = format_ident!("{}", enum_name.to_upper_camel_case());
        let values: Vec<String> = self.values.iter().map(|v| v.to_string()).collect();
        let variants = values.iter().map(|v| v.trim()).map(|v| {
            if v.chars().next().map(char::is_numeric).unwrap_or(false) {
                format_ident!("_{}", v)
            } else {
                let variant_name = v.to_upper_camel_case();
                if variant_name.is_empty() {
                    println!("Warning: item '{}' in the enumeration '{}' cannot be converted into a valid Rust enum member name. It will be converted to its corresponding UTF-8 encoding. You can modify it later as needed.", v, enum_name);
                    let mut ss = String::new();
                    for c in v.chars() {
                        if c.len_utf8() > 1 {
                            write!(&mut ss, "{c}").unwrap();
                        } else {
                            write!(&mut ss, "U{:04X}", c as u32).unwrap();
                        }
                    }
                    format_ident!("{}", ss)
                } else {
                    format_ident!("{}", variant_name)
                }
            }
        });

        let serde_derive = with_serde.extra_derive();
        let copy_derive = if with_copy_enums {
            quote! { , Copy }
        } else {
            quote! {}
        };

        quote! {
            #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum #copy_derive #serde_derive #extra_derives)]
            #[pgorm(rs_type = "String", db_type = "Enum", enum_name = #enum_name)]
            #extra_attributes
            pub enum #enum_iden {
                #(
                    #[pgorm(string_value = #values)]
                    #variants,
                )*
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::writer::{bonus_attributes, bonus_derive};
    use pgorm_query::{Alias, IntoIden};
    use pretty_assertions::assert_eq;

    #[test]
    fn test_enum_variant_starts_with_number() {
        assert_eq!(
            ActiveEnum {
                enum_name: Alias::new("media_type").into_iden(),
                values: vec![
                    "UNKNOWN",
                    "BITMAP",
                    "DRAWING",
                    "AUDIO",
                    "VIDEO",
                    "MULTIMEDIA",
                    "OFFICE",
                    "TEXT",
                    "EXECUTABLE",
                    "ARCHIVE",
                    "3D",
                ]
                .into_iter()
                .map(|variant| Alias::new(variant).into_iden())
                .collect(),
            }
            .impl_active_enum(
                &WithSerde::None,
                true,
                &TokenStream::new(),
                &TokenStream::new(),
            )
            .to_string(),
            quote!(
                #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy)]
                #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "media_type")]
                pub enum MediaType {
                    #[pgorm(string_value = "UNKNOWN")]
                    Unknown,
                    #[pgorm(string_value = "BITMAP")]
                    Bitmap,
                    #[pgorm(string_value = "DRAWING")]
                    Drawing,
                    #[pgorm(string_value = "AUDIO")]
                    Audio,
                    #[pgorm(string_value = "VIDEO")]
                    Video,
                    #[pgorm(string_value = "MULTIMEDIA")]
                    Multimedia,
                    #[pgorm(string_value = "OFFICE")]
                    Office,
                    #[pgorm(string_value = "TEXT")]
                    Text,
                    #[pgorm(string_value = "EXECUTABLE")]
                    Executable,
                    #[pgorm(string_value = "ARCHIVE")]
                    Archive,
                    #[pgorm(string_value = "3D")]
                    _3D,
                }
            )
            .to_string()
        )
    }

    #[test]
    fn test_enum_extra_derives() {
        assert_eq!(
            ActiveEnum {
                enum_name: Alias::new("media_type").into_iden(),
                values: vec!["UNKNOWN", "BITMAP",]
                    .into_iter()
                    .map(|variant| Alias::new(variant).into_iden())
                    .collect(),
            }
            .impl_active_enum(
                &WithSerde::None,
                true,
                &bonus_derive(["specta::Type", "ts_rs::TS"]),
                &TokenStream::new(),
            )
            .to_string(),
            build_generated_enum(),
        );

        #[rustfmt::skip]
        fn build_generated_enum() -> String {
            quote!(
                #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy, specta :: Type, ts_rs :: TS)]
                #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "media_type")]
                pub enum MediaType {
                    #[pgorm(string_value = "UNKNOWN")]
                    Unknown,
                    #[pgorm(string_value = "BITMAP")]
                    Bitmap,
                }
            )
            .to_string()
        }
    }

    #[test]
    fn test_enum_extra_attributes() {
        assert_eq!(
            ActiveEnum {
                enum_name: Alias::new("coinflip_result_type").into_iden(),
                values: vec!["HEADS", "TAILS"]
                    .into_iter()
                    .map(|variant| Alias::new(variant).into_iden())
                    .collect(),
            }
            .impl_active_enum(
                &WithSerde::None,
                true,
                &TokenStream::new(),
                &bonus_attributes([r#"serde(rename_all = "camelCase")"#])
            )
            .to_string(),
            quote!(
                #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy)]
                #[pgorm(
                    rs_type = "String",
                    db_type = "Enum",
                    enum_name = "coinflip_result_type"
                )]
                #[serde(rename_all = "camelCase")]
                pub enum CoinflipResultType {
                    #[pgorm(string_value = "HEADS")]
                    Heads,
                    #[pgorm(string_value = "TAILS")]
                    Tails,
                }
            )
            .to_string()
        );
        assert_eq!(
            ActiveEnum {
                enum_name: Alias::new("coinflip_result_type").into_iden(),
                values: vec!["HEADS", "TAILS"]
                    .into_iter()
                    .map(|variant| Alias::new(variant).into_iden())
                    .collect(),
            }
            .impl_active_enum(
                &WithSerde::None,
                true,
                &TokenStream::new(),
                &bonus_attributes([r#"serde(rename_all = "camelCase")"#, "ts(export)"])
            )
            .to_string(),
            quote!(
                #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy)]
                #[pgorm(
                    rs_type = "String",
                    db_type = "Enum",
                    enum_name = "coinflip_result_type"
                )]
                #[serde(rename_all = "camelCase")]
                #[ts(export)]
                pub enum CoinflipResultType {
                    #[pgorm(string_value = "HEADS")]
                    Heads,
                    #[pgorm(string_value = "TAILS")]
                    Tails,
                }
            )
            .to_string()
        )
    }

    #[test]
    fn test_enum_variant_utf8_encode() {
        assert_eq!(
            ActiveEnum {
                enum_name: Alias::new("ty").into_iden(),
                values: vec![
                    "Question",
                    "QuestionsAdditional",
                    "Answer",
                    "Other",
                    "/",
                    "//",
                    "A-B-C",
                    "你好",
                ]
                .into_iter()
                .map(|variant| Alias::new(variant).into_iden())
                .collect(),
            }
            .impl_active_enum(
                &WithSerde::None,
                true,
                &TokenStream::new(),
                &TokenStream::new(),
            )
            .to_string(),
            quote!(
                #[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum, Copy)]
                #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "ty")]
                pub enum Ty {
                    #[pgorm(string_value = "Question")]
                    Question,
                    #[pgorm(string_value = "QuestionsAdditional")]
                    QuestionsAdditional,
                    #[pgorm(string_value = "Answer")]
                    Answer,
                    #[pgorm(string_value = "Other")]
                    Other,
                    #[pgorm(string_value = "/")]
                    U002F,
                    #[pgorm(string_value = "//")]
                    U002FU002F,
                    #[pgorm(string_value = "A-B-C")]
                    ABC,
                    #[pgorm(string_value = "你好")]
                    你好,
                }
            )
            .to_string()
        )
    }
}
