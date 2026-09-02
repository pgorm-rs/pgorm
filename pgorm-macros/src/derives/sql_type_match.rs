use proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{GenericArgument, PathArguments, PathSegment, Type, TypePath};

/// The `T` of a field written as a bare, single-segment `Option<T>`.
///
/// A qualified spelling (`std::option::Option<T>`) is not an `Option` for the
/// purposes of the inference tables.
pub fn unwrap_option(ty: &Type) -> Option<&Type> {
    let segment = sole_segment(ty)?;
    if segment.ident != "Option" {
        return None;
    }
    sole_type_argument(&segment.arguments)
}

fn sole_segment(ty: &Type) -> Option<&PathSegment> {
    let Type::Path(TypePath { qself: None, path }) = ty else {
        return None;
    };
    if path.leading_colon.is_some() || path.segments.len() != 1 {
        return None;
    }
    path.segments.first()
}

fn sole_type_argument(arguments: &PathArguments) -> Option<&Type> {
    let PathArguments::AngleBracketed(arguments) = arguments else {
        return None;
    };
    match arguments.args.first() {
        Some(GenericArgument::Type(ty)) if arguments.args.len() == 1 => Some(ty),
        _ => None,
    }
}

/// The name of a bare, single-segment path carrying no generic arguments:
/// `i32`, `String`, `Uuid`.
fn bare_name(ty: &Type) -> Option<&Ident> {
    let segment = sole_segment(ty)?;
    matches!(segment.arguments, PathArguments::None).then_some(&segment.ident)
}

/// `&str`, written without a lifetime and without `mut`.
fn is_shared_str(ty: &Type) -> bool {
    let Type::Reference(reference) = ty else {
        return false;
    };
    reference.lifetime.is_none()
        && reference.mutability.is_none()
        && bare_name(&reference.elem).is_some_and(|name| name == "str")
}

/// `Vec<u8>`.
fn is_byte_vec(ty: &Type) -> bool {
    let Some(segment) = sole_segment(ty) else {
        return false;
    };
    segment.ident == "Vec"
        && sole_type_argument(&segment.arguments)
            .and_then(bare_name)
            .is_some_and(|name| name == "u8")
}

// [spec:pgorm:sem:macros.derive.entity-model.column-def+3]
pub fn col_type_match(
    col_type: Option<TokenStream>,
    field_type: &Type,
    field_span: Span,
) -> TokenStream {
    if let Some(col_type) = col_type {
        return quote! { pgorm::prelude::ColumnType::#col_type };
    }
    match inferred_col_type(field_type) {
        Some(col_type) => quote! { pgorm::prelude::ColumnType::#col_type },
        // Assumed to be an ActiveEnum if none of the above types matched.
        None => quote_spanned! { field_span =>
            std::convert::Into::<pgorm::pgorm_query::ColumnType>::into(
                <#field_type as pgorm::pgorm_query::ValueType>::column_type()
            )
        },
    }
}

fn inferred_col_type(field_type: &Type) -> Option<TokenStream> {
    if is_shared_str(field_type) {
        return Some(quote! { string(None) });
    }
    if is_byte_vec(field_type) {
        return Some(quote! { Bytea });
    }
    Some(match bare_name(field_type)?.to_string().as_str() {
        "char" => quote! { Char(None) },
        "String" => quote! { string(None) },
        "i8" => quote! { SmallInteger },
        "i16" => quote! { SmallInteger },
        "i32" => quote! { Integer },
        "u32" => quote! { BigInteger },
        "i64" => quote! { BigInteger },
        "u64" => quote! { BigInteger },
        "f32" => quote! { Float },
        "f64" => quote! { Double },
        "bool" => quote! { Boolean },
        "Date" | "NaiveDate" => quote! { Date },
        "Time" | "NaiveTime" => quote! { Time },
        "DateTime" | "NaiveDateTime" => quote! { Timestamp },
        "DateTimeUtc" | "DateTimeLocal" | "DateTimeWithTimeZone" => {
            quote! { TimestampWithTimeZone }
        }
        "Uuid" => quote! { Uuid },
        "Json" => quote! { Json },
        "Decimal" => quote! { Decimal(None) },
        _ => return None,
    })
}

// [spec:pgorm:sem:macros.derive.entity-model.column-def+3]
pub fn arr_type_match(
    arr_type: Option<TokenStream>,
    field_type: &Type,
    field_span: Span,
) -> TokenStream {
    if let Some(arr_type) = arr_type {
        return quote! { pgorm::pgorm_query::ArrayType::#arr_type };
    }
    match inferred_arr_type(field_type) {
        Some(arr_type) => quote! { pgorm::pgorm_query::ArrayType::#arr_type },
        // Assumed to be an ActiveEnum if none of the above types matched.
        None => quote_spanned! { field_span =>
            std::convert::Into::<pgorm::pgorm_query::ArrayType>::into(
                <#field_type as pgorm::pgorm_query::ValueType>::array_type()
            )
        },
    }
}

fn inferred_arr_type(field_type: &Type) -> Option<TokenStream> {
    if is_shared_str(field_type) {
        return Some(quote! { String });
    }
    Some(match bare_name(field_type)?.to_string().as_str() {
        "char" => quote! { Char },
        "String" => quote! { String },
        "i8" => quote! { TinyInt },
        "i16" => quote! { SmallInt },
        "i32" => quote! { Int },
        "u32" => quote! { Unsigned },
        "i64" => quote! { BigInt },
        "u64" => quote! { BigUnsigned },
        "f32" => quote! { Float },
        "f64" => quote! { Double },
        "bool" => quote! { Bool },
        "Date" | "NaiveDate" => quote! { ChronoDate },
        "Time" | "NaiveTime" => quote! { ChronoTime },
        "DateTime" | "NaiveDateTime" => quote! { ChronoDateTime },
        "DateTimeUtc" | "DateTimeLocal" | "DateTimeWithTimeZone" => {
            quote! { ChronoDateTimeWithTimeZone }
        }
        "Uuid" => quote! { Uuid },
        "Json" => quote! { Json },
        "Decimal" => quote! { Decimal },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn col(ty: &str) -> String {
        let ty = parse_str::<Type>(ty).expect("test type parses");
        col_type_match(None, &ty, Span::call_site()).to_string()
    }

    fn arr(ty: &str) -> String {
        let ty = parse_str::<Type>(ty).expect("test type parses");
        arr_type_match(None, &ty, Span::call_site()).to_string()
    }

    // [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    only bare `&str` is in the table
    #[test]
    fn shared_str_matches_without_a_lifetime() {
        assert!(col("&str").contains("ColumnType :: string"));
        assert!(arr("&str").contains("ArrayType :: String"));

        assert!(col("&'static str").contains("ValueType"));
        assert!(col("&mut str").contains("ValueType"));
    }

    // [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    the fallback keeps the written type
    #[test]
    fn fallback_reproduces_the_type_verbatim() {
        assert!(col("&'a str").contains("& 'a str"));
        assert!(col("Box<dyn Send>").contains("dyn Send"));
        assert!(col("<T as Trait>::Assoc").contains("< T as Trait > :: Assoc"));
    }

    // [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    `Vec<u8>` is the only byte row
    #[test]
    fn byte_vec_matches_only_vec_of_u8() {
        assert!(col("Vec<u8>").contains("Bytea"));
        assert!(col("Vec<u16>").contains("ValueType"));
        assert!(col("Vec<u8, A>").contains("ValueType"));
        assert!(arr("Vec<u8>").contains("ValueType"));
    }

    // [spec:pgorm:sem:macros.derive.entity-model.column-def+3/test]    `Option<T>` unwrapping is structural
    #[test]
    fn option_unwraps_only_when_bare() {
        let bare = parse_str::<Type>("Option<i64>").expect("test type parses");
        assert_eq!(
            unwrap_option(&bare).map(|ty| quote! { #ty }.to_string()),
            Some("i64".to_owned())
        );

        for ty in [
            "std::option::Option<i64>",
            "i64",
            "Option",
            "OptionLike<i64>",
        ] {
            let ty = parse_str::<Type>(ty).expect("test type parses");
            assert!(unwrap_option(&ty).is_none());
        }
    }
}
