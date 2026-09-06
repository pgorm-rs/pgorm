use crate::{
    DateTimeCrate, Error,
    util::{escape_rust_keyword, safe_ident},
};
use heck::{ToSnakeCase, ToUpperCamelCase};
use pgorm_query::{ColumnDef, ColumnSpec, ColumnType, StringLen};
use proc_macro2::{Ident, TokenStream};
use quote::{format_ident, quote};
use std::fmt::Write as FmtWrite;

#[derive(Clone, Debug)]
pub struct Column {
    pub(crate) name: String,
    pub(crate) col_type: ColumnType,
    pub(crate) auto_increment: bool,
    pub(crate) not_null: bool,
    pub(crate) unique: bool,
}

impl Column {
    /// Reject anything the writer could not render: a type outside the mapping
    /// table, and a DB name whose case-converted forms are not Rust
    /// identifiers.
    // [spec:pgorm:req:codegen.entity.types.unsupported+1]
    // [spec:pgorm:sem:codegen.entity.keywords+1]
    pub(crate) fn validate(&self) -> Result<(), Error> {
        let context = format!("column `{}`", self.name);
        validate_col_type(&context, &self.col_type)?;
        safe_ident(&context, &escape_rust_keyword(self.name.to_snake_case()))?;
        safe_ident(
            &context,
            &escape_rust_keyword(self.name.to_upper_camel_case()),
        )?;
        Ok(())
    }

    // [spec:pgorm:sem:codegen.entity.keywords+1]
    pub fn get_name_snake_case(&self) -> Ident {
        format_ident!("{}", escape_rust_keyword(self.name.to_snake_case()))
    }

    pub fn get_name_camel_case(&self) -> Ident {
        format_ident!("{}", escape_rust_keyword(self.name.to_upper_camel_case()))
    }

    pub fn is_snake_case_name(&self) -> bool {
        self.name.to_snake_case() == self.name
    }

    // [spec:pgorm:sem:codegen.entity.types+2]
    // [spec:pgorm:sem:codegen.entity.types.datetime+1]
    // [spec:pgorm:req:codegen.entity.types.unsupported+1]
    pub fn get_rs_type(&self, date_time_crate: &DateTimeCrate) -> TokenStream {
        fn write_rs_type(col_type: &ColumnType, date_time_crate: &DateTimeCrate) -> String {
            #[allow(unreachable_patterns)]
            match col_type {
                ColumnType::Char(_)
                | ColumnType::String(_)
                | ColumnType::Text
                | ColumnType::Custom(_) => "String".to_owned(),
                ColumnType::SmallInteger => "i16".to_owned(),
                ColumnType::Integer => "i32".to_owned(),
                ColumnType::BigInteger => "i64".to_owned(),
                ColumnType::Float => "f32".to_owned(),
                ColumnType::Double => "f64".to_owned(),
                ColumnType::Json | ColumnType::JsonBinary => "Json".to_owned(),
                ColumnType::Date => match date_time_crate {
                    DateTimeCrate::Chrono => "Date".to_owned(),
                    DateTimeCrate::Time => "TimeDate".to_owned(),
                },
                ColumnType::Time => match date_time_crate {
                    DateTimeCrate::Chrono => "Time".to_owned(),
                    DateTimeCrate::Time => "TimeTime".to_owned(),
                },
                ColumnType::Timestamp => match date_time_crate {
                    DateTimeCrate::Chrono => "DateTime".to_owned(),
                    DateTimeCrate::Time => "TimeDateTime".to_owned(),
                },
                ColumnType::TimestampWithTimeZone => match date_time_crate {
                    DateTimeCrate::Chrono => "DateTimeWithTimeZone".to_owned(),
                    DateTimeCrate::Time => "TimeDateTimeWithTimeZone".to_owned(),
                },
                ColumnType::Decimal(_) | ColumnType::Money => "Decimal".to_owned(),
                ColumnType::Uuid => "Uuid".to_owned(),
                ColumnType::Bytea => "Vec<u8>".to_owned(),
                ColumnType::Boolean => "bool".to_owned(),
                ColumnType::Enum { name, .. } => name.to_string().to_upper_camel_case(),
                ColumnType::Array(column_type) => {
                    format!("Vec<{}>", write_rs_type(column_type, date_time_crate))
                }
                other => unreachable!(
                    "column type {other:?} reached the writer; \
                     every Column is type-checked when it is built"
                ),
            }
        }
        let ident: TokenStream = write_rs_type(&self.col_type, date_time_crate)
            .parse()
            .expect("mapped Rust type names are token text");
        match self.not_null {
            true => quote! { #ident },
            false => quote! { Option<#ident> },
        }
    }

    // [spec:pgorm:sem:codegen.entity.compact.attrs+1]
    pub fn get_col_type_attrs(&self) -> Option<TokenStream> {
        let col_type = match &self.col_type {
            ColumnType::Float => Some("Float".to_owned()),
            ColumnType::Double => Some("Double".to_owned()),
            ColumnType::Decimal(Some((p, s))) => Some(format!("Decimal(Some(({p}, {s})))")),
            ColumnType::Money => Some("Money".to_owned()),
            ColumnType::Text => Some("Text".to_owned()),
            ColumnType::JsonBinary => Some("JsonBinary".to_owned()),
            ColumnType::Custom(iden) => Some(format!("custom(\"{}\")", iden.to_string())),
            ColumnType::Bytea => Some("Bytea".to_owned()),
            _ => None,
        };
        col_type.map(|ty| quote! { column_type = #ty })
    }

    // [spec:pgorm:req:codegen.entity.types.unsupported+1]
    pub fn get_def(&self) -> TokenStream {
        fn write_col_def(col_type: &ColumnType) -> TokenStream {
            match col_type {
                ColumnType::Char(s) => match s {
                    Some(s) => quote! { ColumnType::Char(Some(#s)) },
                    None => quote! { ColumnType::Char(None) },
                },
                ColumnType::String(s) => match s {
                    StringLen::N(s) => quote! { ColumnType::String(StringLen::N(#s)) },
                    StringLen::None => quote! { ColumnType::String(StringLen::None) },
                    StringLen::Max => quote! { ColumnType::String(StringLen::Max) },
                },
                ColumnType::Text => quote! { ColumnType::Text },
                ColumnType::SmallInteger => quote! { ColumnType::SmallInteger },
                ColumnType::Integer => quote! { ColumnType::Integer },
                ColumnType::BigInteger => quote! { ColumnType::BigInteger },
                ColumnType::Float => quote! { ColumnType::Float },
                ColumnType::Double => quote! { ColumnType::Double },
                ColumnType::Decimal(s) => match s {
                    Some((s1, s2)) => quote! { ColumnType::Decimal(Some((#s1, #s2))) },
                    None => quote! { ColumnType::Decimal(None) },
                },
                ColumnType::Timestamp => quote! { ColumnType::Timestamp },
                ColumnType::TimestampWithTimeZone => {
                    quote! { ColumnType::TimestampWithTimeZone }
                }
                ColumnType::Time => quote! { ColumnType::Time },
                ColumnType::Date => quote! { ColumnType::Date },
                ColumnType::Bytea => quote! { ColumnType::Bytea },
                ColumnType::Boolean => quote! { ColumnType::Boolean },
                ColumnType::Money => quote! { ColumnType::Money },
                ColumnType::Json => quote! { ColumnType::Json },
                ColumnType::JsonBinary => quote! { ColumnType::JsonBinary },
                ColumnType::Uuid => quote! { ColumnType::Uuid },
                ColumnType::Custom(s) => {
                    let s = s.to_string();
                    quote! { ColumnType::custom(#s) }
                }
                ColumnType::Enum { name, .. } => {
                    let enum_ident = format_ident!("{}", name.to_string().to_upper_camel_case());
                    quote! { #enum_ident::db_type() }
                }
                ColumnType::Array(column_type) => {
                    let column_type = write_col_def(column_type);
                    quote! { ColumnType::Array(Arc::new(#column_type)) }
                }
                #[allow(unreachable_patterns)]
                other => unreachable!(
                    "column type {other:?} reached the writer; \
                     every Column is type-checked when it is built"
                ),
            }
        }
        let mut col_def = write_col_def(&self.col_type);
        col_def.extend(quote! {
            .def()
        });
        if !self.not_null {
            col_def.extend(quote! {
                .null()
            });
        }
        if self.unique {
            col_def.extend(quote! {
                .unique()
            });
        }
        col_def
    }

    pub fn get_info(&self, date_time_crate: &DateTimeCrate) -> String {
        let mut info = String::new();
        let type_info = self
            .get_rs_type(date_time_crate)
            .to_string()
            .replace(' ', "");
        let col_info = self.col_info();
        write!(
            &mut info,
            "Column `{}`: {}{}",
            self.name, type_info, col_info
        )
        .unwrap();
        info
    }

    fn col_info(&self) -> String {
        let mut info = String::new();
        if self.auto_increment {
            write!(&mut info, ", auto_increment").unwrap();
        }
        if self.not_null {
            write!(&mut info, ", not_null").unwrap();
        }
        if self.unique {
            write!(&mut info, ", unique").unwrap();
        }
        info
    }

    // [spec:pgorm:sem:codegen.entity.serde.skip]
    pub fn get_serde_attribute(
        &self,
        is_primary_key: bool,
        serde_skip_deserializing_primary_key: bool,
        serde_skip_hidden_column: bool,
    ) -> TokenStream {
        if self.name.starts_with('_') && serde_skip_hidden_column {
            quote! {
                #[serde(skip)]
            }
        } else if serde_skip_deserializing_primary_key && is_primary_key {
            quote! {
                #[serde(skip_deserializing)]
            }
        } else {
            quote! {}
        }
    }

    pub fn get_inner_col_type(&self) -> &ColumnType {
        match &self.col_type {
            ColumnType::Array(inner_col_type) => inner_col_type.as_ref(),
            _ => &self.col_type,
        }
    }
}

/// The set of `ColumnType`s `get_rs_type` and `get_def` can render, checked
/// through `Array` element types and over the enum names they will emit.
// [spec:pgorm:req:codegen.entity.types.unsupported+1]
fn validate_col_type(context: &str, col_type: &ColumnType) -> Result<(), Error> {
    match col_type {
        ColumnType::Char(_)
        | ColumnType::String(_)
        | ColumnType::Text
        | ColumnType::Custom(_)
        | ColumnType::SmallInteger
        | ColumnType::Integer
        | ColumnType::BigInteger
        | ColumnType::Float
        | ColumnType::Double
        | ColumnType::Json
        | ColumnType::JsonBinary
        | ColumnType::Date
        | ColumnType::Time
        | ColumnType::Timestamp
        | ColumnType::TimestampWithTimeZone
        | ColumnType::Decimal(_)
        | ColumnType::Money
        | ColumnType::Uuid
        | ColumnType::Bytea
        | ColumnType::Boolean => Ok(()),
        ColumnType::Enum { name, .. } => {
            safe_ident(context, &name.to_string().to_upper_camel_case())?;
            Ok(())
        }
        ColumnType::Array(inner_col_type) => validate_col_type(context, inner_col_type),
        other => Err(Error::TransformError(format!(
            "{context}: column type {other:?} is not supported by codegen"
        ))),
    }
}

impl TryFrom<ColumnDef> for Column {
    type Error = Error;

    fn try_from(col_def: ColumnDef) -> Result<Self, Self::Error> {
        Self::try_from(&col_def)
    }
}

// [spec:pgorm:sem:codegen.entity.transform+7]
impl TryFrom<&ColumnDef> for Column {
    type Error = Error;

    fn try_from(col_def: &ColumnDef) -> Result<Self, Self::Error> {
        let name = col_def.get_column_name();
        let Some(col_type) = col_def.get_column_type().cloned() else {
            return Err(Error::TransformError(format!(
                "column `{name}`: column type should not be empty"
            )));
        };
        let auto_increment = col_def
            .get_column_spec()
            .iter()
            .any(|spec| matches!(spec, ColumnSpec::AutoIncrement));
        let not_null = col_def
            .get_column_spec()
            .iter()
            .any(|spec| matches!(spec, ColumnSpec::NotNull));
        let unique = col_def
            .get_column_spec()
            .iter()
            .any(|spec| matches!(spec, ColumnSpec::UniqueKey));
        let column = Self {
            name,
            col_type,
            auto_increment,
            not_null,
            unique,
        };
        column.validate()?;
        Ok(column)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Column, DateTimeCrate};
    use pgorm_query::{Alias, ColumnDef, ColumnType, SharedIden, StringLen};
    use proc_macro2::TokenStream;
    use quote::quote;

    fn to_column(col_def: ColumnDef) -> Column {
        Column::try_from(col_def).expect("column def should convert")
    }

    fn setup() -> Vec<Column> {
        macro_rules! make_col {
            ($name:expr, $col_type:expr) => {
                Column {
                    name: $name.to_owned(),
                    col_type: $col_type,
                    auto_increment: false,
                    not_null: false,
                    unique: false,
                }
            };
        }
        vec![
            make_col!("id", ColumnType::String(StringLen::N(255))),
            make_col!("id", ColumnType::String(StringLen::None)),
            make_col!(
                "cake_id",
                ColumnType::Custom(SharedIden::new(Alias::new("cus_col")))
            ),
            make_col!("CakeId", ColumnType::SmallInteger),
            make_col!("CakeId", ColumnType::Integer),
            make_col!("CakeFillingId", ColumnType::BigInteger),
            make_col!("cake-filling-id", ColumnType::Float),
            make_col!("CAKE_FILLING_ID", ColumnType::Double),
            make_col!("CAKE-FILLING-ID", ColumnType::Bytea),
            make_col!("CAKE", ColumnType::Boolean),
            make_col!("date", ColumnType::Date),
            make_col!("time", ColumnType::Time),
            make_col!("timestamp", ColumnType::Timestamp),
            make_col!("timestamp_tz", ColumnType::TimestampWithTimeZone),
        ]
    }

    #[test]
    fn test_get_name_snake_case() {
        let columns = setup();
        let snack_cases = vec![
            "id",
            "id",
            "cake_id",
            "cake_id",
            "cake_id",
            "cake_filling_id",
            "cake_filling_id",
            "cake_filling_id",
            "cake_filling_id",
            "cake",
            "date",
            "time",
            "timestamp",
            "timestamp_tz",
        ];
        for (col, snack_case) in columns.into_iter().zip(snack_cases) {
            assert_eq!(col.get_name_snake_case().to_string(), snack_case);
        }
    }

    #[test]
    fn test_get_name_camel_case() {
        let columns = setup();
        let camel_cases = vec![
            "Id",
            "Id",
            "CakeId",
            "CakeId",
            "CakeId",
            "CakeFillingId",
            "CakeFillingId",
            "CakeFillingId",
            "CakeFillingId",
            "Cake",
            "Date",
            "Time",
            "Timestamp",
            "TimestampTz",
        ];
        for (col, camel_case) in columns.into_iter().zip(camel_cases) {
            assert_eq!(col.get_name_camel_case().to_string(), camel_case);
        }
    }

    #[test]
    fn test_get_rs_type_with_chrono() {
        let columns = setup();
        let chrono_crate = DateTimeCrate::Chrono;
        let rs_types = vec![
            "String",
            "String",
            "String",
            "i16",
            "i32",
            "i64",
            "f32",
            "f64",
            "Vec<u8>",
            "bool",
            "Date",
            "Time",
            "DateTime",
            "DateTimeWithTimeZone",
        ];
        for (mut col, rs_type) in columns.into_iter().zip(rs_types) {
            let rs_type: TokenStream = rs_type.parse().unwrap();

            col.not_null = true;
            assert_eq!(
                col.get_rs_type(&chrono_crate).to_string(),
                quote!(#rs_type).to_string()
            );

            col.not_null = false;
            assert_eq!(
                col.get_rs_type(&chrono_crate).to_string(),
                quote!(Option<#rs_type>).to_string()
            );
        }
    }

    #[test]
    fn test_get_rs_type_with_time() {
        let columns = setup();
        let time_crate = DateTimeCrate::Time;
        let rs_types = vec![
            "String",
            "String",
            "String",
            "i16",
            "i32",
            "i64",
            "f32",
            "f64",
            "Vec<u8>",
            "bool",
            "TimeDate",
            "TimeTime",
            "TimeDateTime",
            "TimeDateTimeWithTimeZone",
        ];
        for (mut col, rs_type) in columns.into_iter().zip(rs_types) {
            let rs_type: TokenStream = rs_type.parse().unwrap();

            col.not_null = true;
            assert_eq!(
                col.get_rs_type(&time_crate).to_string(),
                quote!(#rs_type).to_string()
            );

            col.not_null = false;
            assert_eq!(
                col.get_rs_type(&time_crate).to_string(),
                quote!(Option<#rs_type>).to_string()
            );
        }
    }

    #[test]
    fn test_get_def() {
        let columns = setup();
        let col_defs = vec![
            "ColumnType::String(StringLen::N(255u32)).def()",
            "ColumnType::String(StringLen::None).def()",
            "ColumnType::custom(\"cus_col\").def()",
            "ColumnType::SmallInteger.def()",
            "ColumnType::Integer.def()",
            "ColumnType::BigInteger.def()",
            "ColumnType::Float.def()",
            "ColumnType::Double.def()",
            "ColumnType::Bytea.def()",
            "ColumnType::Boolean.def()",
            "ColumnType::Date.def()",
            "ColumnType::Time.def()",
            "ColumnType::Timestamp.def()",
            "ColumnType::TimestampWithTimeZone.def()",
        ];
        for (mut col, col_def) in columns.into_iter().zip(col_defs) {
            let mut col_def: TokenStream = col_def.parse().unwrap();

            col.not_null = true;
            assert_eq!(col.get_def().to_string(), col_def.to_string());

            col.not_null = false;
            col_def.extend(quote!(.null()));
            assert_eq!(col.get_def().to_string(), col_def.to_string());

            col.unique = true;
            col_def.extend(quote!(.unique()));
            assert_eq!(col.get_def().to_string(), col_def.to_string());
        }
    }

    #[test]
    fn test_get_info() {
        let column = to_column(ColumnDef::new(Alias::new("id")).string().to_owned());
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `id`: Option<String>"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `id`: String, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .not_null()
                .unique_key()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `id`: String, not_null, unique"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .not_null()
                .unique_key()
                .auto_increment()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `id`: String, auto_increment, not_null, unique"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("date_field"))
                .date()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `date_field`: Date, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("date_field"))
                .date()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Time).as_str(),
            "Column `date_field`: TimeDate, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("time_field"))
                .time()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `time_field`: Time, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("time_field"))
                .time()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Time).as_str(),
            "Column `time_field`: TimeTime, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("timestamp_field"))
                .timestamp()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `timestamp_field`: DateTime, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("timestamp_field"))
                .timestamp()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Time).as_str(),
            "Column `timestamp_field`: TimeDateTime, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("timestamp_with_timezone_field"))
                .timestamp_with_time_zone()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Chrono).as_str(),
            "Column `timestamp_with_timezone_field`: DateTimeWithTimeZone, not_null"
        );

        let column = to_column(
            ColumnDef::new(Alias::new("timestamp_with_timezone_field"))
                .timestamp_with_time_zone()
                .not_null()
                .to_owned(),
        );
        assert_eq!(
            column.get_info(&DateTimeCrate::Time).as_str(),
            "Column `timestamp_with_timezone_field`: TimeDateTimeWithTimeZone, not_null"
        );
    }

    #[test]
    fn test_from_column_def() {
        let column = to_column(ColumnDef::new(Alias::new("id")).string().to_owned());
        assert_eq!(
            column.get_def().to_string(),
            quote! {
                ColumnType::String(StringLen::None).def().null()
            }
            .to_string()
        );

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .not_null()
                .to_owned(),
        );
        assert!(column.not_null);

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .unique_key()
                .not_null()
                .to_owned(),
        );
        assert!(column.unique);
        assert!(column.not_null);

        let column = to_column(
            ColumnDef::new(Alias::new("id"))
                .string()
                .auto_increment()
                .unique_key()
                .not_null()
                .to_owned(),
        );
        assert!(column.auto_increment);
        assert!(column.unique);
        assert!(column.not_null);
    }
}
