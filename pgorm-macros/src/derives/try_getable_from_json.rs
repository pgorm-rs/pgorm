use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub fn expand_derive_from_json_query_result(ident: Ident) -> syn::Result<TokenStream> {
    let impl_not_u8 = quote!(
        #[automatically_derived]
        impl pgorm::pgorm_query::value::with_array::NotU8 for #ident {}
    );

    Ok(quote!(
        #[automatically_derived]
        impl pgorm::TryGetableFromJson for #ident {}

        #[automatically_derived]
        impl std::convert::From<#ident> for pgorm::Value {
            fn from(source: #ident) -> Self {
                pgorm::Value::Json(serde_json::to_value(&source).ok().map(|s| std::boxed::Box::new(s)))
            }
        }

        #[automatically_derived]
        impl pgorm::pgorm_query::ValueType for #ident {
            fn try_from(v: pgorm::Value) -> Result<Self, pgorm::pgorm_query::ValueTypeErr> {
                match v {
                    pgorm::Value::Json(Some(json)) => Ok(
                        serde_json::from_value(*json).map_err(|_| pgorm::pgorm_query::ValueTypeErr)?,
                    ),
                    _ => Err(pgorm::pgorm_query::ValueTypeErr),
                }
            }

            fn type_name() -> String {
                stringify!(#ident).to_owned()
            }

            fn array_type() -> pgorm::pgorm_query::ArrayType {
                pgorm::pgorm_query::ArrayType::Json
            }

            fn column_type() -> pgorm::pgorm_query::ColumnType {
                pgorm::pgorm_query::ColumnType::Json
            }
        }

        #[automatically_derived]
        impl pgorm::pgorm_query::Nullable for #ident {
            fn null() -> pgorm::Value {
                pgorm::Value::Json(None)
            }
        }

        #impl_not_u8
    ))
}
