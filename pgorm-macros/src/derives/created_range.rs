use proc_macro2::TokenStream;
use quote::quote;
use syn::{GenericArgument, LitStr, PathArguments, Type, spanned::Spanned};

/// The derive's reading of its input: the newtype, the range it holds, the
/// subtype that range is over, and the type's name.
struct CreatedRange {
    ident: syn::Ident,
    field_type: Type,
    subtype: Type,
    range_name: LitStr,
    schema_name: Option<LitStr>,
}

// [spec:pgorm:sem:macros.derive.created-range]
pub fn expand_derive_created_range(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    let parsed = parse(input)?;
    Ok(expand(&parsed))
}

fn parse(input: syn::DeriveInput) -> syn::Result<CreatedRange> {
    let syn::DeriveInput {
        ident, data, attrs, ..
    } = input;
    let held = match data {
        syn::Data::Struct(syn::DataStruct {
            fields: syn::Fields::Unnamed(fields),
            ..
        }) if fields.unnamed.len() == 1 => fields.unnamed.into_iter().next(),
        _ => None,
    };
    let Some(field) = held else {
        return Err(syn::Error::new(
            ident.span(),
            "DeriveCreatedRange is derived on a tuple struct holding one `Range<T>`",
        ));
    };
    let Some(subtype) = range_subtype(&field.ty) else {
        return Err(syn::Error::new(
            field.ty.span(),
            "DeriveCreatedRange's field is a `Range<T>`, `T` the subtype the range type ranges over",
        ));
    };

    let mut range_name = None;
    let mut schema_name = None;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("pgorm")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("range_name") {
                range_name = Some(meta.value()?.parse::<LitStr>()?);
            } else if meta.path.is_ident("schema_name") {
                schema_name = Some(meta.value()?.parse::<LitStr>()?);
            } else {
                return Err(meta.error("expected `range_name` or `schema_name`"));
            }
            Ok(())
        })?;
    }
    let Some(range_name) = range_name else {
        return Err(syn::Error::new(
            ident.span(),
            "DeriveCreatedRange needs `#[pgorm(range_name = \"...\")]`, the range type's name",
        ));
    };

    Ok(CreatedRange {
        ident,
        subtype: subtype.clone(),
        field_type: field.ty,
        range_name,
        schema_name,
    })
}

/// The `T` of a field written `Range<T>` — the last path segment `Range`,
/// however qualified, with one type argument.
fn range_subtype(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let segment = path.path.segments.last()?;
    let PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    match arguments.args.first() {
        Some(GenericArgument::Type(subtype))
            if segment.ident == "Range" && arguments.args.len() == 1 =>
        {
            Some(subtype)
        }
        _ => None,
    }
}

fn expand(parsed: &CreatedRange) -> TokenStream {
    let CreatedRange {
        ident,
        field_type,
        subtype,
        range_name,
        schema_name,
    } = parsed;
    let (schema, qualified) = match schema_name {
        Some(schema) => (
            quote! { Some(pgorm::pgorm_query::Name::runtime(#schema)) },
            quote! { .schema(pgorm::pgorm_query::Name::runtime(#schema)) },
        ),
        None => (quote! { None }, quote! {}),
    };

    quote! {
        #[automatically_derived]
        impl pgorm::CreatedRange for #ident {
            fn name() -> pgorm::pgorm_query::TypeName {
                pgorm::pgorm_query::TypeName::new(pgorm::pgorm_query::Name::runtime(#range_name))
                    #qualified
            }
        }

        #[automatically_derived]
        impl std::convert::From<#field_type> for #ident {
            fn from(range: #field_type) -> Self {
                Self(range)
            }
        }

        #[automatically_derived]
        impl std::convert::From<#ident> for pgorm::Value {
            fn from(source: #ident) -> Self {
                pgorm::Value::String(Some(Box::new(source.0.to_string())))
            }
        }

        #[automatically_derived]
        impl pgorm::pgorm_query::Nullable for #ident {
            fn null() -> pgorm::Value {
                pgorm::Value::String(None)
            }
        }

        #[automatically_derived]
        impl pgorm::TryGetable for #ident {
            fn try_get_by<I: pgorm::RowIndex + std::fmt::Display>(
                res: &pgorm::QueryResult,
                idx: I,
            ) -> std::result::Result<Self, pgorm::TryGetError> {
                <#field_type as pgorm::TryGetable>::try_get_by(res, idx).map(Self)
            }

            fn accepts(ty: &pgorm::types::Type) -> bool {
                <#field_type as pgorm::TryGetable>::accepts(ty)
            }
        }

        #[automatically_derived]
        impl pgorm::pgorm_query::ValueType for #ident {
            fn try_from(
                value: pgorm::Value,
            ) -> std::result::Result<Self, pgorm::pgorm_query::ValueTypeError> {
                match value {
                    pgorm::Value::String(Some(text)) => text.parse().map(Self),
                    _ => Err(pgorm::pgorm_query::ValueTypeError),
                }
            }

            fn type_name() -> std::string::String {
                stringify!(#ident).to_owned()
            }

            fn array_type() -> pgorm::pgorm_query::ArrayType {
                pgorm::pgorm_query::ArrayType::String
            }

            fn column_type() -> pgorm::pgorm_query::ColumnType {
                pgorm::pgorm_query::ColumnType::CreatedRange {
                    name: pgorm::pgorm_query::Name::runtime(#range_name),
                    schema: #schema,
                    subtype: std::sync::Arc::new(
                        <#subtype as pgorm::pgorm_query::ValueType>::column_type(),
                    ),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal(input: &str) -> String {
        let input = syn::parse_str::<syn::DeriveInput>(input).expect("test input parses");
        match expand_derive_created_range(input) {
            Ok(_) => panic!("expected a refusal"),
            Err(error) => error.to_string(),
        }
    }

    // [spec:pgorm:sem:macros.derive.created-range/test]    a shape other than a
    // newtype over `Range<T>`, a missing name and an unknown key are compile errors
    #[test]
    fn what_the_derive_cannot_read_is_refused() {
        assert!(refusal("struct R { range: Range<f64> }").contains("tuple struct holding one"));
        assert!(refusal("struct R(Range<f64>, i32);").contains("tuple struct holding one"));
        assert!(refusal("#[pgorm(range_name = \"r\")] struct R(f64);").contains("is a `Range<T>`"));
        assert!(
            refusal("#[pgorm(range_name = \"r\")] struct R(Range<f64, f64>);")
                .contains("is a `Range<T>`")
        );
        assert!(refusal("struct R(Range<f64>);").contains("needs `#[pgorm(range_name"));
        assert!(
            refusal("#[pgorm(range_name = \"r\", enum_name = \"e\")] struct R(Range<f64>);")
                .contains("expected `range_name` or `schema_name`")
        );
    }

    // [spec:pgorm:sem:macros.derive.created-range/test]    a qualified spelling of
    // `Range` is read, and the name and schema are names, never tokens
    #[test]
    fn the_name_and_schema_are_names() {
        let input = syn::parse_str::<syn::DeriveInput>(
            "#[pgorm(range_name = \"a \\\" b\", schema_name = \"s\")] \
             struct R(pgorm::pgorm_query::Range<i32>);",
        )
        .expect("test input parses");
        let expanded = expand_derive_created_range(input)
            .expect("the input is read")
            .to_string();
        assert!(
            expanded.contains("Name :: runtime (\"a \\\" b\")"),
            "{expanded}"
        );
        assert!(expanded.contains(". schema (pgorm :: pgorm_query :: Name :: runtime (\"s\"))"));
        assert!(
            expanded.contains("< i32 as pgorm :: pgorm_query :: ValueType > :: column_type ()")
        );
    }
}
