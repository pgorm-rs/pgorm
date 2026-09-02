use syn::parse::ParseStream;

/// The value carried by a single `#[pgorm(...)]` argument.
pub trait AttributeValue: Sized {
    fn parse_attribute_value(input: ParseStream<'_>) -> syn::Result<Self>;
}

impl AttributeValue for syn::Ident {
    fn parse_attribute_value(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<syn::Token![=]>()?;
        input.parse()
    }
}

impl AttributeValue for syn::Lit {
    fn parse_attribute_value(input: ParseStream<'_>) -> syn::Result<Self> {
        input.parse::<syn::Token![=]>()?;
        input.parse()
    }
}

/// A switch, i.e. an argument that is either present or absent and takes no value.
impl AttributeValue for () {
    fn parse_attribute_value(_input: ParseStream<'_>) -> syn::Result<Self> {
        Ok(())
    }
}

/// Declares a struct of optional `#[pgorm(...)]` arguments together with its parser.
///
/// Arguments are separated by an optional comma, may appear in any order, and the last
/// occurrence of a repeated argument wins. Only the first `#[pgorm(...)]` attribute of an
/// item is read.
macro_rules! from_attributes {
    (
        $(#[$struct_meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident: Option<$ty:ty>,
            )*
        }
    ) => {
        $(#[$struct_meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: Option<$ty>,
            )*
        }

        impl $name {
            pub fn try_from_attributes(attrs: &[syn::Attribute]) -> syn::Result<Option<Self>> {
                for attr in attrs {
                    if attr.path().is_ident("pgorm") {
                        return Some(attr.parse_args::<Self>()).transpose();
                    }
                }

                Ok(None)
            }

            #[allow(dead_code)]
            pub fn from_attributes(attrs: &[syn::Attribute]) -> syn::Result<Self> {
                match Self::try_from_attributes(attrs)? {
                    Some(attr) => Ok(attr),
                    None => Err(syn::Error::new(
                        proc_macro2::Span::call_site(),
                        "missing attribute `#[pgorm]`",
                    )),
                }
            }
        }

        impl syn::parse::Parse for $name {
            fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
                use $crate::derives::attributes::AttributeValue;

                $( let mut $field = None; )*

                while !input.is_empty() {
                    let arg = input.parse::<syn::Ident>()?;

                    match arg.to_string().as_str() {
                        $(
                            stringify!($field) => {
                                $field = Some(<$ty>::parse_attribute_value(input)?);
                            }
                        )*
                        other => {
                            let mut supported = [$( concat!("`", stringify!($field), "`") ),*];
                            supported.sort_unstable();

                            return Err(syn::Error::new(
                                arg.span(),
                                format!(
                                    "`#[pgorm]` got unknown `{}` argument. Supported arguments are {}",
                                    other,
                                    supported.join(", "),
                                ),
                            ));
                        }
                    }

                    input.parse::<syn::Token![,]>().ok();
                }

                Ok(Self { $($field,)* })
            }
        }
    };
}

pub(crate) use from_attributes;

pub mod derive_attr {
    from_attributes! {
        /// Attributes for Models and ActiveModels
        #[derive(Default)]
        #[allow(dead_code)]
        pub struct Pgorm {
            pub column: Option<syn::Ident>,
            pub entity: Option<syn::Ident>,
            pub model: Option<syn::Ident>,
            pub active_model: Option<syn::Ident>,
            pub primary_key: Option<syn::Ident>,
            pub relation: Option<syn::Ident>,
            pub schema_name: Option<syn::Lit>,
            pub table_name: Option<syn::Lit>,
            pub comment: Option<syn::Lit>,
            pub table_iden: Option<()>,
            pub rename_all: Option<syn::Lit>,
        }
    }
}

pub mod field_attr {
    from_attributes! {
        /// Operations for Models and ActiveModels
        #[derive(Default)]
        pub struct Pgorm {
            pub belongs_to: Option<syn::Lit>,
            pub has_one: Option<syn::Lit>,
            pub has_many: Option<syn::Lit>,
            pub on_update: Option<syn::Lit>,
            pub on_delete: Option<syn::Lit>,
            pub on_condition: Option<syn::Lit>,
            pub from: Option<syn::Lit>,
            pub to: Option<syn::Lit>,
            pub fk_name: Option<syn::Lit>,
            pub condition_type: Option<syn::Lit>,
        }
    }
}

pub mod related_attr {
    from_attributes! {
        /// Operations for RelatedEntity enumeration
        #[derive(Default)]
        pub struct Pgorm {
            ///
            /// Allows to modify target entity
            ///
            /// Required on enumeration variants
            ///
            /// If used on enumeration attributes
            /// it allows to specify different
            /// Entity ident
            pub entity: Option<syn::Lit>,
            ///
            /// Allows to specify RelationDef
            ///
            /// Optional
            ///
            /// If not supplied the generated code
            /// will utilize `impl Related` trait
            pub def: Option<syn::Lit>,
        }
    }
}
