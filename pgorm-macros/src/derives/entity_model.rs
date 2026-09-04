use super::case_style::{CaseStyle, CaseStyleHelpers};
use super::sql_type_match::unwrap_option;
use super::util::{
    column_variant_ident, escape_rust_keyword, parse_derived_ident, trim_starting_raw_identifier,
};
use heck::{ToSnakeCase, ToUpperCamelCase};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use syn::{
    Attribute, Data, Expr, Field, Fields, Lit, Type, punctuated::Punctuated, spanned::Spanned,
    token::Comma,
};

/// The field-level `#[pgorm(...)]` configuration of one model field.
#[derive(Default)]
struct FieldAttrs {
    sql_type: Option<TokenStream>,
    column_name: Option<String>,
    enum_name: Option<Ident>,
    comment: Option<Lit>,
    default_value: Option<Lit>,
    default_expr: Option<TokenStream>,
    select_as: Option<String>,
    save_as: Option<String>,
    is_primary_key: bool,
    nullable: bool,
    indexed: bool,
    unique: bool,
    ignore: bool,
}

/// `column_name` carries the name derived before any attribute is read, which an
/// explicit `column_name` key overrides. `auto_increment` and `primary_key_types`
/// are entity-wide and accumulate across every field.
// [spec:pgorm:syn:macros.derive.entity-model.attrs]
fn parse_field_attrs(
    field: &Field,
    column_name: Option<String>,
    auto_increment: &mut bool,
    primary_key_types: &mut Punctuated<Type, Comma>,
) -> syn::Result<FieldAttrs> {
    let mut parsed = FieldAttrs {
        column_name,
        ..Default::default()
    };

    // search for #[pgorm(primary_key, auto_increment = false, column_type = "String(Some(255))", default_value = "new user", default_expr = "gen_random_uuid()", column_name = "name", enum_name = "Name", nullable, indexed, unique)]
    for attr in field.attrs.iter() {
        if !attr.path().is_ident("pgorm") {
            continue;
        }

        // single param
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("column_type") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    let ty: TokenStream = syn::parse_str(&litstr.value())?;
                    parsed.sql_type = Some(ty);
                } else {
                    return Err(meta.error(format!("Invalid column_type {:?}", lit)));
                }
            } else if meta.path.is_ident("auto_increment") {
                let lit = meta.value()?.parse()?;
                if let Lit::Bool(litbool) = lit {
                    *auto_increment = litbool.value();
                } else {
                    return Err(meta.error(format!("Invalid auto_increment = {:?}", lit)));
                }
            } else if meta.path.is_ident("comment") {
                parsed.comment = Some(meta.value()?.parse::<Lit>()?);
            } else if meta.path.is_ident("default_value") {
                parsed.default_value = Some(meta.value()?.parse::<Lit>()?);
            } else if meta.path.is_ident("default_expr") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    let value_expr: TokenStream = syn::parse_str(&litstr.value())?;
                    parsed.default_expr = Some(value_expr);
                } else {
                    return Err(meta.error(format!("Invalid column_type {:?}", lit)));
                }
            } else if meta.path.is_ident("column_name") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    parsed.column_name = Some(litstr.value());
                } else {
                    return Err(meta.error(format!("Invalid column_name {:?}", lit)));
                }
            } else if meta.path.is_ident("enum_name") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    let ty: Ident = syn::parse_str(&litstr.value())?;
                    parsed.enum_name = Some(ty);
                } else {
                    return Err(meta.error(format!("Invalid enum_name {:?}", lit)));
                }
            } else if meta.path.is_ident("select_as") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    parsed.select_as = Some(litstr.value());
                } else {
                    return Err(meta.error(format!("Invalid select_as {:?}", lit)));
                }
            } else if meta.path.is_ident("save_as") {
                let lit = meta.value()?.parse()?;
                if let Lit::Str(litstr) = lit {
                    parsed.save_as = Some(litstr.value());
                } else {
                    return Err(meta.error(format!("Invalid save_as {:?}", lit)));
                }
            } else if meta.path.is_ident("ignore") {
                parsed.ignore = true;
            } else if meta.path.is_ident("primary_key") {
                parsed.is_primary_key = true;
                primary_key_types.push(field.ty.clone());
            } else if meta.path.is_ident("nullable") {
                parsed.nullable = true;
            } else if meta.path.is_ident("indexed") {
                parsed.indexed = true;
            } else if meta.path.is_ident("unique") {
                parsed.unique = true;
            } else {
                // Reads the value expression to advance the parse stream.
                // Some parameters, such as `primary_key`, do not have any value,
                // so ignoring an error occurred here.
                let _: Option<Expr> = meta.value().and_then(|v| v.parse()).ok();
            }

            Ok(())
        })?;
    }

    Ok(parsed)
}

/// Method to derive an Model
// [spec:pgorm:sem:macros.derive.entity-model]
// [spec:pgorm:syn:macros.derive.entity-model.attrs]
// [spec:pgorm:sem:macros.derive.entity-model.casing+1]
// [spec:pgorm:sem:macros.derive.entity-model.column-def+3]
// [spec:pgorm:sem:macros.derive.entity-model.primary-key+1]
pub fn expand_derive_entity_model(data: Data, attrs: Vec<Attribute>) -> syn::Result<TokenStream> {
    // if #[pgorm(table_name = "foo", schema_name = "bar")] specified, create Entity struct
    let mut table_name = None;
    let mut comment = quote! {None};
    let mut schema_name = quote! { None };
    let mut table_iden = false;
    let mut rename_all: Option<CaseStyle> = None;

    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("pgorm"))
        .try_for_each(|attr| {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("comment") {
                    let name: Lit = meta.value()?.parse()?;
                    comment = quote! { Some(#name) };
                } else if meta.path.is_ident("table_name") {
                    table_name = Some(meta.value()?.parse::<Lit>()?);
                } else if meta.path.is_ident("schema_name") {
                    let name: Lit = meta.value()?.parse()?;
                    schema_name = quote! { Some(#name) };
                } else if meta.path.is_ident("table_iden") {
                    table_iden = true;
                } else if meta.path.is_ident("rename_all") {
                    rename_all = Some((&meta).try_into()?);
                } else {
                    // Reads the value expression to advance the parse stream.
                    // Some parameters, such as `primary_key`, do not have any value,
                    // so ignoring an error occurred here.
                    let _: Option<Expr> = meta.value().and_then(|v| v.parse()).ok();
                }

                Ok(())
            })
        })?;

    let entity_def = table_name
        .as_ref()
        .map(|table_name| {
            quote! {
                #[doc = " Generated by pgorm-macros"]
                #[derive(Copy, Clone, Default, Debug, pgorm::prelude::DeriveEntity)]
                pub struct Entity;

                #[automatically_derived]
                impl pgorm::prelude::EntityName for Entity {
                    fn schema_name(&self) -> Option<&str> {
                        #schema_name
                    }

                    fn table_name(&self) -> &str {
                        #table_name
                    }

                    fn comment(&self) -> Option<&str> {
                        #comment
                    }
                }
            }
        })
        .unwrap_or_default();

    // generate Column enum and it's ColumnTrait impl
    let mut columns_enum: Punctuated<_, Comma> = Punctuated::new();
    let mut columns_trait: Punctuated<_, Comma> = Punctuated::new();
    let mut columns_select_as: Punctuated<_, Comma> = Punctuated::new();
    let mut columns_save_as: Punctuated<_, Comma> = Punctuated::new();
    let mut primary_keys: Punctuated<_, Comma> = Punctuated::new();
    let mut primary_key_types: Punctuated<Type, Comma> = Punctuated::new();
    let mut auto_increment = true;
    if table_iden && let Some(table_name) = table_name {
        let table_field_name = Ident::new("Table", Span::call_site());
        columns_enum.push(quote! {
            #[doc = " Generated by pgorm-macros"]
            #[pgorm(table_name=#table_name)]
            #[strum(disabled)]
            #table_field_name
        });
        columns_trait
            .push(quote! { Self::#table_field_name => panic!("Table cannot be used as a column") });
    }
    if let Data::Struct(item_struct) = data
        && let Fields::Named(fields) = item_struct.fields
    {
        for field in fields.named {
            if let Some(ident) = &field.ident {
                let original_field_name = trim_starting_raw_identifier(ident);
                let mut field_name =
                    column_variant_ident(&original_field_name.to_upper_camel_case(), ident)?;

                let derived_column_name = if let Some(case_style) = rename_all {
                    Some(field_name.convert_case(Some(case_style)))
                } else if original_field_name
                    != original_field_name.to_upper_camel_case().to_snake_case()
                {
                    // `to_snake_case` was used to trim prefix and tailing underscore
                    Some(original_field_name.to_snake_case())
                } else {
                    None
                };

                let FieldAttrs {
                    sql_type,
                    column_name,
                    enum_name,
                    comment,
                    default_value,
                    default_expr,
                    select_as,
                    save_as,
                    is_primary_key,
                    mut nullable,
                    indexed,
                    unique,
                    ignore,
                } = parse_field_attrs(
                    &field,
                    derived_column_name,
                    &mut auto_increment,
                    &mut primary_key_types,
                )?;

                if let Some(enum_name) = enum_name {
                    field_name = enum_name;
                }

                field_name = parse_derived_ident(&escape_rust_keyword(&field_name), ident)?;

                let variant_attrs = match &column_name {
                    Some(column_name) => quote! {
                        #[pgorm(column_name = #column_name)]
                        #[doc = " Generated by pgorm-macros"]
                    },
                    None => quote! {
                        #[doc = " Generated by pgorm-macros"]
                    },
                };

                if ignore {
                    continue;
                } else {
                    columns_enum.push(quote! {
                        #variant_attrs
                        #field_name
                    });
                }

                if is_primary_key {
                    primary_keys.push(quote! {
                        #variant_attrs
                        #field_name
                    });
                }

                if let Some(select_as) = select_as {
                    columns_select_as.push(quote! {
                            Self::#field_name => expr.cast_as(pgorm::pgorm_query::Alias::new(#select_as))
                        });
                }
                if let Some(save_as) = save_as {
                    columns_save_as.push(quote! {
                        Self::#field_name => val.cast_as(pgorm::pgorm_query::Alias::new(#save_as))
                    });
                }

                let field_type = match unwrap_option(&field.ty) {
                    Some(inner) => {
                        nullable = true;
                        inner
                    }
                    None => &field.ty,
                };
                let field_span = field.span();

                let pgorm_query_col_type = crate::derives::sql_type_match::col_type_match(
                    sql_type, field_type, field_span,
                );

                let col_def =
                    quote! { pgorm::prelude::ColumnTypeTrait::def(#pgorm_query_col_type) };

                let mut match_row = quote! { Self::#field_name => #col_def };
                if nullable {
                    match_row = quote! { #match_row.nullable() };
                }
                if indexed {
                    match_row = quote! { #match_row.indexed() };
                }
                if unique {
                    match_row = quote! { #match_row.unique() };
                }
                if let Some(default_value) = default_value {
                    match_row = quote! { #match_row.default_value(#default_value) };
                }
                if let Some(comment) = comment {
                    match_row = quote! { #match_row.comment(#comment) };
                }
                if let Some(default_expr) = default_expr {
                    match_row = quote! { #match_row.default(#default_expr) };
                }
                // match_row = quote! { #match_row.comment() };
                columns_trait.push(match_row);
            }
        }
    }

    // Add tailing comma
    if !columns_select_as.is_empty() {
        columns_select_as.push_punct(Comma::default());
    }
    if !columns_save_as.is_empty() {
        columns_save_as.push_punct(Comma::default());
    }

    let primary_key = {
        let auto_increment = auto_increment && primary_keys.len() == 1;
        let primary_key_types = if primary_key_types.len() == 1 {
            let first = primary_key_types.first();
            quote! { #first }
        } else {
            quote! { (#primary_key_types) }
        };
        quote! {
            #[doc = " Generated by pgorm-macros"]
            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                #primary_keys
            }

            #[automatically_derived]
            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = #primary_key_types;

                fn auto_increment() -> bool {
                    #auto_increment
                }
            }
        }
    };

    Ok(quote! {
        #[doc = " Generated by pgorm-macros"]
        #[derive(Copy, Clone, Debug, pgorm::prelude::EnumIter, pgorm::prelude::DeriveColumn)]
        pub enum Column {
            #columns_enum
        }

        #[automatically_derived]
        impl pgorm::prelude::ColumnTrait for Column {
            type EntityName = Entity;

            fn def(&self) -> pgorm::prelude::ColumnDef {
                match self {
                    #columns_trait
                }
            }

            fn select_as(&self, expr: pgorm::pgorm_query::Expr) -> pgorm::pgorm_query::SimpleExpr {
                match self {
                    #columns_select_as
                    _ => pgorm::prelude::ColumnTrait::select_enum_as(self, expr),
                }
            }

            fn save_as(&self, val: pgorm::pgorm_query::Expr) -> pgorm::pgorm_query::SimpleExpr {
                match self {
                    #columns_save_as
                    _ => pgorm::prelude::ColumnTrait::save_enum_as(self, val),
                }
            }
        }

        #entity_def

        #primary_key
    })
}
