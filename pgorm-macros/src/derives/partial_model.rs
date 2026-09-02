use heck::ToUpperCamelCase;
use proc_macro2::Span;
use proc_macro2::TokenStream;
use quote::format_ident;
use quote::quote;
use quote::quote_spanned;
use syn::Expr;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;

use syn::Meta;

use self::util::GetAsKVMeta;
use super::util::parse_derived_ident;

#[derive(Debug)]
enum Error {
    InputNotStruct,
    EntityNotSpecific,
    NotSupportGeneric(Span),
    BothFromColAndFromExpr(Span),
    Syn(syn::Error),
}
#[derive(Debug, PartialEq, Eq)]
enum ColumnAs {
    /// column in the model
    Col { entity: syn::Type, col: syn::Ident },
    /// alias from a column in model
    ColAlias {
        entity: syn::Type,
        col: syn::Ident,
        field: String,
    },
    /// from an expr
    Expr { expr: syn::Expr, field_name: String },
}

struct DerivePartialModel {
    ident: syn::Ident,
    fields: Vec<ColumnAs>,
}

impl DerivePartialModel {
    fn new(input: syn::DeriveInput) -> Result<Self, Error> {
        if !input.generics.params.is_empty() {
            return Err(Error::NotSupportGeneric(input.generics.params.span()));
        }

        let syn::Data::Struct(
            syn::DataStruct {
                fields: syn::Fields::Named(syn::FieldsNamed { named: fields, .. }),
                ..
            },
            ..,
        ) = input.data
        else {
            return Err(Error::InputNotStruct);
        };

        let mut entity = None;

        for attr in input.attrs.iter() {
            if !attr.path().is_ident("pgorm") {
                continue;
            }

            if let Ok(list) = attr.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated) {
                for meta in list {
                    entity = meta
                        .get_as_kv("entity")
                        .map(|s| syn::parse_str::<syn::Type>(&s).map_err(Error::Syn))
                        .transpose()?;
                }
            }
        }

        let mut column_as_list = Vec::with_capacity(fields.len());

        for field in fields {
            let field_span = field.span();
            let field_name = field.ident.ok_or(Error::InputNotStruct)?;

            let mut from_col = None;
            let mut from_expr = None;

            for attr in field.attrs.iter() {
                if !attr.path().is_ident("pgorm") {
                    continue;
                }

                if let Ok(list) = attr.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)
                {
                    for meta in list.iter() {
                        from_col = meta
                            .get_as_kv("from_col")
                            .map(|s| {
                                parse_derived_ident(&s.to_upper_camel_case(), &field_name)
                                    .map_err(Error::Syn)
                            })
                            .transpose()?;
                        from_expr = meta
                            .get_as_kv("from_expr")
                            .map(|s| syn::parse_str::<Expr>(&s).map_err(Error::Syn))
                            .transpose()?;
                    }
                }
            }

            let col_as = match (from_col, from_expr) {
                (None, None) => ColumnAs::Col {
                    entity: entity.clone().ok_or(Error::EntityNotSpecific)?,
                    col: parse_derived_ident(
                        &field_name.to_string().to_upper_camel_case(),
                        &field_name,
                    )
                    .map_err(Error::Syn)?,
                },
                (None, Some(expr)) => ColumnAs::Expr {
                    expr,
                    field_name: field_name.to_string(),
                },
                (Some(col), None) => ColumnAs::ColAlias {
                    entity: entity.clone().ok_or(Error::EntityNotSpecific)?,
                    col,
                    field: field_name.to_string(),
                },
                (Some(_), Some(_)) => return Err(Error::BothFromColAndFromExpr(field_span)),
            };
            column_as_list.push(col_as);
        }

        Ok(Self {
            ident: input.ident,
            fields: column_as_list,
        })
    }

    fn expand(&self) -> syn::Result<TokenStream> {
        Ok(self.impl_partial_model_trait())
    }

    fn impl_partial_model_trait(&self) -> TokenStream {
        let select_ident = format_ident!("select");
        let DerivePartialModel { ident, fields } = self;
        let select_col_code_gen = fields.iter().map(|col_as| match col_as {
            ColumnAs::Col { entity, col: ident } => {
                let col_value = quote!( <#entity as pgorm::EntityTrait>::Column:: #ident);
                quote!(let #select_ident =  pgorm::SelectColumns::select_column(#select_ident, #col_value);)
            },
            ColumnAs::ColAlias { entity, col, field } => {
                let col_value = quote!( <#entity as pgorm::EntityTrait>::Column:: #col);
                quote!(let #select_ident =  pgorm::SelectColumns::select_column_as(#select_ident, #col_value, #field);)
            },
            ColumnAs::Expr { expr, field_name } => {
                quote!(let #select_ident =  pgorm::SelectColumns::select_column_as(#select_ident, #expr, #field_name);)
            },
        });

        quote! {
            #[automatically_derived]
            impl pgorm::PartialModelTrait for #ident{
                fn select_cols<S: pgorm::SelectColumns>(#select_ident: S) -> S::Projected {
                    #(#select_col_code_gen)*
                    #select_ident
                }
            }
        }
    }
}

// [spec:pgorm:sem:macros.derive.partial-model+1]
pub fn expand_derive_partial_model(input: syn::DeriveInput) -> syn::Result<TokenStream> {
    let ident_span = input.ident.span();

    match DerivePartialModel::new(input) {
        Ok(partial_model) => partial_model.expand(),
        Err(Error::NotSupportGeneric(span)) => Ok(quote_spanned! {
            span => compile_error!("you can only derive `DerivePartialModel` on named struct");
        }),
        Err(Error::BothFromColAndFromExpr(span)) => Ok(quote_spanned! {
            span => compile_error!("you can only use one of `from_col` or `from_expr`");
        }),
        Err(Error::EntityNotSpecific) => Ok(quote_spanned! {
            ident_span => compile_error!("you need specific which entity you are using")
        }),
        Err(Error::InputNotStruct) => Ok(quote_spanned! {
            ident_span => compile_error!("you can only derive `DerivePartialModel` on named struct");
        }),
        Err(Error::Syn(err)) => Err(err),
    }
}

mod util {
    use syn::{Meta, MetaNameValue};

    pub(super) trait GetAsKVMeta {
        fn get_as_kv(&self, k: &str) -> Option<String>;
    }

    impl GetAsKVMeta for Meta {
        fn get_as_kv(&self, k: &str) -> Option<String> {
            let Meta::NameValue(MetaNameValue {
                path,
                value: syn::Expr::Lit(exprlit),
                ..
            }) = self
            else {
                return None;
            };

            let syn::Lit::Str(litstr) = &exprlit.lit else {
                return None;
            };

            if path.is_ident(k) {
                Some(litstr.value())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod test {
    use quote::format_ident;
    use syn::{DeriveInput, Type, parse_str};

    use crate::derives::partial_model::ColumnAs;

    use super::DerivePartialModel;

    #[cfg(test)]
    type StdResult<T> = Result<T, Box<dyn std::error::Error>>;

    #[cfg(test)]
    const CODE_SNIPPET: &str = r#"
#[pgorm(entity = "Entity")]
struct PartialModel{
    default_field: i32,
    #[pgorm(from_col = "bar")]
    alias_field: i32,
    #[pgorm(from_expr = "Expr::val(1).add(1)")]
    expr_field : i32
}
"#;
    // [spec:pgorm:sem:macros.derive.partial-model+1/test]
    #[test]
    fn test_load_macro_input() -> StdResult<()> {
        let input = parse_str::<DeriveInput>(CODE_SNIPPET)?;

        let middle = DerivePartialModel::new(input).unwrap();
        assert_eq!(middle.ident, format_ident!("PartialModel"));
        assert_eq!(middle.fields.len(), 3);
        assert_eq!(
            middle.fields[0],
            ColumnAs::Col {
                entity: parse_str::<Type>("Entity").unwrap(),
                col: format_ident!("DefaultField")
            }
        );
        assert_eq!(
            middle.fields[1],
            ColumnAs::ColAlias {
                entity: parse_str::<Type>("Entity").unwrap(),
                col: format_ident!("Bar"),
                field: "alias_field".to_string()
            },
        );
        assert_eq!(
            middle.fields[2],
            ColumnAs::Expr {
                expr: syn::parse_str("Expr::val(1).add(1)").unwrap(),
                field_name: "expr_field".to_string()
            }
        );

        Ok(())
    }
}
