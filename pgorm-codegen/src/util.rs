use crate::Error;
use pgorm_query::TableRef;
use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::format_ident;

/// `format_ident!` panics on anything that is not a legal Rust identifier, so
/// every DB-derived name is put through here while the transform gate can still
/// return the failure. `context` names where the identifier came from.
// [spec:pgorm:sem:codegen.entity.keywords+1]
pub(crate) fn safe_ident(context: &str, raw: &str) -> Result<Ident, Error> {
    if is_ident(raw) {
        Ok(format_ident!("{}", raw))
    } else {
        Err(Error::TransformError(format!(
            "{context}: `{raw}` is not a valid Rust identifier"
        )))
    }
}

/// True when `raw` lexes as exactly one identifier token and nothing else —
/// the same shape `format_ident!` accepts, raw identifiers (`r#type`) included.
fn is_ident(raw: &str) -> bool {
    let Ok(stream) = raw.parse::<TokenStream>() else {
        return false;
    };
    let mut tokens = stream.into_iter();
    let one_ident = matches!(tokens.next(), Some(TokenTree::Ident(ident)) if ident == raw);
    one_ident && tokens.next().is_none()
}

// [spec:pgorm:sem:codegen.entity.keywords+1]
pub(crate) fn escape_rust_keyword<T>(string: T) -> String
where
    T: ToString,
{
    let string = string.to_string();
    if RUST_KEYWORDS.iter().any(|s| s.eq(&string)) {
        format!("r#{string}")
    } else if RUST_SPECIAL_KEYWORDS.iter().any(|s| s.eq(&string)) {
        format!("{string}_")
    } else {
        string
    }
}

pub(crate) const RUST_KEYWORDS: [&str; 49] = [
    "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "static", "struct", "super", "trait", "true", "type", "union", "unsafe", "use",
    "where", "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv",
    "try", "typeof", "unsized", "virtual", "yield",
];

pub(crate) const RUST_SPECIAL_KEYWORDS: [&str; 3] = ["crate", "Self", "self"];

pub(crate) fn unpack_table_ref(table_ref: &TableRef) -> String {
    match table_ref {
        TableRef::Table(tbl)
        | TableRef::SchemaTable(_, tbl)
        | TableRef::DatabaseSchemaTable(_, _, tbl)
        | TableRef::TableAlias(tbl, _)
        | TableRef::SchemaTableAlias(_, tbl, _)
        | TableRef::DatabaseSchemaTableAlias(_, _, tbl, _)
        | TableRef::SubQuery(_, tbl)
        | TableRef::ValuesList(_, tbl)
        | TableRef::FunctionCall(_, tbl) => tbl.to_string(),
    }
}
